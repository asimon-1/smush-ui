#[path = "common/bntx.rs"]
mod common_bntx;

use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use common_bntx::synthetic_bntx;
use smush_ui::bflan::{Bflan, Pat1, Section as BflanSection};
use smush_ui::bflyt::{Bflyt, Section};
use smush_ui::sarc::{
    SarcArchive, SarcEntry,
    inspect::{
        DetectedFileKind, SarcDiffOptions, SarcEntryIdentity, diff, diff_with_options, inspect,
    },
};

#[rustfmt::skip]
const BFLYT: &[u8] = &[
    b'F', b'L', b'Y', b'T', 0xff, 0xfe, 0x14, 0x00,
    0x02, 0x02, 0x00, 0x00, 0x35, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00,
    b'l', b'y', b't', b'1', 0x21, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00,
    0x41, 0x20, 0x00, 0x00, 0x42, 0x48, 0x00, 0x00,
    0x43, 0x16, 0x00, 0x00, 0x43, 0x48, 0x00, 0x00,
    b'M', b'a', b'i', b'n', 0x00,
];

#[test]
fn expanded_archive_diff_reports_bflan_field_changes() {
    let make = |name: &str| {
        Bflan::new(vec![BflanSection::Pat1(Pat1 {
            order: 1,
            user_data: None,
            start_frame: 0,
            end_frame: 4,
            child_binding: false,
            name: name.into(),
            groups: vec![],
            group_record_size: Default::default(),
        })])
        .to_bytes()
        .unwrap()
    };
    let before = SarcArchive::new(vec![SarcEntry::new("anim/open.bflan", make("Open"))]);
    let after = SarcArchive::new(vec![SarcEntry::new("anim/open.bflan", make("Close"))]);
    assert_eq!(
        inspect(&before).entries[0].detected_kind,
        DetectedFileKind::Bflan
    );
    let difference = diff_with_options(
        &before,
        &after,
        SarcDiffOptions {
            expand_bflan: true,
            ..Default::default()
        },
    );
    let changes = &difference.changed[0].bflan.as_ref().unwrap().changes;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, "/sections/0/Pat1/name");
}

#[test]
fn inspection_reports_identity_hash_content_and_magic_separately() {
    let archive = SarcArchive::new(vec![
        SarcEntry::new("blyt/Hud.BFLYT", b"FLYT\xff\xfe\x14\x00payload".to_vec()),
        SarcEntry::hash_only(0x1234_abcd, b"opaque".to_vec()),
        SarcEntry::new("timg/icon.bntx.backup", b"BNTX\0\0\0\0payload".to_vec()),
    ]);

    let inspection = inspect(&archive);
    assert_eq!(3, inspection.entry_count);
    assert_eq!(0, inspection.entries[0].stored_index);
    assert_eq!(DetectedFileKind::Bflyt, inspection.entries[0].detected_kind);
    assert_eq!(Some("bflyt"), inspection.entries[0].extension.as_deref());
    assert_eq!(64, inspection.entries[0].payload_sha256.len());
    assert_eq!(
        SarcEntryIdentity::Path {
            path: "timg/icon.bntx.backup".into()
        },
        inspection.entries[1].identity
    );
    assert_eq!(DetectedFileKind::Bntx, inspection.entries[1].detected_kind);
    assert_eq!(Some("backup"), inspection.entries[1].extension.as_deref());
    assert_eq!(
        SarcEntryIdentity::Hash {
            hash: "1234abcd".into()
        },
        inspection.entries[2].identity
    );
    assert_eq!(1, inspection.entries[2].stored_index);
}

#[test]
fn inspection_sorts_paths_independently_of_storage_order_and_keeps_indices() {
    let archive = SarcArchive::new(vec![
        SarcEntry::new("z.bin", vec![1]),
        SarcEntry::new("a.bin", vec![2]),
        SarcEntry::hash_only(0x1234_5678, vec![3]),
    ]);

    let inspection = inspect(&archive);
    assert_eq!(
        inspection
            .entries
            .iter()
            .map(|entry| (entry.identity.clone(), entry.stored_index))
            .collect::<Vec<_>>(),
        [
            (
                SarcEntryIdentity::Path {
                    path: "a.bin".into()
                },
                1
            ),
            (
                SarcEntryIdentity::Path {
                    path: "z.bin".into()
                },
                0
            ),
            (
                SarcEntryIdentity::Hash {
                    hash: "12345678".into()
                },
                2
            ),
        ]
    );
}

