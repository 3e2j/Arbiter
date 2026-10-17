use super::{
    Bmg, DAT1, Encoding, FLI1, FLW1, HEADER_LEN, Header, INF1, MID1, STR1, SectionHeader, flow,
    malformed, message,
};
use crate::{ByteReader, Record, Result};

/// A file's section bodies, padding included where the file has it.
pub(super) struct Sections<'a> {
    inf1: &'a [u8],
    dat1: &'a [u8],
    mid1: Option<&'a [u8]>,
    str1: Option<&'a [u8]>,
    /// FLW1 and FLI1.
    pub(super) flow: Option<(&'a [u8], &'a [u8])>,
}

pub(super) fn decode(bytes: &[u8]) -> Result<Bmg> {
    let (encoding, sections) = split(bytes)?;
    let (messages, record_len, mid1) = message::read(sections.inf1, sections.dat1, sections.mid1)?;
    Ok(Bmg {
        encoding,
        record_len,
        mid1,
        messages,
        flow: sections
            .flow
            .map(|(flw1, fli1)| flow::read(flw1, fli1))
            .transpose()?,
        strings: sections.str1.map(read_strings),
    })
}

/// Walks the section table from the front, sorting sections by name. A count
/// larger than the file holds runs out of bytes rather than being trusted.
#[expect(
    clippy::similar_names,
    reason = "named after the FLW1 and FLI1 sections"
)]
pub(super) fn split(bytes: &[u8]) -> Result<(Encoding, Sections<'_>)> {
    let mut r = ByteReader::new(bytes);
    r.magic(Bmg::MAGIC)?;
    r.magic(Bmg::KIND)?;
    let header: &Header = r.record()?;

    let mut at = HEADER_LEN;
    // The header's size leaves the flow pair out, so this does too.
    let mut stated = HEADER_LEN;
    let mut inf1 = None;
    let mut dat1 = None;
    let mut mid1 = None;
    let mut str1 = None;
    let mut flw1 = None;
    let mut fli1 = None;

    for _ in 0..header.section_count.get() {
        r.seek(at);
        let section: &SectionHeader = r.record()?;
        let size = section.size.get() as usize;
        if size < SectionHeader::LEN {
            return Err(malformed("a section is smaller than its own header"));
        }
        // The last section may stop short of the padding its size counts.
        let len = (size - SectionHeader::LEN).min(r.remaining());
        let body = r.bytes(len)?;

        let slot = match section.magic {
            INF1 => &mut inf1,
            DAT1 => &mut dat1,
            MID1 => &mut mid1,
            STR1 => &mut str1,
            FLW1 => &mut flw1,
            FLI1 => &mut fli1,
            _ => return Err(malformed("a section has a name no file has")),
        };
        if slot.replace(body).is_some() {
            return Err(malformed("a section appears twice"));
        }
        at = at.saturating_add(size);
        if !matches!(section.magic, FLW1 | FLI1) {
            stated = stated.saturating_add(size);
        }
    }

    if header.size.get() as usize != stated {
        return Err(malformed("the stated size isn't where the sections end"));
    }
    let flow = match (flw1, fli1) {
        (Some(flw1), Some(fli1)) => Some((flw1, fli1)),
        (None, None) => None,
        _ => {
            return Err(malformed(
                "the flow graph is missing one of its two sections",
            ));
        }
    };
    Ok((
        Encoding::from_byte(header.encoding),
        Sections {
            inf1: inf1.ok_or(malformed("there is no message table"))?,
            dat1: dat1.ok_or(malformed("there is no message text"))?,
            mid1,
            str1,
            flow,
        },
    ))
}

fn read_strings(pool: &[u8]) -> Vec<Vec<u8>> {
    pool.split(|&b| b == 0).map(<[u8]>::to_vec).collect()
}
