//! The match search both formats share: turns raw bytes into tokens. Each
//! format lays the tokens out its own way.
//!
//! This is Nintendo's search, so a retail file re-encodes byte for byte.
// Deeper lazy matching and optimal parsing were tried, but saved under half a
// percent, too little to be worth the slower encode.

use super::token::Token;
use super::token::backref::{Backreference, MAX_DISTANCE, MAX_LENGTH, MIN_LENGTH};

/// Extra length, on top of +1, the match one position on needs before it's
/// taken instead, a "lazy match".
///
/// Deferring moves the cursor forward one (+1) and leaves a literal behind
/// (-1), so a bare +1 is a tie, and a loss if it also trips the extended-length
/// byte. Nintendo checks only once, so it demands +2 and never loses.
const LAZY_SLACK: u16 = 1;

/// Every token `input` compresses to, in order.
pub struct Tokens<'a> {
    data: &'a [u8],
    pos: usize,
    chains: Chains,
    /// A lazy match, due after the literal it deferred past.
    pending: Option<Backreference>,
}

impl<'a> Tokens<'a> {
    /// `input` must fit a 32-bit size, which every format's header checks
    /// first.
    pub fn new(input: &'a [u8], size: u32) -> Self {
        Self {
            data: input,
            pos: 0,
            chains: Chains::new(size),
            pending: None,
        }
    }

    /// The pending lazy match, else the longest match at `pos` unless the
    /// one past it beats it by more than [`LAZY_SLACK`].
    fn next_token(&mut self) -> Token {
        let (data, pos) = (self.data, self.pos);
        if let Some(matched) = self.pending.take() {
            return Token::BackReference(matched);
        }

        let Some(best) = self.chains.longest_match(data, pos) else {
            return Token::Literal(data[pos]);
        };
        if pos + 1 < data.len()
            && let Some(next) = self.chains.longest_match(data, pos + 1)
            && next.length() > best.length() + LAZY_SLACK
        {
            self.pending = Some(next);
            return Token::Literal(data[pos]);
        }
        Token::BackReference(best)
    }
}

impl Iterator for Tokens<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        if self.pos >= self.data.len() {
            return None;
        }
        let token = self.next_token();
        self.pos += match &token {
            Token::Literal(_) => 1,
            Token::BackReference(matched) => matched.length() as usize,
        };
        Some(token)
    }
}

/// Bits in a [`HASH_SIZE`] bucket index. Chosen wide enough to keep
/// [`MAX_DISTANCE`]-sized windows of three-byte prefixes from colliding too
/// often, no formula behind the exact value.
const HASH_BITS: u32 = 13;
/// Buckets of three-byte prefixes: one chain per hash of the [`MAX_DISTANCE`]
/// window's worth of positions.
const HASH_SIZE: usize = 1 << HASH_BITS;
/// Position zero is a real one, so absence needs its own value.
const NO_POSITION: u32 = u32::MAX;

/// Where a three-byte prefix turned up earlier: one chain per hash bucket,
/// threaded backward through the input by `prev`.
///
/// Brute force is O(n * [`MAX_DISTANCE`]): compare every position against
/// its whole window. Filing positions by their 3-byte prefix limits each
/// comparison to positions sharing that prefix, cheap when prefixes are
/// varied. But nothing caps chain length: if a whole window shares one
/// prefix (a long run, a short repeat), the chain holds all of it, and the
/// walk is back to O([`MAX_DISTANCE`]) per position.
///
/// Flat tables, not `HashMap`: `head` is the bucket lookup, a collision
/// just costs one wasted byte comparison in `longest_match`, never a wrong
/// match, and `prev` threads every position sharing a bucket, not just the
/// newest. Both are sized once up front, no per-position allocation.
struct Chains {
    /// Newest position filed under each prefix.
    head: Box<[u32]>,
    /// Previous position sharing a position's prefix.
    prev: Box<[u32]>,
    /// First position not yet filed. Positions go in in order and once each.
    unfiled: u32,
    /// The chain within the window, reused so the search does not allocate
    /// per byte.
    in_window: Vec<u32>,
}

impl Chains {
    fn new(len: u32) -> Self {
        Self {
            head: vec![NO_POSITION; HASH_SIZE].into_boxed_slice(),
            // Every slot is written when its position is filed, before
            // anything reads it, so zeroed memory skips writing `NO_POSITION`
            // into all `len` slots up front.
            prev: vec![0; len as usize].into_boxed_slice(),
            unfiled: 0,
            in_window: Vec::with_capacity(MAX_DISTANCE as usize),
        }
    }

    /// Knuth's multiplicative hash over the three bytes at `pos`, which the
    /// caller has checked are there.
    fn hash(data: &[u8], pos: usize) -> usize {
        let key =
            u32::from(data[pos]) << 16 | u32::from(data[pos + 1]) << 8 | u32::from(data[pos + 2]);
        (key.wrapping_mul(2_654_435_761) >> (32 - HASH_BITS)) as usize
    }

    /// Files every position below `end`. The last `MIN_MATCH - 1` bytes have
    /// no full-length prefix and are left out.
    fn fill(&mut self, data: &[u8], end: usize) {
        let end = end.min(data.len().saturating_sub(MIN_LENGTH as usize - 1));
        while (self.unfiled as usize) < end {
            let at = self.unfiled;
            let bucket = Self::hash(data, at as usize);
            self.prev[at as usize] = self.head[bucket];
            self.head[bucket] = at;
            self.unfiled += 1;
        }
    }

    /// The match to take at `pos`, if any.
    ///
    /// Positions that a match skipped over get filed on the next call, so
    /// the caller only ever walks forward.
    fn longest_match(&mut self, data: &[u8], pos: usize) -> Option<Backreference> {
        if pos + MIN_LENGTH as usize > data.len() {
            return None;
        }
        self.fill(data, pos + 1);

        let window = pos.saturating_sub(MAX_DISTANCE as usize);
        let ceiling = (data.len() - pos).min(MAX_LENGTH as usize);

        self.in_window.clear();
        let mut candidate = self.prev[pos];
        while candidate != NO_POSITION && candidate as usize >= window {
            self.in_window.push(candidate);
            candidate = self.prev[candidate as usize];
        }

        let mut best: Option<Backreference> = None;
        // Nintendo parity: the window is scanned front to back and a candidate
        // only replaces a strictly shorter one, so ties go to the earliest.
        // This tie-break only changes which equal-length match is picked, not
        // the final filesize. The chain runs newest first, hence the reverse.
        for &candidate in self.in_window.iter().rev() {
            let start = candidate as usize;
            let best_len = best.map_or(0, |matched| matched.length() as usize);

            // The byte right after `best_len` must also match, or the candidate
            // can't beat it. Checked early before doing the full compare below.
            if best_len > 0 && data[start + best_len] != data[pos + best_len] {
                continue;
            }

            let mut length = 0;
            while length < ceiling && data[start + length] == data[pos + length] {
                length += 1;
            }
            // `new` turns down anything under `MIN_LENGTH`.
            if length > best_len
                && let Some(found) = Backreference::new(pos - start, length)
            {
                best = Some(found);
                // Nothing scanned after this can beat it, so the first candidate
                // to reach the cap wins outright and the walk can stop here.
                if length == ceiling {
                    break;
                }
            }
        }

        best
    }
}
