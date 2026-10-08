use super::*;
use crate::Error;
use crate::Record;

fn file(path: &str, data: &[u8]) -> File {
    File {
        path: path.to_owned(),
        data: data.to_vec(),
        ..File::default()
    }
}

fn rarc(files: Vec<File>) -> Rarc {
    Rarc {
        root: "root".to_owned(),
        files,
        next_id: None,
    }
}

fn encode(rarc: &Rarc) -> Result<Vec<u8>> {
    let mut out = Writer::new();
    rarc.encode(&mut out)?;
    Ok(out.finish())
}

fn decode(bytes: &[u8]) -> Result<Rarc> {
    Rarc::decode(bytes, &mut Diagnostics::default())
}

fn fixture() -> Vec<u8> {
    encode(&rarc(vec![
        file("a.bin", b"AAAAA"),
        file("sub/b.bin", b"BBB"),
    ]))
    .unwrap()
}

// Where the fixture's sections land.
const NODES: usize = 0x40;
const ENTRIES: usize = 0x60;
const NAMES: usize = 0x100;
const DATA: usize = 0x120;
/// `a.bin` in `.\0..\0root\0a.bin\0sub\0b.bin\0`.
const NAME_A: usize = 0x0A;

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn set_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
}

fn set_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

fn entry_at(index: usize) -> usize {
    ENTRIES + index * Entry::LEN
}

/// Renames `a.bin` in place, keeping its hash honest.
fn rename_a(bytes: &mut [u8], name: &[u8]) {
    bytes[NAMES + NAME_A..][..name.len()].copy_from_slice(name);
    set_u16(bytes, entry_at(0) + 2, name_hash(name));
}

/// The fixture field by field against the retail layout. Every other test
/// trusts `encode`, and this is what earns it.
#[test]
fn encodes_the_retail_layout() {
    let data = fixture();
    assert_eq!(data.len(), 0x160);

    let header: Vec<u32> = (0..8).map(|i| u32_at(&data, i * 4)).collect();
    assert_eq!(
        header,
        [
            u32::from_be_bytes(*b"RARC"),
            0x160,
            0x20,
            0x100,
            0x40,
            0x40,
            0,
            0
        ]
    );
    let info: Vec<u32> = (0..6).map(|i| u32_at(&data, INFO_AT + i * 4)).collect();
    assert_eq!(info, [2, 0x20, 7, 0x40, 0x20, 0xE0]);
    // Two ids against seven entries isn't synced, so the counter is past the
    // highest id.
    assert_eq!(u16_at(&data, INFO_AT + 0x18), 2);
    assert_eq!(data[INFO_AT + 0x1A], 0);

    assert_eq!(&data[NODES..NODES + 4], b"ROOT");
    assert_eq!(u32_at(&data, NODES + 4), 5);
    assert_eq!(u16_at(&data, NODES + 8), name_hash(b"root"));
    assert_eq!(u16_at(&data, NODES + 10), 4);
    assert_eq!(u32_at(&data, NODES + 12), 0);
    assert_eq!(&data[NODES + 16..NODES + 20], b"SUB ");
    assert_eq!(u16_at(&data, NODES + 26), 3);
    assert_eq!(u32_at(&data, NODES + 28), 4);

    // The root's run is `a.bin`, `sub`, `.`, `..`. Ids go out in the order
    // files are met, not by entry index: `b.bin` is 1, not 4.
    let entry = |index: usize| {
        let at = entry_at(index);
        (
            u16_at(&data, at),
            u32_at(&data, at + 4),
            u32_at(&data, at + 8),
            u32_at(&data, at + 12),
        )
    };
    assert_eq!(entry(0), (0, 0x1100_000A, 0, 5));
    assert_eq!(entry(1), (0xFFFF, 0x0200_0010, 1, 0x10));
    assert_eq!(entry(2), (0xFFFF, 0x0200_0000, 0, 0x10));
    assert_eq!(entry(3), (0xFFFF, 0x0200_0002, u32::MAX, 0x10));
    assert_eq!(entry(4), (1, 0x1100_0014, 0x20, 3));
    assert_eq!(entry(5), (0xFFFF, 0x0200_0000, 1, 0x10));
    assert_eq!(entry(6), (0xFFFF, 0x0200_0002, 0, 0x10));

    assert_eq!(
        &data[NAMES..DATA],
        b".\0..\0root\0a.bin\0sub\0b.bin\0\0\0\0\0\0\0"
    );
    assert_eq!(&data[DATA..DATA + 5], b"AAAAA");
    assert_eq!(&data[DATA + 0x20..DATA + 0x23], b"BBB");
    assert!(data[DATA + 0x23..].iter().all(|&b| b == 0));
}

