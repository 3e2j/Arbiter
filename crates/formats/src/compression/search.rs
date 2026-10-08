//! The match search both formats share: turns raw bytes into tokens. Each
//! format lays the tokens out its own way.

use super::Strategy;
use super::token::Token;
use super::token::backref::{Backreference, MAX_DISTANCE, MAX_LENGTH, MIN_LENGTH};

/// After finding a valid backref match, we look to see if a match beside it is
/// better, a "lazy match". These are the knobs that shape detection.
///
/// Each lazy match is a mathematical gamble.
/// Deferring for a chance at a better match has a cost:
///
/// The cursor moves forward (+1) and leaves behind a literal byte (-1).
/// So a bare +1 improvement is a tie (0), and is a small loss (-1) if it also trips
/// the backreference into its extended-length byte. These tokens are a sunk cost
/// so ideally we want the best chance at an improvement that outweighs the loss/tie.
struct LazyMatch {
    /// How many further positions it's willing to chase for a better backreference
    depth: usize,
    /// Extra length demanded on top of the bare +1 improvement required to
    /// defer at all before it's worth spending a literal on.
    slack: usize,
}

impl LazyMatch {
    /// Nintendo's original encoder.
    ///
    /// Only checks for a lazy match once, reducing build time. With only one shot
    /// at the gamble, the floor must be raised to zero so a net-loss cannot occur.
    /// At worst a gamble loses -2 bytes (literal + extended-byte), so we need a
    /// guaranteed +2 improvement (slack of 1).
    const PARITY: Self = Self {
        depth: 1,
        slack: 1, // Improvement of +2
    };

    /// Gets a smaller file.
    ///
    /// Chases a better match over many tries to recover from a bad gamble.
    ///
    /// `depth` of `MAX_LENGTH` is a safe cap: each step must beat the last
    /// by `slack`, and matches can't grow past `MAX_LENGTH` either, so the
    /// chase most likely runs out of room long before using it up.
    // Still a greedy heuristic, not the smallest possible output: a
    // locally-good match can rule out a shorter one that would have opened
    // onto a longer one right after. Optimal parsing (shortest path over
    // every candidate length at every position, not just the longest)
    // would close that gap. Not implemented here without a reason to need it.
    //
    // TODO: Make the cost of specifically hitting the boundary length be slack = 1
    // so we never get a loss, just a more extensive search.
    const EXTENSIVE: Self = Self {
        depth: MAX_LENGTH as usize,
        slack: 0,
    };

    const fn of(strategy: Strategy) -> Self {
        match strategy {
            Strategy::Parity => Self::PARITY,
            Strategy::Extensive => Self::EXTENSIVE,
        }
    }
}

/// Every token `input` compresses to, in order.
pub struct Tokens<'a> {
    data: &'a [u8],
    pos: usize,
    chains: Chains,
    lookahead: Lookahead,
    strategy: LazyMatch,
}

impl<'a> Tokens<'a> {
    /// `input` must fit a 32-bit size, which every format's header checks
    /// first.
    pub fn new(input: &'a [u8], size: u32, strategy: Strategy) -> Self {
        Self {
            data: input,
            pos: 0,
            chains: Chains::new(size),
            lookahead: Lookahead::default(),
            strategy: LazyMatch::of(strategy),
        }
    }
}

impl Iterator for Tokens<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        if self.pos >= self.data.len() {
            return None;
        }
        let token = next_token(
            &mut self.chains,
            &mut self.lookahead,
            self.data,
            self.pos,
            &self.strategy,
        );
        self.pos += match &token {
            Token::Literal(_) => 1,
            Token::BackReference(matched) => matched.length() as usize,
        };
        Some(token)
    }
}

/// A backreference the lazy match has committed to using, plus how many more
/// literal bytes have to come out of [`next_token`] before it's due.
#[derive(Default)]
struct Lookahead {
    pending: Option<Backreference>,
    literals_before: usize,
}

/// Drains any pending lookahead state, else gets the longest match at `pos`
/// and hands it to [`chase_lazy_match`] to see if deferring finds something
/// better.
fn next_token(
    chains: &mut Chains,
    lookahead: &mut Lookahead,
    data: &[u8],
    pos: usize,
    strategy: &LazyMatch,
) -> Token {
    if lookahead.literals_before > 0 {
        lookahead.literals_before -= 1;
        return Token::Literal(data[pos]);
    }
    if let Some(matched) = lookahead.pending.take() {
        return Token::BackReference(matched);
    }

    let Some(best) = chains.longest_match(data, pos) else {
        return Token::Literal(data[pos]);
    };

    let (best, steps) = chase_lazy_match(chains, data, pos, strategy, best);

    if steps == 0 {
        return Token::BackReference(best);
    }
    lookahead.pending = Some(best);
    lookahead.literals_before = steps - 1;
    Token::Literal(data[pos])
}

/// Chases a longer match than `best` past `pos`, per `strategy`. Returns the
/// best match found and how many positions it deferred to find it;
/// `steps == 0` means `best` came back unchanged.
fn chase_lazy_match(
    chains: &mut Chains,
    data: &[u8],
    pos: usize,
    strategy: &LazyMatch,
    mut best: Backreference,
) -> (Backreference, usize) {
    let mut steps = 0;
    while steps < strategy.depth && pos + steps + 1 < data.len() {
        let Some(next) = chains.longest_match(data, pos + steps + 1) else {
            break;
        };
        if next.length() as usize <= best.length() as usize + strategy.slack {
            break;
        }
        best = next;
        steps += 1;
    }
    (best, steps)
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
