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
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            data: input,
            pos: 0,
            chains: Chains::new(),
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
/// Positions a back-reference can reach, and so the slots the window holds.
const WINDOW: usize = MAX_DISTANCE as usize;
/// A position by its place in the window.
type Slot = u16;
/// A slot is a position modulo [`WINDOW`], which a power of two makes a mask.
const SLOT_MASK: Slot = MAX_DISTANCE - 1;
const _: () = assert!(WINDOW.is_power_of_two());
/// Slot zero is a real one, so absence needs its own value.
const NO_SLOT: Slot = Slot::MAX;

/// Positions from the last [`WINDOW`], chained by the hash of their first three
/// bytes, so a search compares only positions sharing a prefix instead of
/// brute forcing the whole window. A long run or short repeat still puts the
/// whole window in one chain.
///
/// Chains run oldest first and positions leave in the order they were filed,
/// so each is a queue. A hash collision costs a wasted comparison, never a
/// wrong match.
struct Chains {
    /// Oldest slot filed under each bucket.
    head: Box<[Slot]>,
    /// Newest slot filed under each bucket.
    tail: Box<[Slot]>,
    /// The next newer slot sharing a slot's bucket.
    next: Box<[Slot]>,
    /// First position not yet filed. Positions go in in order and once each.
    unfiled: usize,
    /// `unfiled`'s slot.
    unfiled_slot: Slot,
}

impl Chains {
    fn new() -> Self {
        Self {
            head: vec![NO_SLOT; HASH_SIZE].into_boxed_slice(),
            tail: vec![NO_SLOT; HASH_SIZE].into_boxed_slice(),
            next: vec![NO_SLOT; WINDOW].into_boxed_slice(),
            unfiled: 0,
            unfiled_slot: 0,
        }
    }

    /// Knuth's multiplicative hash over the three bytes at `pos`, which the
    /// caller has checked are there.
    fn hash(data: &[u8], pos: usize) -> usize {
        let key =
            u32::from(data[pos]) << 16 | u32::from(data[pos + 1]) << 8 | u32::from(data[pos + 2]);
        (key.wrapping_mul(2_654_435_761) >> (32 - HASH_BITS)) as usize
    }

    /// Files every position below `end`, dropping each one that falls out of
    /// reach. The last `MIN_LENGTH - 1` bytes have no full-length prefix and
    /// are left out.
    fn fill(&mut self, data: &[u8], end: usize) {
        let end = end.min(data.len().saturating_sub(MIN_LENGTH as usize - 1));
        while self.unfiled < end {
            let at = self.unfiled;
            // The window's oldest position, so the oldest in its bucket. It
            // shares `at`'s slot, so it has to leave first.
            if let Some(gone) = at.checked_sub(WINDOW) {
                let bucket = Self::hash(data, gone);
                let after = self.next[usize::from(self.head[bucket])];
                self.head[bucket] = after;
                if after == NO_SLOT {
                    self.tail[bucket] = NO_SLOT;
                }
            }

            let bucket = Self::hash(data, at);
            let slot = self.unfiled_slot;
            self.next[usize::from(slot)] = NO_SLOT;
            match self.tail[bucket] {
                NO_SLOT => self.head[bucket] = slot,
                newest => self.next[usize::from(newest)] = slot,
            }
            self.tail[bucket] = slot;
            self.unfiled += 1;
            self.unfiled_slot = slot.wrapping_add(1) & SLOT_MASK;
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
        self.fill(data, pos);

        let ceiling = (data.len() - pos).min(MAX_LENGTH as usize);
        let mut best: Option<Backreference> = None;
        // Nintendo parity: the window is scanned front to back and a candidate
        // only replaces a strictly shorter one, so ties go to the earliest.
        // This tie-break only changes which equal-length match is picked, not
        // the final filesize. Chains run oldest first already.
        let mut slot = self.head[Self::hash(data, pos)];
        while slot != NO_SLOT {
            // Every filed position is within `WINDOW` behind `pos`, so its
            // distance back fits the slot mask.
            let start =
                pos - 1 - ((pos - 1).wrapping_sub(usize::from(slot)) & usize::from(SLOT_MASK));
            slot = self.next[usize::from(slot)];
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