#[test]
fn round_trips() {
    let data = fixture();
    assert!(Rarc::detect(&data));
    let decoded = decode(&data).unwrap();
    assert_eq!(decoded.root, "root");
    let files: Vec<_> = decoded
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.data.as_slice(), f.id, f.preload))
        .collect();
    assert_eq!(
        files,
        [
            ("a.bin", b"AAAAA".as_slice(), Some(0), Preload::Mram),
            ("sub/b.bin", b"BBB".as_slice(), Some(1), Preload::Mram),
        ]
    );
    assert_eq!(encode(&decoded).unwrap(), data);
}

#[test]
fn an_empty_archive_keeps_its_root() {
    let data = encode(&Rarc {
        root: "bmgres99".to_owned(),
        ..Rarc::default()
    })
    .unwrap();
    let decoded = decode(&data).unwrap();
    assert_eq!(decoded.root, "bmgres99");
    assert_eq!(decoded.files, []);
    assert_eq!(encode(&decoded).unwrap(), data);
}

/// `a` and everything under it is numbered before `b`, and the two `sub`s
/// are separate directories.
#[test]
fn directories_number_depth_first() {
    let data = encode(&rarc(vec![
        file("a/x.bin", b"X"),
        file("a/sub/y.bin", b"Y"),
        file("b/sub/z.bin", b"Z"),
    ]))
    .unwrap();
    assert_eq!(u32_at(&data, INFO_AT), 5);
    assert_eq!(u32_at(&data, INFO_AT + 8), 17);

    let node = |index: usize| {
        let at = NODES + index * Node::LEN;
        (
            &data[at..at + 4],
            u16_at(&data, at + 10),
            u32_at(&data, at + 12),
        )
    };
    assert_eq!(node(0), (b"ROOT".as_slice(), 4, 0));
    assert_eq!(node(1), (b"A   ".as_slice(), 4, 4));
    assert_eq!(node(2), (b"SUB ".as_slice(), 3, 8));
    assert_eq!(node(3), (b"B   ".as_slice(), 3, 11));
    assert_eq!(node(4), (b"SUB ".as_slice(), 3, 14));

    let entries = INFO_AT + u32_at(&data, INFO_AT + 12) as usize;
    let parent = |index: usize| u32_at(&data, entries + index * Entry::LEN + 8);
    assert_eq!(parent(10), 1);
    assert_eq!(parent(16), 3);

    let decoded = decode(&data).unwrap();
    let paths: Vec<_> = decoded.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["a/x.bin", "a/sub/y.bin", "b/sub/z.bin"]);
    assert_eq!(encode(&decoded).unwrap(), data);
}

#[test]
fn ids_matching_entries_are_synced() {
    let data = encode(&rarc(vec![file("a.bin", b"A"), file("b.bin", b"B")])).unwrap();
    assert_eq!(data[INFO_AT + 0x1A], 1);
    // Two files, then the root's `.` and `..`.
    assert_eq!(u16_at(&data, INFO_AT + 0x18), 4);
    let decoded = decode(&data).unwrap();
    assert_eq!(decoded.next_id, None);
    assert_eq!(encode(&decoded).unwrap(), data);
}

/// Nothing derives a counter of its own, so it's carried.
#[test]
fn an_odd_next_id_is_carried() {
    let mut data = fixture();
    set_u16(&mut data, INFO_AT + 0x18, 5);
    let decoded = decode(&data).unwrap();
    assert_eq!(decoded.next_id, Some(5));
    assert_eq!(encode(&decoded).unwrap(), data);
}

/// A new id never repeats one carried, even one sitting at its entry index.
#[test]
fn new_ids_skip_carried_ones() {
    let mut files = vec![file("a.bin", b"A"), file("sub/b.bin", b"B")];
    files[0].id = Some(4);
    files[1].id = None;
    let decoded = decode(&encode(&rarc(files)).unwrap()).unwrap();
    assert_eq!(decoded.files[0].id, Some(4));
    assert_eq!(decoded.files[1].id, Some(0));
}

#[test]
fn shared_ids_are_refused() {
    let mut files = vec![file("a.bin", b"A"), file("b.bin", b"B")];
    files[0].id = Some(1);
    files[1].id = Some(1);
    assert!(matches!(encode(&rarc(files)), Err(Error::Malformed { .. })));
}

