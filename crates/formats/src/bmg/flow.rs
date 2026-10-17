//! FLW1 and FLI1: the nodes that display a message, branch on something the
//! game knows, or fire an event, each pointing at what comes next.
//!
//! ```text
//! FLW1  header    node record count, table entry count
//!       nodes     one 8 byte record per node
//!       table     u16 node positions, for the edges a record has no room for
//!       mask      one byte per table entry, 0xFF for a dead end
//! FLI1  header    root count, and the entry width
//!       roots     one 8 byte entry per root: a flow id and its node
//! ```
//!
//! A branch owns a run of table entries, one per answer, and an event owns
//! one for its next edge. A text node keeps its next edge in its record. A
//! dead end is 0xFFFF wherever the edge is.
//!
//! The game never reads the mask, but every file has one, so it's checked
//! against the table on decode. Each FLW1 array is padded to 16 bytes, and
//! the header states the padded counts.

use std::collections::HashMap;

use super::{MessageId, malformed, positions, too_large};
use crate::{Be16, ByteReader, ByteWriter, Record, Result, record};

/// Stable handle for a node, held by edges and roots. Assigned on decode as
/// the node's position, and meaningless after that, so inserting or removing
/// a node can't repoint an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Displays a message, then carries on.
    Text {
        id: NodeId,
        message: MessageId,
        /// `None` ends the conversation.
        next: Option<NodeId>,
    },
    /// Asks the game something and takes one of several ways out.
    Branch {
        id: NodeId,
        /// An index into the game's table of queries.
        query: u16,
        param: u16,
        /// Where each answer goes, in the order the query numbers them.
        children: Vec<Option<NodeId>>,
    },
    /// Makes something happen: an item given, a flag set, an effect played.
    Event {
        id: NodeId,
        /// An index into the game's table of events.
        event: u8,
        /// Split up per event, which is game data.
        params: [u8; 4],
        next: Option<NodeId>,
    },
}

impl Node {
    #[must_use]
    pub const fn id(&self) -> NodeId {
        match self {
            Self::Text { id, .. } | Self::Branch { id, .. } | Self::Event { id, .. } => *id,
        }
    }
}

/// One way into the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Root {
    /// What the game is asked for. TP sends ids from 3000 up to the stage's
    /// own flow file instead.
    pub public_id: u16,
    pub node: NodeId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flow {
    /// In stored order, which carries nothing since edges are handles. A
    /// node nothing reaches is still stored.
    pub nodes: Vec<Node>,
    pub roots: Vec<Root>,
}

record! {
    struct Flw1Header {
        node_count: Be16,
        table_count: Be16,
        pad: [u8; 4],
    }
}

record! {
    /// A node record before its kind is known. Every kind is this long.
    struct NodeRecord {
        kind: u8,
        body: [u8; 7],
    }
}

record! {
    /// Keeps its edge and that edge's mask byte in the record.
    struct TextNode {
        kind: u8,
        pad_1: u8,
        message: Be16,
        next: Be16,
        mask: u8,
        pad_7: u8,
    }
}

record! {
    struct BranchNode {
        kind: u8,
        child_count: u8,
        query: Be16,
        param: Be16,
        table_start: Be16,
    }
}

record! {
    struct EventNode {
        kind: u8,
        event: u8,
        table_index: Be16,
        params: [u8; 4],
    }
}

record! {
    struct Fli1Header {
        count: Be16,
        /// Always 8.
        entry_len: u8,
        pad: [u8; 5],
    }
}

record! {
    /// Each field padded out to 4 bytes.
    struct Fli1Entry {
        flow_id: Be16,
        pad_2: [u8; 2],
        node: Be16,
        pad_6: [u8; 2],
    }
}

mod kind {
    pub const TEXT: u8 = 0x01;
    pub const BRANCH: u8 = 0x02;
    pub const EVENT: u8 = 0x03;
}

const DEAD_END: u16 = 0xFFFF;
const ALIGN: usize = 16;

fn edge(entry: u16) -> Option<NodeId> {
    (entry != DEAD_END).then_some(NodeId(u32::from(entry)))
}

/// The mask byte shadowing a stored edge.
const fn mask(entry: u16) -> u8 {
    if entry == DEAD_END { 0xFF } else { 0x00 }
}

/// Borrows a node record as its kind's layout.
fn view<T: Record>(record: &NodeRecord) -> Result<&T> {
    ByteReader::new(record.as_bytes()).record()
}

