use std::{fs, io::Cursor, path::Path};

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::{
    Bflyt, Section,
    sections::{
        Bounding, Group, GroupEnd, GroupStart, LineTransform, Pane, PaneEnd, PaneFlags, PaneInfo,
        PaneOrigin, PaneStart, PaneUsageFlags, PartScalingFlags, Parts, PartsData,
        PartsMaterialUsageFlags, PartsPaneBasicInfo, PartsProperty, TextAlignment,
        TextHorizontalOrigin, TextOrigin, TextVerticalOrigin, Textbox, TextboxUsageFlags, Vec2,
        Vec3, Window, WindowFrameMode,
    },
};

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

fn pane(name: &str) -> PaneInfo {
    PaneInfo {
        flags: PaneFlags::default(),
        origin: PaneOrigin::default(),
        alpha: 0xff,
        part_scaling: PartScalingFlags::default(),
        name: name.to_owned(),
        user_name: String::new(),
        position: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        scale: Vec2 { x: 1.0, y: 1.0 },
        size: Vec2 { x: 100.0, y: 50.0 },
    }
}

fn synthetic_bflyt(sections: Vec<Section>) -> Bflyt {
    let mut fixture: Bflyt = Cursor::new(BFLYT)
        .read_le()
        .expect("base fixture should parse");
    fixture.sections.extend(sections);

    let mut output = Cursor::new(Vec::new());
    output
        .write_le(&fixture)
        .expect("synthetic BFLYT should write");
    Cursor::new(output.into_inner())
        .read_le()
        .expect("synthetic BFLYT should parse")
}

#[test]
fn reads_a_bflyt_layout() {
    let mut input = Cursor::new(BFLYT);

    let bflyt: Bflyt = input.read_le().expect("fixture should parse");

    assert!(
        bflyt.sections[0]
            .as_layout()
            .expect("section should be a layout")
            .is_drawn_from_middle
    );

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
    assert!(!json.contains("\"filesize\""));
    assert!(!json.contains("\"num_sections\""));
    assert!(json.contains("\"is_drawn_from_middle\": true"));
    let restored: Bflyt = serde_json::from_str(&json).expect("JSON should deserialize to a layout");
    let mut output = Cursor::new(Vec::new());

    output
        .write_le(&restored)
        .expect("JSON layout should write");
    assert_eq!(output.into_inner(), BFLYT);
}

#[test]
fn derives_filesize_after_json_changes_section_size() {
    let bflyt: Bflyt = Cursor::new(BFLYT).read_le().expect("fixture should parse");
    let mut json = serde_json::to_value(&bflyt).expect("layout should serialize to JSON");
    json["sections"][0]["Layout"]["name"] = "A longer layout name".into();
    let restored: Bflyt = serde_json::from_value(json).expect("edited JSON should deserialize");
    assert_eq!(restored.filesize, 0);

    let mut output = Cursor::new(Vec::new());
    output
        .write_le(&restored)
        .expect("edited JSON layout should write");
    let bytes = output.into_inner();
    assert_eq!(
        u32::from_le_bytes(bytes[0x0c..0x10].try_into().unwrap()) as usize,
        bytes.len()
    );
    let reparsed: Bflyt = Cursor::new(bytes.clone())
        .read_le()
        .expect("written layout should parse");
    assert_eq!(reparsed.filesize as usize, bytes.len());
}

#[test]
fn every_real_bflyt_fixture_round_trips_through_json() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_assets/bflyt");
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
    let bflyt = synthetic_bflyt(vec![
        Section::Pane(Pane::new(pane("parent"))),
        Section::PaneStart(PaneStart::new()),
        Section::Bounding(Bounding::new(pane("child"))),
        Section::PaneEnd(PaneEnd::new()),
    ]);

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
fn group_boundaries_are_a_json_children_hierarchy() {
    let bflyt = synthetic_bflyt(vec![
        Section::Group(Group::new("RootGroup".to_owned(), Vec::new())),
        Section::GroupStart(GroupStart::new()),
        Section::Group(Group::new("ChildGroup".to_owned(), Vec::new())),
        Section::GroupEnd(GroupEnd::new()),
    ]);

    let json = serde_json::to_string_pretty(&bflyt).expect("layout should serialize to JSON");
    assert!(!json.contains("GroupStart"));
    assert!(!json.contains("GroupEnd"));
    assert!(json.contains("\"name\": \"RootGroup\""));
    assert!(!json.contains("\"members\""));
    assert!(json.contains("\"children\""));

    let restored: Bflyt =
        serde_json::from_str(&json).expect("nested group JSON should deserialize");
    let marker_counts = |layout: &Bflyt| {
        layout.sections.iter().fold((0, 0), |counts, section| {
            (
                counts.0 + usize::from(section.as_group_start().is_some()),
                counts.1 + usize::from(section.as_group_end().is_some()),
            )
        })
    };
    assert_eq!(marker_counts(&restored), marker_counts(&bflyt));
}

