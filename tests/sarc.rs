use binrw::{BinRead, BinReaderExt, Endian};
use smush_ui::sarc::{SarcArchive, SarcEntry};
use std::io::Cursor;

#[rustfmt::skip]
const SARC: &[u8] = &[
    // SARC header (0x00..0x14)
    b'S', b'A', b'R', b'C', // 0x00: Magic
    0x14, 0x00, // 0x04: Header size
    0xff, 0xfe, // 0x06: Little-endian BOM
    0x44, 0x00, 0x00, 0x00, // 0x08: Filesize
    0x40, 0x00, 0x00, 0x00, // 0x0c: Data section offset
    0x00, 0x01, // 0x10: Version
    0x00, 0x00, // 0x12: Padding

    // SFAT header (0x14..0x20)
    b'S', b'F', b'A', b'T', // 0x14: Identifier
    0x0c, 0x00, // 0x18: Header size
    0x01, 0x00, // 0x1a: Number of files
    0x65, 0x00, 0x00, 0x00, // 0x1c: Hash multiplier (101)

    // FAT entry (0x20..0x30)
    0x61, 0x00, 0x00, 0x00, // 0x20: Filename hash for "a"
    0x00, 0x00, 0x00, 0x01, // 0x24: Filename attributes, name at SFNT + 0
    0x00, 0x00, 0x00, 0x00, // 0x28: File data start
    0x04, 0x00, 0x00, 0x00, // 0x2c: File data end

    // SFNT header and filename table (0x30..0x3c)
    b'S', b'F', b'N', b'T', // 0x30: Identifier
    0x08, 0x00, // 0x34: Header size
    0x00, 0x00, // 0x36: Padding
    b'a', 0x00, 0x00, 0x00, // 0x38: NUL-terminated, 4-byte-aligned filename

    // Padding to data section (0x3c..0x40)
    0x00, 0x00, 0x00, 0x00,

    // File data (0x40..0x44)
    b'd', b'a', b't', b'a',
];

#[rustfmt::skip]
const COLLISION_SARC: &[u8] = &[
    // SARC header (0x00..0x14)
    b'S', b'A', b'R', b'C', // 0x00: Magic
    0x14, 0x00, // 0x04: Header size
    0xff, 0xfe, // 0x06: Little-endian BOM
    0x71, 0x00, 0x00, 0x00, // 0x08: Filesize
    0x60, 0x00, 0x00, 0x00, // 0x0c: Data section offset
    0x00, 0x01, // 0x10: Version
    0x00, 0x00, // 0x12: Padding

    // SFAT header (0x14..0x20)
    b'S', b'F', b'A', b'T', // 0x14: Identifier
    0x0c, 0x00, // 0x18: Header size
    0x02, 0x00, // 0x1a: Number of files
    0x65, 0x00, 0x00, 0x00, // 0x1c: Hash multiplier (101)

    // FAT entries (0x20..0x40): both names hash to 0x32c0e7f3 with multiplier 101
    0xf3, 0xe7, 0xc0, 0x32, // 0x20: Filename hash for "ymsrhiru"
    0x00, 0x00, 0x00, 0x01, // 0x24: First filename attributes, name at SFNT + 0
    0x00, 0x00, 0x00, 0x00, // 0x28: First file data start
    0x01, 0x00, 0x00, 0x00, // 0x2c: First file data end
    0xf3, 0xe7, 0xc0, 0x32, // 0x30: Filename hash for "josckzaz"
    0x03, 0x00, 0x00, 0x02, // 0x34: Second filename attributes, collision ordinal 2, name at SFNT + 12
    0x10, 0x00, 0x00, 0x00, // 0x38: Second file data start
    0x11, 0x00, 0x00, 0x00, // 0x3c: Second file data end

    // SFNT header and filename table (0x40..0x60)
    b'S', b'F', b'N', b'T', // 0x40: Identifier
    0x08, 0x00, // 0x44: Header size
    0x00, 0x00, // 0x46: Padding
    b'y', b'm', b's', b'r', b'h', b'i', b'r', b'u', 0x00, // 0x48: First filename
    0x00, 0x00, 0x00, // Alignment to 0x54
    b'j', b'o', b's', b'c', b'k', b'z', b'a', b'z', 0x00, // 0x54: Second filename
    0x00, 0x00, 0x00, // Alignment to data section

    // File data (0x60..0x71)
    b'x',
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Alignment to 0x70
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    b'y',
];