#[expect(
    clippy::similar_names,
    reason = "named after the FLW1 and FLI1 sections"
)]
pub(super) fn read(flw1: &[u8], fli1: &[u8]) -> Result<Flow> {
    let mut r = ByteReader::new(flw1);
    let header: &Flw1Header = r.record()?;
    let node_count = header.node_count.get();
    let table_count = usize::from(header.table_count.get());

    let records: &[NodeRecord] = r.records_at(r.pos(), usize::from(node_count))?;
    let table_at = r.pos() + size_of_val(records);
    let table: &[Be16] = r.records_at(table_at, table_count)?;
    let masks = r.bytes_at(table_at + size_of_val(table), table_count)?;
    if table.iter().zip(masks).any(|(e, &m)| m != mask(e.get())) {
        return Err(malformed("a flow table entry disagrees with its mask byte"));
    }
    let entry = |at: usize, what| table.get(at).map(|e| edge(e.get())).ok_or(malformed(what));

    let mut nodes = Vec::with_capacity(records.len());
    for (id, record) in (0u32..).zip(records) {
        let id = NodeId(id);
        nodes.push(match record.kind {
            kind::TEXT => {
                let text: &TextNode = view(record)?;
                let next = text.next.get();
                if text.mask != mask(next) {
                    return Err(malformed("a text node's edge disagrees with its mask byte"));
                }
                Node::Text {
                    id,
                    message: MessageId(u32::from(text.message.get())),
                    next: edge(next),
                }
            }
            kind::BRANCH => {
                let branch: &BranchNode = view(record)?;
                let start = usize::from(branch.table_start.get());
                let children = (start..start + usize::from(branch.child_count))
                    .map(|at| entry(at, "a branch's children run past the flow table"))
                    .collect::<Result<_>>()?;
                Node::Branch {
                    id,
                    query: branch.query.get(),
                    param: branch.param.get(),
                    children,
                }
            }
            kind::EVENT => {
                let event: &EventNode = view(record)?;
                Node::Event {
                    id,
                    event: event.event,
                    params: event.params,
                    next: entry(
                        usize::from(event.table_index.get()),
                        "an event's edge is past the flow table",
                    )?,
                }
            }
            // The one padding record, rounding an odd count up to even.
            _ if id.0 + 1 == u32::from(node_count) => continue,
            _ => return Err(malformed("a flow node is not text, branch or event")),
        });
    }

    let mut r = ByteReader::new(fli1);
    let header: &Fli1Header = r.record()?;
    let entries: &[Fli1Entry] = r.records_at(r.pos(), usize::from(header.count.get()))?;
    let roots = entries
        .iter()
        .map(|e| Root {
            public_id: e.flow_id.get(),
            node: NodeId(u32::from(e.node.get())),
        })
        .collect();
    Ok(Flow { nodes, roots })
}

/// FLW1 and FLI1. Edges, roots and text nodes' messages are written as
/// positions. The table fills in node order, which is where retail puts it.
#[expect(
    clippy::similar_names,
    reason = "named after the FLW1 and FLI1 sections"
)]
pub(super) fn write(flow: &Flow, messages: &HashMap<MessageId, u16>) -> Result<(Vec<u8>, Vec<u8>)> {
    let node_at = positions(
        flow.nodes.iter().map(Node::id),
        "two flow nodes share an id",
    )?;
    let position = |node: NodeId| {
        node_at.get(&node).copied().ok_or(malformed(
            "a flow edge or root names a node the graph doesn't hold",
        ))
    };
    let entry = |edge: Option<NodeId>| edge.map_or(Ok(DEAD_END), position);

    let record_count = flow.nodes.len().next_multiple_of(ALIGN / NodeRecord::LEN);
    let mut records = ByteWriter::with_capacity(record_count * NodeRecord::LEN);
    let mut table = Vec::new();
    for node in &flow.nodes {
        match node {
            Node::Text { message, next, .. } => {
                let message = *messages.get(message).ok_or(malformed(
                    "a text node names a message the file doesn't hold",
                ))?;
                let next = entry(*next)?;
                records.record(&TextNode {
                    kind: kind::TEXT,
                    pad_1: 0,
                    message: Be16::new(message),
                    next: Be16::new(next),
                    mask: mask(next),
                    pad_7: 0,
                });
            }
            Node::Branch {
                query,
                param,
                children,
                ..
            } => {
                let child_count =
                    u8::try_from(children.len()).map_err(|_| too_large("a branch's answers"))?;
                let start = table_index(&table)?;
                for &child in children {
                    table.push(entry(child)?);
                }
                records.record(&BranchNode {
                    kind: kind::BRANCH,
                    child_count,
                    query: Be16::new(*query),
                    param: Be16::new(*param),
                    table_start: Be16::new(start),
                });
            }
            Node::Event {
                event,
                params,
                next,
                ..
            } => {
                let index = table_index(&table)?;
                table.push(entry(*next)?);
                records.record(&EventNode {
                    kind: kind::EVENT,
                    event: *event,
                    table_index: Be16::new(index),
                    params: *params,
                });
            }
        }
    }
    records.zeros((record_count - flow.nodes.len()) * NodeRecord::LEN);
    table.resize(table.len().next_multiple_of(ALIGN / Be16::LEN), 0);

    let records = records.finish();
    let mut flw1 = ByteWriter::with_capacity(Flw1Header::LEN + records.len() + table.len() * 3);
    flw1.record(&Flw1Header {
        node_count: Be16::new(
            u16::try_from(record_count).map_err(|_| too_large("the flow node count"))?,
        ),
        table_count: Be16::new(table_index(&table)?),
        pad: [0; 4],
    });
    flw1.bytes(&records);
    for &e in &table {
        flw1.u16(e);
    }
    for &e in &table {
        flw1.u8(mask(e));
    }

    let mut fli1 = ByteWriter::with_capacity(Fli1Header::LEN + flow.roots.len() * Fli1Entry::LEN);
    fli1.record(&Fli1Header {
        count: Be16::new(
            u16::try_from(flow.roots.len()).map_err(|_| too_large("the flow root count"))?,
        ),
        entry_len: u8::try_from(Fli1Entry::LEN).map_err(|_| too_large("a flow root"))?,
        pad: [0; 5],
    });
    for root in &flow.roots {
        fli1.record(&Fli1Entry {
            flow_id: Be16::new(root.public_id),
            pad_2: [0; 2],
            node: Be16::new(position(root.node)?),
            pad_6: [0; 2],
        });
    }
    Ok((flw1.finish(), fli1.finish()))
}

