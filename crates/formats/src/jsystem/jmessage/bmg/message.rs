//! INF1, DAT1 and MID1. One message is spread across all three: its record
//! in INF1, the text that record points at in DAT1, and its id at the same
//! position in MID1.

use super::{malformed, too_large};
use crate::{Be16, Be32, ByteReader, ByteWriter, Record, Result, record};

/// Internal stable handle for a message, held by whatever refers to one.
/// Initially assigned from the message's position, and kept as messages move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageKey(pub u32);

/// One message: what it's called, how it's displayed, and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// What the game looks the message up by, from MID1. Zero in a file
    /// without one. The attributes may hold a copy, left as found.
    pub id: u16,
    pub key: MessageKey,
    /// The INF1 record after the text offset: box style, sound, speaker and
    /// the rest, whichever byte is which being game data.
    pub attributes: Box<[u8]>,
    pub text: Vec<TextSegment>,
}

/// One stretch of a message's text, still encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSegment {
    Text(Vec<u8>),
    /// An escape sequence, split the way `JMessage::TProcessor::on_tag_`
    /// reads it. The opener and length byte are derived on encode.
    Tag {
        group: u8,
        code: u16,
        args: Vec<u8>,
    },
}

/// What MID1 says about its ids.
///
/// The header's `ordered` bit, promising sorted ids the game may binary
/// search, is a fact about the ids, so it's derived on encode. A file
/// claiming it over unsorted ids is refused.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mid1Header {
    /// The id array's layout. The game asserts it's zero.
    pub form: u8,
    /// How far a stored id is shifted to pack a second id below it. Nothing
    /// in TP passes a second id, so anything but zero is refused.
    pub shift_bytes: u8,
}

record! {
    struct Inf1Header {
        count: Be16,
        record_len: Be16,
        /// Not kept. TP derives a message's group from its id instead.
        group_id: Be16,
        pad: [u8; 2],
    }
}

record! {
    struct RawMid1Header {
        /// Not read, as INF1's count is what the game walks by.
        count: Be16,
        /// `ordered` in the high nibble, `form` in the low.
        ordered_form: u8,
        shift_bytes: u8,
        pad: [u8; 4],
    }
}

/// The text offset at the front of every INF1 record.
const TEXT_OFFSET_LEN: u16 = 4;

/// Opens a tag, followed by the whole tag's length.
const TAG_OPENER: u8 = 0x1A;
/// The opener, length, group and code, which the arguments follow.
const TAG_HEADER_LEN: usize = 5;

const ORDERED: u8 = 0x10;
const FORM_MASK: u8 = 0x0F;

/// The messages, their attributes width, and the MID1 header if there is
/// one.
pub(super) fn read(
    inf1: &[u8],
    dat1: &[u8],
    mid1: Option<&[u8]>,
) -> Result<(Vec<Message>, u16, Option<Mid1Header>)> {
    let mut r = ByteReader::new(inf1);
    let header: &Inf1Header = r.record()?;
    let count = usize::from(header.count.get());
    let attributes_len = header
        .record_len
        .get()
        .checked_sub(TEXT_OFFSET_LEN)
        .ok_or(malformed("an INF1 record is narrower than its text offset"))?;
    let ids: Option<&[Be32]> = mid1
        .map(|mid1| ByteReader::new(mid1).records_at(RawMid1Header::LEN, count))
        .transpose()?;

    let mut messages = Vec::with_capacity(count);
    for key in (0u32..).take(count) {
        let text_at = r.u32()? as usize;
        let attributes = r.bytes(usize::from(attributes_len))?.into();
        let id = match ids.and_then(|ids| ids.get(key as usize)) {
            Some(stored) => u16::try_from(stored.get())
                .map_err(|_| malformed("a MID1 id doesn't fit in 16 bits"))?,
            None => 0,
        };
        messages.push(Message {
            id,
            key: MessageKey(key),
            attributes,
            text: read_text(dat1, text_at)?,
        });
    }

    let mid1 = mid1.map(|mid1| read_mid1(mid1, &messages)).transpose()?;
    Ok((messages, attributes_len, mid1))
}

