use super::*;

fn sample() -> Bmg {
    Bmg {
        encoding: Encoding::ShiftJis,
        record_len: 6,
        mid1: Some(Mid1Header::default()),
        messages: vec![Message {
            id: 5,
            key: MessageKey(0),
            attributes: Box::new([0, 5]),
            text: vec![TextSegment::Text(b"Hi".to_vec())],
        }],
        flow: Some(Flow {
            nodes: vec![Node::Text {
                id: NodeId(0),
                message: MessageKey(0),
                next: None,
            }],
            roots: vec![Root {
                id: 3000,
                node: NodeId(0),
            }],
        }),
        strings: None,
    }
}

fn encode(bmg: &Bmg) -> Result<Vec<u8>> {
    let mut out = ByteWriter::new();
    bmg.encode(&mut out)?;
    Ok(out.finish())
}

fn decode(bytes: &[u8]) -> Result<Bmg> {
    Bmg::decode(bytes, &mut Diagnostics::default())
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// A file of `magics`, one 0x10 section each, stating `size`.
fn bare(size: u32, magics: &[&[u8; 4]]) -> Vec<u8> {
    let mut out = ByteWriter::new();
    out.bytes(b"MESGbmg1");
    out.u32(size);
    out.u32(u32::try_from(magics.len()).unwrap());
    out.u8(Encoding::ShiftJis.byte());
    out.zeros(15);
    for magic in magics {
        out.bytes(*magic);
        out.u32(0x10);
        out.zeros(8);
    }
    out.finish()
}

/// Five sections each padded to 0x20 but the last, the stated size stopping
/// before the flow pair.
#[test]
fn encodes_the_retail_layout() {
    let data = encode(&sample()).unwrap();
    assert_eq!(&data[..8], b"MESGbmg1");
    assert_eq!(u32_at(&data, 8), 0x80);
    assert_eq!(u32_at(&data, 12), 5);
    assert_eq!(data[16], Encoding::ShiftJis.byte());
    assert!(data[17..0x20].iter().all(|&b| b == 0));
    for (at, magic) in [
        (0x20, b"INF1"),
        (0x40, b"DAT1"),
        (0x60, b"MID1"),
        (0x80, b"FLW1"),
        (0xA0, b"FLI1"),
    ] {
        assert_eq!(
            (&data[at..at + 4], u32_at(&data, at + 4)),
            (&magic[..], 0x20)
        );
    }
    // FLI1's header and one root, unpadded.
    assert_eq!(data.len(), 0xA8 + 0x10);
}

/// Without a flow pair the stated size is the whole file, the last section's
/// dropped padding included.
#[test]
fn the_stated_size_without_flow_is_the_whole_file() {
    let data = encode(&Bmg {
        flow: None,
        ..sample()
    })
    .unwrap();
    assert_eq!(u32_at(&data, 8), 0x80);
    assert_eq!(u32_at(&data, 12), 3);
    assert_eq!(data.len(), 0x60 + 8 + 8 + 4);
}

#[test]
fn round_trips() {
    let data = encode(&sample()).unwrap();
    assert!(Bmg::detect(&data));
    assert_eq!(decode(&data).unwrap(), sample());

    // A pool filling its section is the one that comes back as it went.
    let mut strings = vec![Vec::new(), b"arrow".to_vec()];
    strings.resize(20, Vec::new());
    let bmg = Bmg {
        flow: None,
        strings: Some(strings),
        ..sample()
    };
    assert_eq!(decode(&encode(&bmg).unwrap()).unwrap(), bmg);

    // Addressed by position, so the public id has nowhere to go.
    let mut bmg = Bmg {
        mid1: None,
        flow: None,
        ..sample()
    };
    bmg.messages[0].id = 0;
    assert_eq!(decode(&encode(&bmg).unwrap()).unwrap(), bmg);
}

#[test]
fn the_encoding_byte_round_trips() {
    for byte in 0x00..=0x04 {
        assert_eq!(Encoding::from_byte(byte).byte(), byte);
    }
    assert_eq!(Encoding::from_byte(0xFF), Encoding::ShiftJis);
}

#[test]
fn the_stated_size_leaves_the_flow_pair_out() {
    let magics = [b"INF1", b"DAT1", b"FLW1", b"FLI1"];
    let data = bare(0x40, &magics);
    let (_, sections) = decode::split(&data).unwrap();
    assert!(sections.flow.is_some());
    assert!(matches!(
        decode::split(&bare(0x60, &magics)),
        Err(FormatError::Malformed { .. })
    ));
}

#[test]
fn broken_section_tables_are_refused() {
    let malformed = |data: &[u8]| matches!(decode::split(data), Err(FormatError::Malformed { .. }));
    assert!(malformed(&bare(0x50, &[b"INF1", b"DAT1", b"XXXX"])));
    assert!(malformed(&bare(0x50, &[b"INF1", b"DAT1", b"DAT1"])));
    assert!(malformed(&bare(0x40, &[b"INF1", b"DAT1", b"FLW1"])));
    assert!(malformed(&bare(0x30, &[b"INF1"])));

    let mut kind = bare(0x40, &[b"INF1", b"DAT1"]);
    kind[4..8].copy_from_slice(b"bmg2");
    assert!(!Bmg::detect(&kind));
    assert_eq!(
        decode(&kind),
        Err(FormatError::WrongMagic {
            expected: Bmg::KIND
        })
    );
}

#[test]
fn unwritable_files_are_refused() {
    let bmg = Bmg {
        strings: Some(vec![b"a\0b".to_vec()]),
        ..sample()
    };
    assert!(matches!(encode(&bmg), Err(FormatError::Malformed { .. })));

    let mut bmg = sample();
    bmg.messages.push(bmg.messages[0].clone());
    assert!(matches!(encode(&bmg), Err(FormatError::Malformed { .. })));
}

#[test]
fn text_decodes_in_its_encoding() {
    assert_eq!(Encoding::Windows1252.decode(b"caf\xe9 \x85"), "café …");
    assert_eq!(
        Encoding::ShiftJis.decode(b"\x83\x8a\x83\x93\x83N"),
        "リンク"
    );
    assert_eq!(Encoding::Utf16Be.decode(b"\x00H\x00i"), "Hi");
    assert_eq!(Encoding::Utf8.decode(b"\xff"), "\u{fffd}");
}