#[test]
fn parses_synthetic_archive() {
    let archive = SarcArchive::from_bytes(SARC).unwrap();
    assert_eq!((archive.version, archive.padding), (0x100, 0));
    assert_eq!(archive.entries.len(), 1);
    assert_eq!(archive.entries[0].name.as_deref(), Some("a"));
    assert_eq!(archive.entries[0].hash, 97);
    assert_eq!(archive.entries[0].data, b"data");
    let mut reader = Cursor::new(SARC);
    let read_archive: SarcArchive = reader.read_le().unwrap();
    assert_eq!(read_archive, archive);
}

#[test]
fn writes_synthetic_archives_byte_for_byte() {
    assert_eq!(
        SarcArchive::from_bytes(SARC).unwrap().to_bytes().unwrap(),
        SARC
    );
    assert_eq!(
        SarcArchive::from_bytes(COLLISION_SARC)
            .unwrap()
            .to_bytes()
            .unwrap(),
        COLLISION_SARC
    );
}

#[test]
fn derives_sorted_tables_names_offsets_and_archive_size() {
    let archive = SarcArchive::new(vec![
        SarcEntry::new("z", b"z".to_vec()),
        SarcEntry::new("a", b"aa".to_vec()),
    ]);

    let bytes = archive.to_bytes().unwrap();

    assert_eq!(read_u32(&bytes, 8), bytes.len() as u32);
    assert_eq!(read_u32(&bytes, 12), 0x50);
    assert_eq!(read_u32(&bytes, 0x20), b'a' as u32);
    assert_eq!(read_u32(&bytes, 0x24), 0x0100_0000);
    assert_eq!(read_u32(&bytes, 0x28), 0);
    assert_eq!(read_u32(&bytes, 0x2c), 2);
    assert_eq!(read_u32(&bytes, 0x30), b'z' as u32);
    assert_eq!(read_u32(&bytes, 0x34), 0x0100_0001);
    assert_eq!(read_u32(&bytes, 0x38), 0x10);
    assert_eq!(read_u32(&bytes, 0x3c), 0x11);
    assert_eq!(&bytes[0x48..0x50], b"a\0\0\0z\0\0\0");
    assert_eq!(&bytes[0x50..0x52], b"aa");
    assert_eq!(bytes[0x60], b'z');
    assert_eq!(bytes.len(), 0x61);
}

#[test]
fn aligns_ordinary_and_gpu_payloads_at_absolute_positions() {
    let ordinary = SarcArchive::new(vec![
        SarcEntry::new("a.bflyt", vec![1]),
        SarcEntry::new("b.info", vec![2]),
    ])
    .to_bytes()
    .unwrap();
    let ordinary_data = read_u32(&ordinary, 12);
    assert!(ordinary_data < 0x100);
    for record in [0x20, 0x30] {
        assert_eq!((ordinary_data + read_u32(&ordinary, record + 8)) % 0x10, 0);
    }

    let gpu = SarcArchive::new(vec![
        SarcEntry::new("timg/a.bntx", vec![1]),
        SarcEntry::new("shader/b.bnsh", vec![2]),
    ])
    .to_bytes()
    .unwrap();
    let gpu_data = read_u32(&gpu, 12);
    assert_eq!(gpu_data, 0x1000);
    for record in [0x20, 0x30] {
        assert_eq!((gpu_data + read_u32(&gpu, record + 8)) % 0x1000, 0);
    }
}

#[test]
fn deterministically_recalculates_offsets_when_layout_changes() {
    let mut archive = SarcArchive::from_bytes(SARC).unwrap();
    archive.entries[0].data.copy_from_slice(b"edit");
    let same_layout = archive.to_bytes().unwrap();
    assert_eq!(read_u32(&same_layout, 12), 0x40);
    assert_eq!(&same_layout[0x40..], b"edit");

    archive.entries[0].name = Some("a-name-that-no-longer-fits.bflyt".into());
    let relocated = archive.to_bytes().unwrap();
    assert!(read_u32(&relocated, 12) > 0x40);
    assert_eq!(read_u32(&relocated, 12) % 0x10, 0);
    let reparsed = SarcArchive::from_bytes(&relocated).unwrap();
    assert_eq!(
        reparsed.entries[0].name.as_deref(),
        Some("a-name-that-no-longer-fits.bflyt")
    );
    assert_eq!(reparsed.entries[0].data, b"edit");

    let mut stronger_alignment = SarcArchive::from_bytes(SARC).unwrap();
    stronger_alignment.entries[0].name = Some("a.bntx".into());
    let stronger_alignment = stronger_alignment.to_bytes().unwrap();
    assert_eq!(read_u32(&stronger_alignment, 12), 0x1000);
}

