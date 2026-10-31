use super::{
    ALIGN, Bmg, DAT1, FLI1, FLW1, HEADER_LEN, Header, INF1, MID1, STR1, SectionHeader, flow,
    malformed, message, positions, too_large,
};
use crate::{Be32, ByteWriter, Record, Result};

/// Sections go out in the order retail files have them. The last one's
/// padding is left off, as on the discs.
#[expect(
    clippy::similar_names,
    reason = "named after the FLW1 and FLI1 sections"
)]
pub(super) fn encode(bmg: &Bmg, out: &mut ByteWriter) -> Result<()> {
    let (inf1, dat1, mid1) = message::write(&bmg.messages, bmg.attributes_len, bmg.mid1)?;
    let mut sections = vec![(INF1, inf1), (DAT1, dat1)];
    if let Some(mid1) = mid1 {
        sections.push((MID1, mid1));
    }
    if let Some(strings) = &bmg.strings {
        sections.push((STR1, write_strings(strings)?));
    }
    // The header's size stops here.
    let stated = sections.len();
    if let Some(graph) = &bmg.flow {
        let messages = positions(
            bmg.messages.iter().map(|m| m.key),
            "two messages share an id",
        )?;
        let (flw1, fli1) = flow::write(graph, &messages)?;
        sections.push((FLW1, flw1));
        sections.push((FLI1, fli1));
    }

    let size = HEADER_LEN
        + sections[..stated]
            .iter()
            .map(|(_, b)| padded(b))
            .sum::<usize>();
    out.bytes(&Bmg::MAGIC);
    out.bytes(&Bmg::KIND);
    out.record(&Header {
        size: be32(size)?,
        section_count: be32(sections.len())?,
        encoding: bmg.encoding.byte(),
        pad: [0; 15],
    });
    let Some((last, rest)) = sections.split_last() else {
        return Ok(());
    };
    for section in rest {
        let written = write_section(out, section)?;
        out.zeros(padded(&section.1) - written);
    }
    write_section(out, last)?;
    Ok(())
}

/// Returns how many bytes went out, short of padding.
fn write_section(out: &mut ByteWriter, (magic, body): &([u8; 4], Vec<u8>)) -> Result<usize> {
    out.record(&SectionHeader {
        magic: *magic,
        size: be32(padded(body))?,
    });
    out.bytes(body);
    Ok(SectionHeader::LEN + body.len())
}

/// A section's stated size.
const fn padded(body: &[u8]) -> usize {
    (SectionHeader::LEN + body.len()).next_multiple_of(ALIGN)
}

/// The pool rejoined on its terminators.
fn write_strings(strings: &[Vec<u8>]) -> Result<Vec<u8>> {
    if strings.iter().any(|s| s.contains(&0)) {
        return Err(malformed("a string holds a terminator"));
    }
    Ok(strings.join(&0))
}

fn be32(value: usize) -> Result<Be32> {
    u32::try_from(value)
        .map(Be32::new)
        .map_err(|_| too_large("the message file"))
}