/// Splits one message's text into runs and tags up to its terminator. An
/// empty run beside a tag is left out, as it adds nothing back.
fn read_text(dat1: &[u8], start: usize) -> Result<Vec<TextSegment>> {
    let mut segments = Vec::new();
    let mut run = start;
    let mut at = start;
    loop {
        let byte = *dat1
            .get(at)
            .ok_or(malformed("a message's text runs past the end of DAT1"))?;
        if byte != 0 && byte != TAG_OPENER {
            at += 1;
            continue;
        }
        if at > run {
            segments.push(TextSegment::Text(dat1[run..at].to_vec()));
        }
        if byte == 0 {
            return Ok(segments);
        }
        let len = *dat1
            .get(at + 1)
            .ok_or(malformed("a tag is cut off before its length byte"))?;
        let end = at + usize::from(len);
        let Some([_, _, group, high, low, args @ ..]) = dat1.get(at..end) else {
            return Err(malformed(
                "a tag runs past the end of DAT1 or is shorter than its header",
            ));
        };
        segments.push(TextSegment::Tag {
            group: *group,
            code: u16::from_be_bytes([*high, *low]),
            args: args.to_vec(),
        });
        at = end;
        run = end;
    }
}

fn read_mid1(mid1: &[u8], messages: &[Message]) -> Result<Mid1Header> {
    let header: &RawMid1Header = ByteReader::new(mid1).record()?;
    if header.ordered_form & ORDERED != 0 && !sorted(messages) {
        return Err(malformed("MID1 claims its ids are sorted, and they aren't"));
    }
    if header.shift_bytes != 0 {
        return Err(malformed("MID1 packs a second id into each id"));
    }
    Ok(Mid1Header {
        form: header.ordered_form & FORM_MASK,
        shift_bytes: header.shift_bytes,
    })
}

fn sorted(messages: &[Message]) -> bool {
    messages.is_sorted_by_key(|m| m.id)
}

/// INF1, DAT1, and MID1 when the file has one.
pub(super) type Sections = (Vec<u8>, Vec<u8>, Option<Vec<u8>>);

/// DAT1 opens with a lone terminator, as on retail, so no message starts at
/// zero.
pub(super) fn write(
    messages: &[Message],
    attributes_len: u16,
    mid1: Option<Mid1Header>,
) -> Result<Sections> {
    let record_len = attributes_len
        .checked_add(TEXT_OFFSET_LEN)
        .ok_or(too_large("an INF1 record"))?;
    let count = count(messages)?;

    let mut inf1 =
        ByteWriter::with_capacity(Inf1Header::LEN + messages.len() * usize::from(record_len));
    inf1.record(&Inf1Header {
        count: Be16::new(count),
        record_len: Be16::new(record_len),
        group_id: Be16::new(0),
        pad: [0; 2],
    });
    let mut dat1 = ByteWriter::new();
    dat1.u8(0);
    for message in messages {
        if message.attributes.len() != usize::from(attributes_len) {
            return Err(malformed(
                "a message's attributes aren't the width the file states",
            ));
        }
        inf1.u32(u32::try_from(dat1.len()).map_err(|_| too_large("the message text"))?);
        inf1.bytes(&message.attributes);
        write_text(&mut dat1, &message.text)?;
    }

    let mid1 = mid1
        .map(|header| write_mid1(header, messages, count))
        .transpose()?;
    Ok((inf1.finish(), dat1.finish(), mid1))
}

fn count(messages: &[Message]) -> Result<u16> {
    u16::try_from(messages.len()).map_err(|_| too_large("the message count"))
}

/// Refuses a run that wouldn't split back out the same, as a stray
/// terminator or opener ends a message early.
fn write_text(dat1: &mut ByteWriter, text: &[TextSegment]) -> Result<()> {
    for segment in text {
        match segment {
            TextSegment::Text(run) => {
                if run.iter().any(|&b| b == 0 || b == TAG_OPENER) {
                    return Err(malformed("a text run holds a terminator or a tag opener"));
                }
                dat1.bytes(run);
            }
            TextSegment::Tag { group, code, args } => {
                let len = u8::try_from(TAG_HEADER_LEN + args.len())
                    .map_err(|_| too_large("a tag's arguments"))?;
                dat1.u8(TAG_OPENER);
                dat1.u8(len);
                dat1.u8(*group);
                dat1.u16(*code);
                dat1.bytes(args);
            }
        }
    }
    dat1.u8(0);
    Ok(())
}