#[test]
fn writes_collisions_non_ascii_names_and_rejects_ambiguous_hash_only_entries() {
    let collision = SarcArchive::new(vec![
        SarcEntry::new("ymsrhiru", b"x".to_vec()),
        SarcEntry::new("josckzaz", b"y".to_vec()),
    ])
    .to_bytes()
    .unwrap();
    assert_eq!(read_u32(&collision, 0x24) >> 24, 1);
    assert_eq!(read_u32(&collision, 0x34) >> 24, 2);
    let reparsed = SarcArchive::from_bytes(&collision).unwrap();
    assert_eq!(reparsed.entries[0].hash, reparsed.entries[1].hash);

    let non_ascii = SarcArchive::new(vec![SarcEntry::new("é", b"data".to_vec())]);
    let reparsed = SarcArchive::from_bytes(&non_ascii.to_bytes().unwrap()).unwrap();
    assert_eq!(reparsed.entries[0].name.as_deref(), Some("é"));
    let expected_hash = [0xc3u8, 0xa9].into_iter().fold(0u32, |hash, byte| {
        hash.wrapping_mul(101)
            .wrapping_add(i32::from(byte as i8) as u32)
    });
    assert_eq!(reparsed.entries[0].hash, expected_hash);

    let duplicate_hash_only = SarcArchive::new(vec![
        SarcEntry::hash_only(7, vec![1]),
        SarcEntry::hash_only(7, vec![2]),
    ]);
    assert!(duplicate_hash_only.to_bytes().is_err());
}

#[test]
fn rewrites_added_removed_and_resized_entries_semantically() {
    let mut archive = SarcArchive::from_bytes(SARC).unwrap();
    archive.entries[0].data = b"resized payload".to_vec();
    archive
        .entries
        .push(SarcEntry::new("b.info", b"second".to_vec()));

    let reparsed = SarcArchive::from_bytes(&archive.to_bytes().unwrap()).unwrap();
    assert_eq!(reparsed.entries.len(), 2);
    assert_eq!(reparsed.entries[0].name.as_deref(), Some("a"));
    assert_eq!(reparsed.entries[0].data, b"resized payload");
    assert_eq!(reparsed.entries[1].name.as_deref(), Some("b.info"));
    assert_eq!(reparsed.entries[1].data, b"second");

    archive.entries.remove(0);
    let reparsed = SarcArchive::from_bytes(&archive.to_bytes().unwrap()).unwrap();
    assert_eq!(reparsed.entries.len(), 1);
    assert_eq!(reparsed.entries[0].name.as_deref(), Some("b.info"));
}

#[test]
fn rejects_unrepresentable_entry_counts_and_filenames() {
    let too_many = (0..0x4000)
        .map(|index| SarcEntry::hash_only(index, Vec::new()))
        .collect();
    assert!(SarcArchive::new(too_many).to_bytes().is_err());
    assert!(
        SarcArchive::new(vec![SarcEntry::new("bad\0name", Vec::new())])
            .to_bytes()
            .is_err()
    );
}

#[test]
fn reads_from_the_current_stream_position_and_stops_at_the_declared_end() {
    let prefix_length = 7;
    let archive_length = SARC.len();
    let mut containing_stream = vec![0xaa; prefix_length];
    containing_stream.extend_from_slice(SARC);
    containing_stream.extend([0xbb; 5]);
    let mut reader = Cursor::new(containing_stream);
    reader.set_position(prefix_length as u64);

    SarcArchive::read_options(&mut reader, Endian::Little, ()).unwrap();

    assert_eq!(reader.position(), (prefix_length + archive_length) as u64);
}

#[test]
fn rejects_sections_that_cross_the_declared_end() {
    let declared_length = 0x30u32;
    let mut containing_stream = SARC.to_vec();
    containing_stream[8..12].copy_from_slice(&declared_length.to_le_bytes());
    let mut reader = Cursor::new(containing_stream);

    assert!(SarcArchive::read_options(&mut reader, Endian::Little, ()).is_err());
}

#[test]
fn parses_hash_only_entries_and_named_hash_collisions() {
    let mut hash_only = SARC.to_vec();
    hash_only[36..40].copy_from_slice(&0u32.to_le_bytes());
    let archive = SarcArchive::from_bytes(&hash_only).unwrap();
    assert_eq!(archive.entries[0].name, None);

    let archive = SarcArchive::from_bytes(COLLISION_SARC).unwrap();
    assert_eq!(archive.entries[0].name.as_deref(), Some("ymsrhiru"));
    assert_eq!(archive.entries[1].name.as_deref(), Some("josckzaz"));
    assert_eq!(archive.entries[0].data, b"x");
    assert_eq!(archive.entries[1].data, b"y");
}