#[test]
fn diff_reports_added_removed_and_payload_changes_in_identity_order() {
    let before = SarcArchive::new(vec![
        SarcEntry::new("changed.bin", vec![1, 2]),
        SarcEntry::new("removed.bin", vec![3]),
        SarcEntry::hash_only(0x0102_0304, vec![4]),
    ]);
    let after = SarcArchive::new(vec![
        SarcEntry::new("added.bin", vec![5]),
        SarcEntry::new("changed.bin", vec![1, 2, 3]),
        SarcEntry::hash_only(0x0102_0304, vec![4]),
    ]);

    let difference = diff(&before, &after);
    assert!(difference.has_differences());
    assert_eq!(1, difference.added.len());
    assert_eq!(1, difference.removed.len());
    assert_eq!(1, difference.changed.len());
    assert_eq!(2, difference.changed[0].before_payload_size);
    assert_eq!(3, difference.changed[0].after_payload_size);
    assert_eq!(0, difference.changed[0].before_stored_index);
    assert_eq!(1, difference.changed[0].after_stored_index);
}

#[test]
fn stored_index_changes_alone_are_not_payload_changes() {
    let before = SarcArchive::new(vec![
        SarcEntry::new("a.bin", vec![1]),
        SarcEntry::new("b.bin", vec![2]),
    ]);
    let after = SarcArchive::new(vec![
        SarcEntry::new("b.bin", vec![2]),
        SarcEntry::new("a.bin", vec![1]),
    ]);

    assert!(!diff(&before, &after).has_differences());
}

#[test]
fn independent_expansion_flags_attach_typed_nested_diffs() {
    let before_bntx = synthetic_bntx();
    let mut after_bntx = before_bntx.clone();
    after_bntx.textures[0].image_data[0] ^= 0xff;

    let mut after_bflyt: Bflyt = Cursor::new(BFLYT).read_le().unwrap();
    match &mut after_bflyt.sections[0] {
        Section::Layout(layout) => layout.width += 1.0,
        _ => unreachable!(),
    }
    let mut after_bflyt_bytes = Cursor::new(Vec::new());
    after_bflyt_bytes.write_le(&after_bflyt).unwrap();

    let before = SarcArchive::new(vec![
        SarcEntry::new("blyt/main.bflyt", BFLYT.to_vec()),
        SarcEntry::new("timg/main.bntx", before_bntx.to_bytes().unwrap()),
    ]);
    let after = SarcArchive::new(vec![
        SarcEntry::new("blyt/main.bflyt", after_bflyt_bytes.into_inner()),
        SarcEntry::new("timg/main.bntx", after_bntx.to_bytes().unwrap()),
    ]);

    let bflyt_only = diff_with_options(
        &before,
        &after,
        SarcDiffOptions {
            expand_bflyt: true,
            expand_bflan: false,
            expand_bntx: false,
        },
    );
    assert!(
        bflyt_only.changed[0]
            .bflyt
            .as_ref()
            .unwrap()
            .has_differences()
    );
    assert!(
        bflyt_only
            .changed
            .iter()
            .all(|change| change.bntx.is_none())
    );

    let combined = diff_with_options(
        &before,
        &after,
        SarcDiffOptions {
            expand_bflyt: true,
            expand_bflan: false,
            expand_bntx: true,
        },
    );
    assert!(combined.changed[0].bflyt.is_some());
    assert!(combined.changed[1].bntx.as_ref().unwrap().has_differences());
}

#[test]
fn failed_nested_parse_falls_back_to_opaque_outer_change() {
    let before = SarcArchive::new(vec![SarcEntry::new(
        "timg/broken.bntx",
        b"BNTX\0\0\0\0before".to_vec(),
    )]);
    let after = SarcArchive::new(vec![SarcEntry::new(
        "timg/broken.bntx",
        b"BNTX\0\0\0\0after".to_vec(),
    )]);

    let difference = diff_with_options(
        &before,
        &after,
        SarcDiffOptions {
            expand_bflyt: false,
            expand_bflan: false,
            expand_bntx: true,
        },
    );
    assert_eq!(1, difference.changed.len());
    assert!(difference.changed[0].bntx.is_none());
}
