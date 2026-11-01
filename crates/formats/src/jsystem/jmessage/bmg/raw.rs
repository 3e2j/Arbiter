//! A message's text as one string: runs as they read, `{{` for a `{`, and
//! each tag as `{group:code}`, or `{group:code:args}` with its argument
//! bytes in hex.

use std::fmt::Write;

use super::{Encoding, TextSegment};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RawError {
    // "Not a tag at 3. Tags are {group:code}. For a { on its own, write {{."
    // Written like this below, just looks weird due to rust syntax doing the same thing.
    #[error("Not a tag at {at}. Tags are {{group:code}}. For a {{ on its own, write {{{{.")]
    Tag { at: usize },
    #[error("The file's encoding, {encoding:?}, can't hold {ch:?}.")]
    Unencodable { encoding: Encoding, ch: char },
}

/// One stretch of raw text, borrowed from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawPiece<'a> {
    Text(&'a str),
    Tag { group: u8, code: u16, args: Vec<u8> },
}

/// `raw` split into text and tags, a `{{` coming out as a lone `{`. Stops
/// after the first error.
pub fn pieces(raw: &str) -> impl Iterator<Item = Result<RawPiece<'_>, RawError>> {
    let mut rest = raw;
    std::iter::from_fn(move || {
        let open = rest.find('{');
        if open != Some(0) {
            let (text, after) = rest.split_at(open.unwrap_or(rest.len()));
            rest = after;
            return (!text.is_empty()).then_some(Ok(RawPiece::Text(text)));
        }
        let (brace, after) = rest.split_at(1);
        if let Some(after) = after.strip_prefix('{') {
            rest = after;
            return Some(Ok(RawPiece::Text(brace)));
        }
        let at = raw.len() - rest.len();
        let tag = after
            .split_once('}')
            .and_then(|(inner, tail)| Some((read_tag(inner)?, tail)));
        let Some((tag, tail)) = tag else {
            rest = "";
            return Some(Err(RawError::Tag { at }));
        };
        rest = tail;
        Some(Ok(tag))
    })
}

/// `group:code`, or `group:code:args` with the args in hex, as between a
/// tag's braces.
fn read_tag(inner: &str) -> Option<RawPiece<'static>> {
    let mut parts = inner.trim().splitn(3, ':');
    let group = parts.next()?.trim().parse().ok()?;
    let code = parts.next()?.trim().parse().ok()?;
    let hex = parts.next().unwrap_or_default().trim();
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let args = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok())
        .collect::<Option<_>>()?;
    Some(RawPiece::Tag { group, code, args })
}

/// Raw text as segments, its runs encoded in `encoding`.
///
/// # Errors
///
/// A `{` that opens no tag, or a character `encoding` can't hold.
pub fn read(raw: &str, encoding: Encoding) -> Result<Vec<TextSegment>, RawError> {
    let mut segments = Vec::new();
    let mut run = String::new();
    for piece in pieces(raw) {
        match piece? {
            RawPiece::Text(text) => run.push_str(text),
            RawPiece::Tag { group, code, args } => {
                flush(&mut run, encoding, &mut segments)?;
                segments.push(TextSegment::Tag { group, code, args });
            }
        }
    }
    flush(&mut run, encoding, &mut segments)?;
    Ok(segments)
}

fn flush(
    run: &mut String,
    encoding: Encoding,
    segments: &mut Vec<TextSegment>,
) -> Result<(), RawError> {
    if run.is_empty() {
        return Ok(());
    }
    let Some(bytes) = encoding.encode(run) else {
        let ch = run
            .chars()
            .find(|ch| encoding.encode(ch.encode_utf8(&mut [0; 4])).is_none())
            .unwrap_or('?');
        return Err(RawError::Unencodable { encoding, ch });
    };
    segments.push(TextSegment::Text(bytes));
    run.clear();
    Ok(())
}

/// `text` as raw text onto `out`, its runs read in `encoding`.
pub fn write(text: &[TextSegment], encoding: Encoding, out: &mut String) {
    for segment in text {
        match segment {
            TextSegment::Text(bytes) => escape(&encoding.decode(bytes), out),
            TextSegment::Tag { group, code, args } => write_tag(*group, *code, args, out),
        }
    }
}

/// One tag, braces and all, onto `out`.
pub fn write_tag(group: u8, code: u16, args: &[u8], out: &mut String) {
    let _ = write!(out, "{{{group}:{code}");
    if !args.is_empty() {
        out.push(':');
        for byte in args {
            let _ = write!(out, "{byte:02X}");
        }
    }
    out.push('}');
}

/// `text` onto `out` with each `{` doubled.
pub fn escape(text: &str, out: &mut String) {
    for piece in text.split_inclusive('{') {
        out.push_str(piece);
        if piece.ends_with('{') {
            out.push('{');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<TextSegment> {
        vec![
            TextSegment::Text(b"a{b".to_vec()),
            TextSegment::Tag {
                group: 0,
                code: 7,
                args: vec![0, 30],
            },
            TextSegment::Text(b"\nc".to_vec()),
        ]
    }

    #[test]
    fn text_writes_out_raw_and_reads_back() {
        let mut raw = String::new();
        write(&sample(), Encoding::Windows1252, &mut raw);
        assert_eq!(raw, "a{{b{0:7:001E}\nc");
        assert_eq!(read(&raw, Encoding::Windows1252), Ok(sample()));
        assert_eq!(read("{ 1 : 2 }", Encoding::Utf8).unwrap().len(), 1);
    }

    /// Only `{{` is a literal `{`, so a tag can sit inside literal braces
    /// and a literal brace can still be used.
    #[test]
    fn literal_braces_hold_a_tag() {
        let text = vec![
            TextSegment::Text(b"{something ".to_vec()),
            TextSegment::Tag {
                group: 0,
                code: 0,
                args: Vec::new(),
            },
            TextSegment::Text(b"!}".to_vec()),
        ];
        let raw = "{{something {0:0}!}";
        assert_eq!(read(raw, Encoding::Windows1252).as_ref(), Ok(&text));

        let mut written = String::new();
        write(&text, Encoding::Windows1252, &mut written);
        assert_eq!(written, raw);
    }

    #[test]
    fn what_doesnt_read_or_encode_is_refused() {
        let encoding = Encoding::Windows1252;
        assert_eq!(read("a {x} b", encoding), Err(RawError::Tag { at: 2 }));
        assert_eq!(read("{0:7:1}", encoding), Err(RawError::Tag { at: 0 }));
        assert_eq!(read("a {", encoding), Err(RawError::Tag { at: 2 }));
        assert_eq!(
            read("\u{30ea}", encoding),
            Err(RawError::Unencodable {
                encoding,
                ch: '\u{30ea}'
            })
        );
        assert!(read("\u{30ea}", Encoding::ShiftJis).is_ok());
    }
}