#[test]
fn parts_overrides_are_typed_in_json() {
    let property = PartsProperty {
        name: "target".to_owned(),
        usage_flags: TextboxUsageFlags::default(),
        basic_usage_flags: PaneUsageFlags::default(),
        material_usage_flags: PartsMaterialUsageFlags::default(),
        user_data_type: 0,
        pane_override: Some(Box::new(Section::Bounding(Bounding::new(pane("override"))))),
        user_data_override: None,
        is_user_data_sentinel: false,
        pane_basic_info: Some(PartsPaneBasicInfo::new(
            "user".to_owned(),
            Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            Vec3 {
                x: 4.0,
                y: 5.0,
                z: 6.0,
            },
            Vec2 { x: 2.0, y: 3.0 },
            Vec2 { x: 4.0, y: 5.0 },
            0x80,
        )),
    };
    let bflyt = synthetic_bflyt(vec![Section::Parts(Parts::new(
        pane("parts"),
        Vec2 { x: 1.0, y: 1.0 },
        vec![property],
        PartsData {
            part_name: "Part".to_owned(),
        },
    ))]);

    let json = serde_json::to_string(&bflyt).expect("layout should serialize to JSON");
    assert!(!json.contains("referenced_data"));
    assert!(!json.contains("property_offset"));
    assert!(!json.contains("ext_user_data_offset"));
    assert!(!json.contains("pane_basic_info_offset"));
    assert_eq!(json.matches("\"pane_override\"").count(), 1);
    assert_eq!(json.matches("\"pane_basic_info\"").count(), 1);

    let restored: Bflyt = serde_json::from_str(&json).expect("typed parts JSON should deserialize");
    assert_eq!(restored.sections.len(), bflyt.sections.len());
    let property = &restored.sections[1].as_parts().unwrap().properties[0];
    assert!(matches!(
        property.pane_override.as_deref(),
        Some(Section::Bounding(_))
    ));
    assert_eq!(property.pane_basic_info.as_ref().unwrap().alpha, 0x80);
}

#[test]
fn window_metrics_and_flags_are_typed_in_json() {
    let mut window = Window::new(pane("window"));
    window.content_expansion_left = -1;
    window.frame_thickness_bottom = 8;
    window
        .flags
        .set_frame_mode(WindowFrameMode::HorizontalFrameOnly);
    let bflyt = synthetic_bflyt(vec![Section::Window(window)]);

    let json = serde_json::to_string(&bflyt).expect("layout should serialize to JSON");
    assert!(!json.contains("\"rectangle\""));
    assert_eq!(json.matches("\"content_expansion_left\"").count(), 1);
    assert_eq!(json.matches("\"frame_thickness_bottom\"").count(), 1);
    assert!(json.contains("\"frame_mode\":\"HorizontalFrameOnly\""));

    let restored: Bflyt =
        serde_json::from_str(&json).expect("typed window JSON should deserialize");
    assert_eq!(restored.sections.len(), bflyt.sections.len());
    let window = restored.sections[1].as_window().unwrap();
    assert_eq!(window.content_expansion_left, -1);
    assert_eq!(window.frame_thickness_bottom, 8);
}

#[test]
fn textbox_metadata_is_typed_in_json() {
    let mut textbox = Textbox::new(pane("textbox"));
    textbox.origin = TextOrigin::new(TextVerticalOrigin::Center, TextHorizontalOrigin::Left);
    textbox.line_alignment = TextAlignment::Synchronous;
    textbox.flags.set_enable_line_transform(true);
    textbox.data.text = "text".to_owned();
    textbox.data.line_transforms = Some(vec![
        LineTransform {
            offset: 1.0,
            width: 2.0,
        },
        LineTransform {
            offset: 3.0,
            width: 4.0,
        },
    ]);
    let bflyt = synthetic_bflyt(vec![Section::Textbox(textbox)]);

    let json = serde_json::to_string(&bflyt).expect("layout should serialize to JSON");
    assert!(!json.contains("\"active_shadows\""));
    assert!(json.contains("\"vertical\":\"Center\""));
    assert!(json.contains("\"line_alignment\":\"Synchronous\""));
    assert_eq!(json.matches("\"line_transforms\"").count(), 1);

    let restored: Bflyt =
        serde_json::from_str(&json).expect("typed textbox JSON should deserialize");
    assert_eq!(restored.sections.len(), bflyt.sections.len());
    let textbox = restored.sections[1].as_textbox().unwrap();
    assert_eq!(textbox.origin.vertical(), TextVerticalOrigin::Center);
    assert_eq!(textbox.line_alignment, TextAlignment::Synchronous);
    assert_eq!(textbox.data.line_transforms.as_ref().unwrap().len(), 2);
}

#[test]
fn smaller_scalar_fields_are_typed_in_json() {
    let bflyt = synthetic_bflyt(vec![Section::Parts(Parts::new(
        pane("parts"),
        Vec2 { x: 1.0, y: 1.0 },
        Vec::new(),
        PartsData {
            part_name: "Part".to_owned(),
        },
    ))]);

    let json = serde_json::to_string(&bflyt).expect("layout should serialize to JSON");
    assert!(!json.contains("\"num_sections\""));
    assert!(json.contains("\"is_drawn_from_middle\":true"));
    assert!(json.contains("\"part_scaling\":{\"unk1\":false"));

    let restored: Bflyt =
        serde_json::from_str(&json).expect("typed scalar JSON should deserialize");
    assert_eq!(restored.sections.len(), bflyt.sections.len());
}
