use std::{fs, io::Cursor, path::Path};

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::Bflyt;

#[rustfmt::skip]
const BFLYT: &[u8] = &[
    b'F', b'L', b'Y', b'T', // Magic
    0xff, 0xfe, // BOM
    0x14, 0x00, // Header size
    0x02, 0x02, 0x00, 0x00, // Version
    0x35, 0x00, 0x00, 0x00, // Filesize
    0x01, 0x00, // Number of sections
    0x00, 0x00, // Padding
    // lyt1 section
    b'l', b'y', b't', b'1', // Magic
    0x21, 0x00, 0x00, 0x00, // Section Size
    0x01, // Is Drawn from Middle
    0x00, 0x00, 0x00, // Padding
    0x41, 0x20, 0x00, 0x00, // Width
    0x42, 0x48, 0x00, 0x00, // Height
    0x43, 0x16, 0x00, 0x00, // Maximum parts height
    0x43, 0x48, 0x00, 0x00, // Maximum parts width
    b'M', b'a', b'i', b'n', 0x00, // NUL-terminated layout name
];

#[test]
fn reads_a_bflyt_layout() {
    let mut input = Cursor::new(BFLYT);

    let _bflyt: Bflyt = input.read_le().expect("fixture should parse");

    assert_eq!(input.position(), BFLYT.len() as u64);
}

#[test]
fn writes_the_original_bflyt_bytes() {
    let mut input = Cursor::new(BFLYT);
    let bflyt: Bflyt = input.read_le().expect("fixture should parse");
    let mut output = Cursor::new(Vec::new());

    output.write_le(&bflyt).expect("parsed layout should write");

    assert_eq!(output.into_inner(), BFLYT);
}

#[test]
fn parses_fields() {
    let mut input = Cursor::new(BFLYT);
    let _bflyt: Bflyt = input.read_le().expect("fixture should parse");
    assert_eq!(_bflyt.version, [0x2, 0x2, 0x0, 0x0]);
    assert_eq!(_bflyt.filesize, 0x35);
    assert_eq!(_bflyt.num_sections, 1);
    assert_eq!(_bflyt.sections.len(), 1);
    assert_eq!(
        _bflyt.sections[0]
            .as_layout()
            .expect("section couldn't parse as layout")
            .name,
        "Main"
    );
}

#[test]
fn serializes_to_json_and_back() {
    let mut input = Cursor::new(BFLYT);
    let bflyt: Bflyt = input.read_le().expect("fixture should parse");

    let json = serde_json::to_string_pretty(&bflyt).expect("layout should serialize to JSON");
    let restored: Bflyt = serde_json::from_str(&json).expect("JSON should deserialize to a layout");
    let mut output = Cursor::new(Vec::new());

    output
        .write_le(&restored)
        .expect("JSON layout should write");
    assert_eq!(output.into_inner(), BFLYT);
}

#[test]
fn every_real_bflyt_fixture_round_trips_through_json() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_assets");
    let fixtures: Vec<_> = fs::read_dir(&fixture_dir)
        .expect("test_assets directory should exist")
        .map(|entry| {
            entry
                .expect("fixture directory entries should be readable")
                .path()
        })
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "bflyt")
        })
        .collect();

    assert!(
        !fixtures.is_empty(),
        "test_assets should contain at least one .bflyt fixture"
    );

    for path in fixtures {
        let bytes = fs::read(&path)
            .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
        let mut input = Cursor::new(bytes.as_slice());
        let bflyt: Bflyt = input
            .read_le()
            .unwrap_or_else(|error| panic!("{} should parse: {error}", path.display()));

        assert_eq!(
            input.position(),
            bytes.len() as u64,
            "{} should be consumed completely",
            path.display()
        );
        assert_eq!(
            bflyt.filesize as usize,
            bytes.len(),
            "{} should have an accurate header filesize",
            path.display()
        );

        let json = serde_json::to_vec(&bflyt)
            .unwrap_or_else(|error| panic!("{} should serialize to JSON: {error}", path.display()));
        let restored: Bflyt = serde_json::from_slice(&json)
            .unwrap_or_else(|error| panic!("{} JSON should deserialize: {error}", path.display()));
        let mut output = Cursor::new(Vec::new());

        output.write_le(&restored).unwrap_or_else(|error| {
            panic!("{} JSON should write as BFLYT: {error}", path.display())
        });
        let written = output.into_inner();
        let reparsed: Bflyt = Cursor::new(written.as_slice())
            .read_le()
            .unwrap_or_else(|error| panic!("{} JSON output should parse: {error}", path.display()));
        assert_eq!(
            serde_json::to_value(&reparsed).expect("reparsed layout should serialize"),
            serde_json::to_value(&restored).expect("restored layout should serialize"),
            "{} should survive binary -> JSON -> binary",
            path.display()
        );
    }
}