#[test]
fn sign_extends_non_ascii_filename_bytes_when_hashing() {
    let mut bytes = SARC.to_vec();
    bytes[56..59].copy_from_slice(b"\xc3\xa9\0");
    let hash = [0xc3u8, 0xa9].into_iter().fold(0u32, |hash, byte| {
        let signed_byte = u32::from_ne_bytes(i32::from(byte as i8).to_ne_bytes());
        hash.wrapping_mul(0x65).wrapping_add(signed_byte)
    });
    bytes[32..36].copy_from_slice(&hash.to_le_bytes());

    let archive = SarcArchive::from_bytes(&bytes).unwrap();

    assert_eq!(archive.entries[0].name.as_deref(), Some("é"));
}

fn invalid_at(mut b: Vec<u8>, at: usize, bytes: &[u8]) -> bool {
    b[at..at + bytes.len()].copy_from_slice(bytes);
    SarcArchive::from_bytes(&b).is_err()
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[test]
fn rejects_bad_headers_and_bounds() {
    let f = SARC.to_vec();
    assert!(invalid_at(f.clone(), 0, b"NOPE"));
    assert!(invalid_at(f.clone(), 20, b"NOPE"));
    assert!(invalid_at(f.clone(), 48, b"NOPE"));
    assert!(invalid_at(f.clone(), 6, &[0xfe, 0xff]));
    assert!(invalid_at(f.clone(), 16, &2u16.to_le_bytes()));
    assert!(invalid_at(f.clone(), 4, &0x18u16.to_le_bytes()));
    assert!(invalid_at(f.clone(), 24, &0x10u16.to_le_bytes()));
    assert!(invalid_at(f.clone(), 52, &0x10u16.to_le_bytes()));
    assert!(invalid_at(f.clone(), 26, &0x4000u16.to_le_bytes()));
    assert!(invalid_at(f.clone(), 28, &100u32.to_le_bytes()));
    assert!(invalid_at(f.clone(), 8, &0x13u32.to_le_bytes()));
    assert!(invalid_at(f.clone(), 8, &0x45u32.to_le_bytes()));
    assert!(invalid_at(f.clone(), 12, &0x38u32.to_le_bytes()));
    assert!(invalid_at(f.clone(), 36, &0x0100_0001u32.to_le_bytes()));
    let mut unterminated = f.clone();
    unterminated[56..64].fill(b'x');
    assert!(SarcArchive::from_bytes(&unterminated).is_err());
    assert!(invalid_at(f.clone(), 56, &[0xff]));
    assert!(invalid_at(f.clone(), 12, &0x45u32.to_le_bytes()));
    assert!(invalid_at(f.clone(), 40, &5u32.to_le_bytes()));
    assert!(invalid_at(f.clone(), 44, &0x100u32.to_le_bytes()));
    assert!(invalid_at(f.clone(), 32, &98u32.to_le_bytes()));
    assert!(SarcArchive::from_bytes(&f[..SARC_HEADER_LENGTH]).is_err());
}

const SARC_HEADER_LENGTH: usize = 0x14;

#[test]
fn rejects_malformed_collision_groups() {
    let fixture = COLLISION_SARC.to_vec();
    assert!(invalid_at(
        fixture.clone(),
        52,
        &0x0300_0000u32.to_le_bytes()
    ));
    assert!(invalid_at(
        fixture.clone(),
        52,
        &0x0200_0000u32.to_le_bytes()
    ));
    assert!(invalid_at(fixture.clone(), 52, &0u32.to_le_bytes()));
    assert!(invalid_at(fixture.clone(), 48, &194u32.to_le_bytes()));
    assert!(invalid_at(fixture, 56, &0u32.to_le_bytes()));
}

#[test]
fn conditionally_parses_local_arc_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_assets");
    if !root.exists() {
        return;
    }
    let mut pending = vec![root];
    let mut count = 0;
    while let Some(dir) = pending.pop() {
        for item in std::fs::read_dir(dir).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "arc") {
                let bytes = std::fs::read(&path).unwrap();
                let archive = SarcArchive::from_bytes(&bytes)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert_eq!(
                    archive
                        .to_bytes()
                        .unwrap_or_else(|e| panic!("{}: {e}", path.display())),
                    bytes,
                    "{}",
                    path.display()
                );
                count += 1;
            }
        }
    }
    assert!(count > 0, "local ARC fixture directory should not be empty");
    eprintln!("parsed {count} local ARC fixtures");
}
