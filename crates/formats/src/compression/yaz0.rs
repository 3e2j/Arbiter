//! Yaz0: each group of eight tokens is led by its flag byte, tokens inline.

use diag::Diagnostics;

use super::search::Tokens;
use super::token::Token;
use super::token::backref::Backreference;
use super::{Strategy, header, size_of};
use crate::{Decode, Encode, Reader, Result, Writer};

/// The flag byte: one bit per token in its group.
type Flags = u8;
/// Tokens led by one flag byte.
const GROUP_SIZE: u32 = Flags::BITS;
const TOP_FLAG_BIT: Flags = 1 << (Flags::BITS - 1);
/// The header's zero padding after the decompressed size.
const PADDING: usize = 8;

/// A Yaz0 wrapper, held decompressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Yaz0 {
    pub data: Vec<u8>,
    /// How [`encode`](Encode::encode) searches.
    pub strategy: Strategy,
}

impl Yaz0 {
    pub const MAGIC: [u8; 4] = *b"Yaz0";
}

impl Decode for Yaz0 {
    fn detect(bytes: &[u8]) -> bool {
        bytes.starts_with(&Self::MAGIC)
    }

    fn decode(bytes: &[u8], _: &mut Diagnostics) -> Result<Self> {
        Ok(Self {
            data: decompress(bytes)?,
            strategy: Strategy::Parity,
        })
    }
}

impl Encode for Yaz0 {
    fn encode(&self, out: &mut Writer) -> Result<()> {
        compress(&self.data, self.strategy, out)
    }
}

pub(super) fn decompress(input: &[u8]) -> Result<Vec<u8>> {
    let mut reader = Reader::new(input);
    let size = header(&mut reader, Yaz0::MAGIC)?;
    reader.bytes(PADDING)?;

    let mut out = vec![0; size];
    let mut pos = 0;
    let mut flags: Flags = 0;
    let mut items_left = 0;

    while pos < size {
        if items_left == 0 {
            flags = reader.u8()?;
            items_left = GROUP_SIZE;
        }
        let is_literal = flags & TOP_FLAG_BIT != 0;
        flags <<= 1;
        items_left -= 1;

        if is_literal {
            out[pos] = reader.u8()?;
            pos += 1;
            continue;
        }

        let pair = reader.u16()?;
        let backref = Backreference::from_pair(pair, || reader.u8())?;
        pos = backref.copy(&mut out, pos)?;
    }

    Ok(out)
}

pub(super) fn compress(input: &[u8], strategy: Strategy, out: &mut Writer) -> Result<()> {
    let size = size_of(input)?;
    out.bytes(&Yaz0::MAGIC);
    out.u32(size);
    out.zeros(PADDING);

    let mut flags: Flags = 0;
    // Each token is at most a pair and its extra byte.
    let mut group = Vec::with_capacity(GROUP_SIZE as usize * 3);
    let mut items = 0;

    for token in Tokens::new(input, size, strategy) {
        match token {
            Token::Literal(byte) => {
                flags |= TOP_FLAG_BIT >> items;
                group.push(byte);
            }
            Token::BackReference(matched) => {
                group.extend_from_slice(&matched.pair().to_be_bytes());
                group.extend(matched.extended());
            }
        }

        items += 1;
        if items == GROUP_SIZE {
            out.u8(flags);
            out.bytes(&group);
            group.clear();
            flags = 0;
            items = 0;
        }
    }

    // On an exact multiple of 8 this writes a trailing zero flag byte the
    // decoder never reads, kept for Nintendo parity.
    if !input.is_empty() {
        out.u8(flags);
        out.bytes(&group);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    fn header(size: u32) -> Vec<u8> {
        let mut data = b"Yaz0".to_vec();
        data.extend_from_slice(&size.to_be_bytes());
        data.extend_from_slice(&[0; PADDING]);
        data
    }

    fn decode(bytes: &[u8]) -> Result<Yaz0> {
        Yaz0::decode(bytes, &mut Diagnostics::default())
    }

    /// A literal group, then a back-reference over the four bytes it wrote.
    fn sample() -> Vec<u8> {
        let mut data = header(10);
        // Four literals, then a reference: length 4 + 2, distance 3 + 1.
        data.push(0b1111_0000);
        data.extend_from_slice(b"abcd");
        data.extend_from_slice(&[0x40, 0x03]);
        data
    }

    #[test]
    fn decodes_literals_and_overlapping_runs() {
        assert_eq!(decode(&sample()).unwrap().data, b"abcdabcdab");
    }

    #[test]
    fn rejects_other_data() {
        assert!(!Yaz0::detect(b"RARC...."));
        assert!(matches!(decode(b"RARC...."), Err(Error::WrongMagic { .. })));
    }

    #[test]
    fn rejects_truncated_input() {
        let data = sample();
        assert!(decode(&data[..data.len() - 3]).is_err());
    }

    #[test]
    fn rejects_a_run_past_the_declared_size() {
        let mut data = header(3);
        data.push(0b1000_0000);
        data.push(b'a');
        // Length (4 - 1) + 3, distance 0 + 1: 7 bytes total, not 3.
        data.extend_from_slice(&[0x40, 0x00]);
        assert!(matches!(decode(&data), Err(Error::Malformed { .. })));
    }

    #[test]
    fn rejects_a_back_reference_past_the_start() {
        let mut data = header(4);
        data.push(0b0000_0000);
        data.extend_from_slice(&[0x40, 0x03]);
        assert!(matches!(decode(&data), Err(Error::Malformed { .. })));
    }
}