/// Where the next table entry lands.
fn table_index(table: &[u16]) -> Result<u16> {
    u16::try_from(table.len()).map_err(|_| too_large("the flow table"))
}

#[cfg(test)]
#[expect(
    clippy::similar_names,
    reason = "named after the FLW1 and FLI1 sections"
)]
mod tests {
    use super::*;
    use crate::FormatError;

    /// One node of each kind, an odd count forcing the padding record, a
    /// two-way branch, and live and dead edges in the table.
    fn sample() -> (Vec<u8>, Vec<u8>) {
        let flw1 = [
            0x00, 0x04, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, // header
            0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, // text -> node 1
            0x02, 0x02, 0x00, 0x23, 0x00, 0x00, 0x00, 0x00, // branch, table at 0
            0x03, 0x08, 0x00, 0x02, 0x0b, 0x00, 0x00, 0x00, // event, table at 2
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // padding
            0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, // table: 0, dead, dead, 0
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // table: 0, 0, 0, 0
            0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, // mask
        ]
        .to_vec();
        let fli1 = [
            0x00, 0x02, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, // header
            0x0b, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // flow 3000 -> node 0
            0x0b, 0xb9, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, // flow 3001 -> node 2
        ]
        .to_vec();
        (flw1, fli1)
    }

    fn one_message() -> HashMap<MessageId, u16> {
        HashMap::from([(MessageId(0), 0)])
    }

    fn is_malformed<T>(result: &Result<T>) -> bool {
        matches!(result, Err(FormatError::Malformed { .. }))
    }

    #[test]
    fn every_node_kind_and_root_is_read() {
        let (flw1, fli1) = sample();
        let flow = read(&flw1, &fli1).unwrap();
        assert_eq!(
            flow.nodes,
            [
                Node::Text {
                    id: NodeId(0),
                    message: MessageId(0),
                    next: Some(NodeId(1)),
                },
                Node::Branch {
                    id: NodeId(1),
                    query: 35,
                    param: 0,
                    children: vec![Some(NodeId(0)), None],
                },
                Node::Event {
                    id: NodeId(2),
                    event: 8,
                    params: [11, 0, 0, 0],
                    next: None,
                },
            ]
        );
        assert_eq!(
            flow.roots,
            [
                Root {
                    public_id: 3000,
                    node: NodeId(0),
                },
                Root {
                    public_id: 3001,
                    node: NodeId(2),
                },
            ]
        );
    }

    #[test]
    fn the_graph_writes_back_as_read() {
        let (flw1, fli1) = sample();
        let flow = read(&flw1, &fli1).unwrap();
        assert_eq!(write(&flow, &one_message()).unwrap(), (flw1, fli1));
    }

    #[test]
    fn broken_graphs_are_malformed() {
        let broken = |at: usize, value: u8| {
            let (mut flw1, fli1) = sample();
            flw1[at] = value;
            read(&flw1, &fli1)
        };
        // A padding record anywhere but last.
        assert!(is_malformed(&broken(8, 0x00)));
        // A live table entry whose mask says dead.
        assert!(is_malformed(&broken(56, 0xff)));
        // A live text edge whose mask says dead.
        assert!(is_malformed(&broken(14, 0xff)));
        // A branch's two children starting at 7 of 8.
        assert!(is_malformed(&broken(23, 0x07)));
        // An event's edge at 8 of 8.
        assert!(is_malformed(&broken(27, 0x08)));

        let (flw1, fli1) = sample();
        assert!(matches!(
            read(&flw1[..flw1.len() - 8], &fli1),
            Err(FormatError::OutOfBounds { .. })
        ));
    }

    /// A shuffled node is still found by its handle, not its old position.
    #[test]
    fn edges_follow_handles() {
        let (flw1, fli1) = sample();
        let mut flow = read(&flw1, &fli1).unwrap();
        flow.nodes.swap(0, 2);
        let (flw1, _) = write(&flow, &one_message()).unwrap();
        assert_eq!(flw1[8], kind::EVENT);
        // The text node, now at 2, still points at the branch at 1.
        assert_eq!(
            &flw1[24..32],
            [0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00]
        );

        flow.nodes.remove(1);
        assert!(is_malformed(&write(&flow, &one_message())));
        assert!(is_malformed(&write(
            &read(&sample().0, &sample().1).unwrap(),
            &HashMap::new()
        )));
    }
}