fn write_mid1(header: Mid1Header, messages: &[Message], count: u16) -> Result<Vec<u8>> {
    if header.shift_bytes != 0 {
        return Err(malformed("MID1 packs a second id into each id"));
    }
    if header.form & !FORM_MASK != 0 {
        return Err(malformed("a MID1 form doesn't fit its nibble"));
    }
    let ordered = if sorted(messages) { ORDERED } else { 0 };
    let mut out = ByteWriter::with_capacity(RawMid1Header::LEN + messages.len() * Be32::LEN);
    out.record(&RawMid1Header {
        count: Be16::new(count),
        ordered_form: ordered | header.form,
        shift_bytes: 0,
        pad: [0; 4],
    });
    for message in messages {
        out.u32(u32::from(message.id));
    }
    Ok(out.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FormatError;

    fn tag(code: u16, args: &[u8]) -> TextSegment {
        TextSegment::Tag {
            group: 0,
            code,
            args: args.to_vec(),
        }
    }

    fn text(run: &[u8]) -> TextSegment {
        TextSegment::Text(run.to_vec())
    }

    fn message(key: u32, id: u16, attributes: &[u8], text: &[TextSegment]) -> Message {
        Message {
            id,
            key: MessageKey(key),
            attributes: attributes.into(),
            text: text.to_vec(),
        }
    }

    fn inf1_header(count: u16, record_len: u16) -> Vec<u8> {
        Inf1Header {
            count: Be16::new(count),
            record_len: Be16::new(record_len),
            group_id: Be16::new(0),
            pad: [0; 2],
        }
        .as_bytes()
        .to_vec()
    }

    /// A count of zero, which nothing reads.
    fn mid1_header(ordered_form: u8, shift_bytes: u8) -> Vec<u8> {
        RawMid1Header {
            count: Be16::new(0),
            ordered_form,
            shift_bytes,
            pad: [0; 4],
        }
        .as_bytes()
        .to_vec()
    }

    fn record(text_at: u32, attributes: &[u8]) -> Vec<u8> {
        [&text_at.to_be_bytes()[..], attributes].concat()
    }

    fn is_malformed<T>(result: &Result<T>) -> bool {
        matches!(result, Err(FormatError::Malformed { .. }))
    }

    const HEADER: Mid1Header = Mid1Header {
        form: 0,
        shift_bytes: 0,
    };

    fn sample() -> Vec<Message> {
        vec![
            message(0, 5, &[0x00, 0x05], &[text(b"Hi")]),
            message(
                1,
                10,
                &[0x00, 0x0A],
                &[
                    TextSegment::Tag {
                        group: 0x01,
                        code: 0x0002,
                        args: Vec::new(),
                    },
                    text(b"Yo"),
                ],
            ),
        ]
    }

    #[test]
    fn text_splits_at_tags() {
        let dat1 = b"Hi \x1A\x06\x01\x00\x02\xAA there\0";
        assert_eq!(
            read_text(dat1, 0).unwrap(),
            [
                text(b"Hi "),
                TextSegment::Tag {
                    group: 0x01,
                    code: 0x0002,
                    args: vec![0xAA],
                },
                text(b" there"),
            ]
        );
    }

    /// A tag at the start, one right after, and nothing after the second.
    #[test]
    fn empty_runs_around_tags_are_dropped() {
        let dat1 = [0x1A, 5, 0, 0, 0xAA, 0x1A, 5, 0, 0, 0xBB, 0x00];
        assert_eq!(
            read_text(&dat1, 0).unwrap(),
            [tag(0xAA, &[]), tag(0xBB, &[])]
        );
    }

    #[test]
    fn broken_text_is_malformed() {
        assert!(is_malformed(&read_text(b"abc", 0)));
        assert!(is_malformed(&read_text(&[b'a', 0x1A], 0)));
        // The game takes the length less 5 as the argument count, so a
        // shorter tag would have a huge one.
        assert!(is_malformed(&read_text(&[0x1A, 1, 0x00], 0)));
        assert!(is_malformed(&read_text(&[0x1A, 4, 0x00, 0x00, 0x00], 0)));
        assert!(is_malformed(&read_text(&[0x1A, 5, 0x00], 0)));
    }

    #[test]
    fn mid1_is_checked() {
        let header = read_mid1(&mid1_header(0xF3, 0x00), &[]).unwrap();
        assert_eq!(header.form, 0x3);
        assert!(is_malformed(&read_mid1(&mid1_header(0x00, 0x02), &[])));

        // The game binary searches ids under the bit, and misses unsorted ones.
        let unsorted = [message(0, 10, &[], &[]), message(1, 5, &[], &[])];
        assert!(is_malformed(&read_mid1(&mid1_header(0x10, 0), &unsorted)));
        assert!(read_mid1(&mid1_header(0x00, 0), &unsorted).is_ok());
    }

    #[test]
    fn messages_read_with_text_attributes_and_ids() {
        let mut inf1 = inf1_header(2, 6);
        inf1.extend(record(0, &[0xAA, 0xBB]));
        inf1.extend(record(3, &[0xCC, 0xDD]));
        let dat1 = b"Hi\0Yo\0";

        let (messages, attributes_len, mid1) = read(&inf1, dat1, None).unwrap();
        assert_eq!(attributes_len, 2);
        assert_eq!(mid1, None);
        assert_eq!(
            messages,
            [
                message(0, 0, &[0xAA, 0xBB], &[text(b"Hi")]),
                message(1, 0, &[0xCC, 0xDD], &[text(b"Yo")]),
            ]
        );

        // An id comes out whole, and the attributes aren't checked against it.
        let mut ids = mid1_header(0, 0);
        ids.extend(5u32.to_be_bytes());
        ids.extend(0x0102u32.to_be_bytes());
        let (messages, _, mid1) = read(&inf1, dat1, Some(&ids)).unwrap();
        assert_eq!(messages[0].id, 5);
        assert_eq!(messages[1].id, 0x0102);
        assert_eq!(mid1, Some(HEADER));
    }

    #[test]
    fn broken_tables_are_malformed() {
        let mut inf1 = inf1_header(1, 6);
        inf1.extend(record(0, &[0, 0]));
        let mut ids = mid1_header(0, 0);
        ids.extend(0x0001_0000u32.to_be_bytes());
        assert!(is_malformed(&read(&inf1, b"\0", Some(&ids))));

        assert!(is_malformed(&read(&inf1_header(1, 3), &[], None)));
    }

    /// DAT1 opens with a lone terminator, so the first text is at 1, and MID1
    /// sets the `ordered` bit above `form`.
    #[test]
    fn messages_write_the_retail_layout() {
        let (inf1, dat1, mid1) = write(&sample(), 2, Some(HEADER)).unwrap();
        let mut expected = inf1_header(2, 6);
        expected.extend(record(1, &[0x00, 0x05]));
        expected.extend(record(4, &[0x00, 0x0A]));
        assert_eq!(inf1, expected);
        assert_eq!(dat1, b"\0Hi\0\x1A\x05\x01\x00\x02Yo\0");
        assert_eq!(
            mid1.unwrap(),
            [0, 2, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 10]
        );
    }

    #[test]
    fn messages_round_trip() {
        let header = Mid1Header {
            form: 3,
            shift_bytes: 0,
        };
        let (inf1, dat1, mid1) = write(&sample(), 2, Some(header)).unwrap();
        let (messages, attributes_len, read_header) = read(&inf1, &dat1, mid1.as_deref()).unwrap();
        assert_eq!(messages, sample());
        assert_eq!(attributes_len, 2);
        assert_eq!(read_header, Some(header));
    }

    #[test]
    fn the_ordered_bit_follows_the_ids() {
        let mut shuffled = sample();
        shuffled.swap(0, 1);
        let (_, _, mid1) = write(&shuffled, 2, Some(HEADER)).unwrap();
        assert_eq!(mid1.unwrap()[2], 0x00);
    }

    #[test]
    fn unwritable_messages_are_refused() {
        assert!(is_malformed(&write(&sample(), 4, None)));
        let terminator = [message(0, 0, &[], &[text(b"a\0b")])];
        assert!(is_malformed(&write(&terminator, 0, None)));

        let long = |len| [message(0, 0, &[], &[tag(0, &vec![0; len])])];
        let (_, dat1, _) = write(&long(250), 0, None).unwrap();
        assert_eq!(dat1[2], 255);
        assert!(matches!(
            write(&long(251), 0, None),
            Err(FormatError::TooLarge { .. })
        ));
    }
}
