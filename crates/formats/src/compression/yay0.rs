//! Yay0: three streams, each at an offset the header gives.
//!
//! 1. The masks hold the token bits as 32-bit words
//! 2. The links hold each back-reference's pair
//! 3. The chunks each literal byte and extra length byte.

use diag::Diagnostics;

use super::search::Tokens;
use super::token::Token;
use super::token::backref::Backreference;
use super::{Strategy, header, size_of};
use crate::{Decode, Encode, Error, Reader, Result, Writer};

/// One mask word: one bit per token.
type Mask = u32;
const MASK_SIZE: u32 = Mask::BITS;
const TOP_MASK_BIT: Mask = 1 << (Mask::BITS - 1);
/// Magic, decompressed size, and the links and chunks offsets.
const HEADER_LEN: usize = 16;

/// A Yay0 wrapper, held decompressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Yay0 {
    pub data: Vec<u8>,
    /// How [`encode`](Encode::encode) searches.
    pub strategy: Strategy,
}

impl Yay0 {
    pub const MAGIC: [u8; 4] = *b"Yay0";
}

impl Decode for Yay0 {
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

impl Encode for Yay0 {
    fn encode(&self, out: &mut Writer) -> Result<()> {
        compress(&self.data, self.strategy, out)
    }
}

pub(super) fn decompress(input: &[u8]) -> Result<Vec<u8>> {
    let mut masks = Reader::new(input);
    let size = header(&mut masks, Yay0::MAGIC)?;
    let mut links = Reader::new(input);
    links.seek(masks.u32()? as usize);
    let mut chunks = Reader::new(input);
    chunks.seek(masks.u32()? as usize);

    let mut out = vec![0; size];
    let mut pos = 0;
    let mut mask: Mask = 0;
    let mut bits_left = 0;

    while pos < size {
        if bits_left == 0 {
            mask = masks.u32()?;
            bits_left = MASK_SIZE;
        }
        let is_literal = mask & TOP_MASK_BIT != 0;
        mask <<= 1;
        bits_left -= 1;

        if is_literal {
            out[pos] = chunks.u8()?;
            pos += 1;
            continue;
        }

        let pair = links.u16()?;
        let backref = Backreference::from_pair(pair, || chunks.u8())?;
        pos = backref.copy(&mut out, pos)?;
    }

    Ok(out)
}

pub(super) fn compress(input: &[u8], strategy: Strategy, out: &mut Writer) -> Result<()> {
    let size = size_of(input)?;

    let mut masks = Writer::new();
    let mut links = Writer::new();
    let mut chunks = Writer::new();
    let mut mask: Mask = 0;
    let mut bits = 0;

    for token in Tokens::new(input, size, strategy) {
        match token {
            Token::Literal(byte) => {
                mask |= TOP_MASK_BIT >> bits;
                chunks.u8(byte);
            }
            Token::BackReference(matched) => {
                links.u16(matched.pair());
                if let Some(extended) = matched.extended() {
                    chunks.u8(extended);
                }
            }
        }

        bits += 1;
        if bits == MASK_SIZE {
            masks.u32(mask);
            mask = 0;
            bits = 0;
        }
    }
    if bits > 0 {
        masks.u32(mask);
    }

    // Offsets are from the start of this file, wherever `out` places it.
    let links_at = HEADER_LEN + masks.len();
    let chunks_at = links_at + links.len();
    let offset = |pos: usize| {
        u32::try_from(pos).map_err(|_| Error::TooLarge {
            what: "a Yay0 stream offset",
        })
    };

    out.bytes(&Yay0::MAGIC);
    out.u32(size);
    out.u32(offset(links_at)?);
    out.u32(offset(chunks_at)?);
    out.bytes(&masks.finish());
    out.bytes(&links.finish());
    out.bytes(&chunks.finish());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(bytes: &[u8]) -> Result<Yay0> {
        Yay0::decode(bytes, &mut Diagnostics::default())
    }

    /// Four literals, then a back-reference over them: length 4 + 2,
    /// distance 3 + 1.
    fn sample() -> Vec<u8> {
        let mut data = b"Yay0".to_vec();
        for word in [10u32, 20, 22, 0xF000_0000] {
            data.extend_from_slice(&word.to_be_bytes());
        }
        data.extend_from_slice(&[0x40, 0x03]);
        data.extend_from_slice(b"abcd");
        data
    }

    #[test]
    fn decodes_from_three_streams() {
        assert_eq!(decode(&sample()).unwrap().data, b"abcdabcdab");
    }

    #[test]
    fn rejects_other_data() {
        assert!(!Yay0::detect(b"Yaz0...."));
        assert!(matches!(decode(b"Yaz0...."), Err(Error::WrongMagic { .. })));
    }

    /// A stream offset past the end is truncation, not an empty stream.
    #[test]
    fn rejects_a_stream_past_the_end() {
        let mut data = sample();
        data[15] = 0xFF;
        assert!(matches!(decode(&data), Err(Error::OutOfBounds { .. })));
    }
}