#[test]
fn pane_boundaries_are_a_json_children_hierarchy() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_assets/com_btn_arrow_00.bflyt");
    let bytes = fs::read(path).expect("pane hierarchy fixture should be readable");
    let bflyt: Bflyt = Cursor::new(bytes)
        .read_le()
        .expect("pane hierarchy fixture should parse");

    let json = serde_json::to_string_pretty(&bflyt).expect("layout should serialize to JSON");
    assert!(!json.contains("PaneStart"));
    assert!(!json.contains("PaneEnd"));
    assert!(json.contains("\"children\""));

    let restored: Bflyt = serde_json::from_str(&json).expect("nested pane JSON should deserialize");
    assert_eq!(restored.sections.len(), bflyt.sections.len());
    assert_eq!(restored.num_sections as usize, restored.sections.len());
    let marker_counts = |layout: &Bflyt| {
        layout.sections.iter().fold((0, 0), |counts, section| {
            (
                counts.0 + usize::from(section.as_pane_start().is_some()),
                counts.1 + usize::from(section.as_pane_end().is_some()),
            )
        })
    };
    assert_eq!(marker_counts(&restored), marker_counts(&bflyt));
}

#[test]
fn parts_overrides_are_typed_in_json() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_assets/info_training.bflyt");
    let bytes = fs::read(path).expect("parts override fixture should be readable");
    let bflyt: Bflyt = Cursor::new(bytes)
        .read_le()
        .expect("parts override fixture should parse");

    let json = serde_json::to_string(&bflyt).expect("layout should serialize to JSON");
    assert!(!json.contains("referenced_data"));
    assert!(!json.contains("property_offset"));
    assert!(!json.contains("ext_user_data_offset"));
    assert!(!json.contains("pane_basic_info_offset"));
    assert_eq!(json.matches("\"pane_override\"").count(), 44);
    assert_eq!(json.matches("\"pane_basic_info\"").count(), 12);

    let restored: Bflyt = serde_json::from_str(&json).expect("typed parts JSON should deserialize");
    assert_eq!(restored.sections.len(), bflyt.sections.len());
}

#[test]
fn window_metrics_and_flags_are_typed_in_json() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_assets/info_training.bflyt");
    let bytes = fs::read(path).expect("window fixture should be readable");
    let bflyt: Bflyt = Cursor::new(bytes)
        .read_le()
        .expect("window fixture should parse");

    let json = serde_json::to_string(&bflyt).expect("layout should serialize to JSON");
    assert!(!json.contains("\"rectangle\""));
    assert_eq!(json.matches("\"content_expansion_left\"").count(), 5);
    assert_eq!(json.matches("\"frame_thickness_bottom\"").count(), 5);
    assert!(json.contains("\"frame_mode\":\"HorizontalFrameOnly\""));

    let restored: Bflyt =
        serde_json::from_str(&json).expect("typed window JSON should deserialize");
    assert_eq!(restored.sections.len(), bflyt.sections.len());
}

#[test]
fn textbox_metadata_is_typed_in_json() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_assets/info_training.bflyt");
    let bytes = fs::read(path).expect("textbox fixture should be readable");
    let bflyt: Bflyt = Cursor::new(bytes)
        .read_le()
        .expect("textbox fixture should parse");

    let json = serde_json::to_string(&bflyt).expect("layout should serialize to JSON");
    assert!(!json.contains("\"active_shadows\""));
    assert!(json.contains("\"vertical\":\"Center\""));
    assert!(json.contains("\"line_alignment\":\"Synchronous\""));
    assert_eq!(json.matches("\"line_transforms\"").count(), 57);

    let restored: Bflyt =
        serde_json::from_str(&json).expect("typed textbox JSON should deserialize");
    assert_eq!(restored.sections.len(), bflyt.sections.len());
}