#[test]
fn compression_flags_follow_the_bytes() {
    let data = encode(&rarc(vec![
        file("a.bin", b"Yaz0 in shape only"),
        file("b.bin", b"Yay0 in shape only"),
    ]))
    .unwrap();
    assert_eq!(data[entry_at(0) + 4], 0x95);
    assert_eq!(data[entry_at(1) + 4], 0x15);
}

#[test]
fn preload_sizes_split_by_memory() {
    let mut files = vec![file("a.bin", b"A"), file("sub/b.bin", b"B")];
    files[1].preload = Preload::Aram;
    let data = encode(&rarc(files)).unwrap();
    assert_eq!(u32_at(&data, 0x14), 0x20);
    assert_eq!(u32_at(&data, 0x18), 0x20);
    assert_eq!(decode(&data).unwrap().files[1].preload, Preload::Aram);
}

/// Checked in data order, not list order: `a.bin` goes out before
/// `sub/b.bin` however they're listed.
#[test]
fn memories_out_of_data_order_are_refused() {
    let listed = || vec![file("sub/b.bin", b"B"), file("a.bin", b"A")];
    let mut files = listed();
    files[1].preload = Preload::Aram;
    assert!(matches!(encode(&rarc(files)), Err(Error::Malformed { .. })));
    let mut files = listed();
    files[0].preload = Preload::Aram;
    assert!(encode(&rarc(files)).is_ok());
}

#[test]
fn unusable_paths_are_refused() {
    for path in ["sub/../b.bin", "", "sub//b.bin", ".", "a\\b", "a.bin/x"] {
        let files = vec![file("a.bin", b"A"), file(path, b"")];
        assert!(
            matches!(encode(&rarc(files)), Err(Error::Name { .. })),
            "{path:?}"
        );
    }
    let files = vec![file("a.bin", b"A"), file("a.bin", b"B")];
    assert!(matches!(encode(&rarc(files)), Err(Error::Name { .. })));
}

#[test]
fn shift_jis_names_round_trip() {
    let mut data = fixture();
    // Halfwidth katakana RI, one byte in Shift-JIS.
    rename_a(&mut data, b"\xD8.bin");
    let decoded = decode(&data).unwrap();
    assert_eq!(decoded.files[0].path, "ﾘ.bin");
    assert_eq!(encode(&decoded).unwrap(), data);
}

#[test]
fn bad_names_are_refused() {
    // Whether each is refused as a name rather than as malformed.
    let cases: [(&[u8], bool); 3] = [
        (b"a/bin", true),
        // A lead byte with no trail.
        (b"\x85.bin", false),
        // A byte order mark doesn't switch the encoding.
        (b"\xFF\xFEAA", false),
    ];
    for (name, as_name) in cases {
        let mut data = fixture();
        rename_a(&mut data, name);
        let err = decode(&data).unwrap_err();
        assert_eq!(
            matches!(err, Error::Name { .. }),
            as_name,
            "{name:?}: {err}"
        );
        assert_eq!(
            matches!(err, Error::Malformed { .. }),
            !as_name,
            "{name:?}: {err}"
        );
    }
    let mut data = fixture();
    set_u16(&mut data, entry_at(0) + 2, 0xBEEF);
    assert!(matches!(decode(&data), Err(Error::Malformed { .. })));
}

#[test]
fn broken_structure_is_refused() {
    let corruptions: [fn(&mut Vec<u8>); 10] = [
        |d| d.truncate(d.len() - 4),
        |d| set_u32(d, 0x10, 0),
        |d| set_u32(d, 0x14, u32::MAX),
        |d| set_u32(d, INFO_AT, 0),
        |d| set_u32(d, INFO_AT, u32::MAX),
        |d| set_u32(d, INFO_AT + 8, u32::MAX),
        |d| set_u16(d, NODES + 10, 100),
        // `sub` back at the root.
        |d| set_u32(d, entry_at(1) + 8, 0),
        |d| set_u32(d, entry_at(1) + 8, 9),
        |d| d[entry_at(0) + 4] = flag::FILE,
    ];
    for (n, corrupt) in (0..).zip(corruptions) {
        let mut data = fixture();
        corrupt(&mut data);
        assert!(decode(&data).is_err(), "corruption {n}");
    }
    assert!(matches!(decode(b"Yaz0...."), Err(Error::WrongMagic { .. })));
}
