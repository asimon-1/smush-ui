use std::io::Cursor;

use binrw::{BinWrite, Endian};
use ssbu_ui_sandbox::bffnt::wire::{CmapSection, CwdhSection, FinfSection, TglpSectionPrefix};
use ssbu_ui_sandbox::bflan::wire::{
    AnimationCurveHeader, HermiteKeyframe, Pah1Share, Pai1DataPrefix, PaiEntryHeader, PaiTagHeader,
    Pat1DataPrefix, Pat1FrameMetadata, Pat1VersionExtension, StepKeyframe,
};
use ssbu_ui_sandbox::bflan::{
    AnimationCurve, AnimationEntryTarget, AnimationShare, CurveType, Keyframe, Pah1, Pai1,
    PaiEntry, PaiTag, Pat1,
};
use ssbu_ui_sandbox::bflim::{BflimFooter, BflimImageInfo};
use ssbu_ui_sandbox::bflyt::wire::{LayoutSectionPrefix, MaterialInfoPrefix, PaneData};
use ssbu_ui_sandbox::binary::{ByteOrder, FileHeader};
use ssbu_ui_sandbox::{Bflan, Bflyt, SarcArchive, from_yaml_str, to_yaml_string};

#[test]
fn bflyt_yaml_round_trip_preserves_nested_panes() {
    let source = r#"
version: 2
layout:
  width: 1280.0
  height: 720.0
  is_drawn_from_middle: true
  name: sample
root:
  name: RootPane
  visible: true
  origin:
    x: left
    y: top
  kind:
    type: container
  children:
    - name: logo
      kind:
        type: picture
        material: logo_material
      position:
        x: 12.0
        y: 24.0
        z: 0.0
"#;

    let layout: Bflyt = from_yaml_str(source).expect("valid layout YAML");
    let serialized = to_yaml_string(&layout).expect("layout serializes to YAML");
    let decoded: Bflyt = from_yaml_str(&serialized).expect("serialized YAML parses");

    assert_eq!(decoded, layout);
    assert_eq!(decoded.root.children[0].name, "logo");
}

#[test]
fn binary_file_header_round_trips_in_both_byte_orders() {
    let expected = FileHeader {
        magic: *b"FLYT",
        byte_order_mark: 0xFEFF,
        header_size: 0x14,
        version: 0x0202_0000,
        file_size: 0x1234,
        section_count: 7,
        padding: 0,
    };

    for endian in [Endian::Big, Endian::Little] {
        let mut cursor = Cursor::new(Vec::new());
        expected
            .write_with_endian(&mut cursor, endian)
            .expect("header writes");
        assert_eq!(cursor.get_ref().len(), 0x14);

        cursor.set_position(0);
        let decoded = FileHeader::read_with_endian(&mut cursor, endian).expect("header reads");

        assert_eq!(decoded, expected);
        assert!(decoded.has_magic(b"FLYT"));
    }
}

#[test]
fn fixed_wire_records_match_documented_sizes() {
    macro_rules! assert_wire_size {
        ($record:expr, $expected_size:expr) => {{
            let mut cursor = Cursor::new(Vec::new());
            $record
                .write_options(&mut cursor, Endian::Big, ())
                .expect("wire record writes");
            assert_eq!(cursor.get_ref().len(), $expected_size);
        }};
    }

    assert_wire_size!(LayoutSectionPrefix::default(), 20);
    assert_wire_size!(PaneData::default(), 76);
    assert_wire_size!(MaterialInfoPrefix::default(), 20);

    assert_wire_size!(BflimFooter::default(), 0x28);
    assert_wire_size!(BflimImageInfo::default(), 0x10);

    assert_wire_size!(FinfSection::default(), 0x20);
    assert_wire_size!(TglpSectionPrefix::default(), 0x20);
    assert_wire_size!(CwdhSection::default(), 0x10);
    assert_wire_size!(CmapSection::default(), 0x14);

    assert_wire_size!(Pat1DataPrefix::default(), 12);
    assert_wire_size!(Pat1VersionExtension::default(), 4);
    assert_wire_size!(Pat1FrameMetadata::default(), 5);
    assert_wire_size!(Pah1Share::default(), 52);
    assert_wire_size!(Pai1DataPrefix::default(), 12);
    assert_wire_size!(PaiEntryHeader::default(), 32);
    assert_wire_size!(PaiTagHeader::default(), 8);
    assert_wire_size!(AnimationCurveHeader::default(), 12);
    assert_wire_size!(StepKeyframe::default(), 8);
    assert_wire_size!(HermiteKeyframe::default(), 12);
}

#[test]
fn sarc_round_trip_preserves_files_and_byte_order() {
    let mut archive = SarcArchive::new();
    archive
        .files
        .insert("blyt/sample.bflyt".to_owned(), b"layout data".to_vec());
    archive
        .files
        .insert("anim/sample.bflan".to_owned(), b"animation data".to_vec());
    archive
        .files
        .insert("timg/__Combined.bntx".to_owned(), vec![0xAB; 257]);

    for byte_order in [ByteOrder::LittleEndian, ByteOrder::BigEndian] {
        archive.byte_order = byte_order;
        let encoded = archive.to_bytes().expect("archive writes");
        let decoded = SarcArchive::from_bytes(&encoded).expect("archive reads");

        assert_eq!(decoded, archive);
    }
}

#[test]
fn sarc_rejects_non_sarc_input() {
    assert!(SarcArchive::from_bytes(b"not an archive").is_err());
}

#[test]
fn sarc_yaml_round_trip_preserves_archive_payloads() {
    let mut archive = SarcArchive::new();
    archive.byte_order = ByteOrder::BigEndian;
    archive.hash_multiplier = 0x65;
    archive.files.insert(
        "blyt/sample.bflyt".to_owned(),
        vec![0x00, 0x01, 0x7F, 0x80, 0xFF],
    );
    archive
        .files
        .insert("anim/empty.bflan".to_owned(), Vec::new());

    let yaml = archive
        .to_yaml_string()
        .expect("archive serializes to YAML");
    let decoded = SarcArchive::from_yaml_str(&yaml).expect("YAML parses to archive");

    assert_eq!(
        SarcArchive::from_bytes(&decoded.to_bytes().expect("archive writes")).unwrap(),
        archive
    );
}

#[test]
fn sarc_yaml_rejects_unsupported_versions_and_invalid_hex() {
    let unsupported = "version: 3\nbyte_order: little_endian\nhash_multiplier: 101\nfiles: {}\n";
    assert!(matches!(
        SarcArchive::from_yaml_str(unsupported),
        Err(ssbu_ui_sandbox::SarcYamlError::UnsupportedVersion(3))
    ));

    let invalid_hex =
        "version: 1\nbyte_order: little_endian\nhash_multiplier: 101\nfiles:\n  x.bin: \"0z\"\n";
    assert!(matches!(
        SarcArchive::from_yaml_str(invalid_hex),
        Err(ssbu_ui_sandbox::SarcYamlError::InvalidHex { .. })
    ));
}

#[test]
fn bflan_yaml_round_trip_preserves_animation_data() {
    let bflan = Bflan {
        header: FileHeader {
            magic: *b"FLAN",
            byte_order_mark: 0xFEFF,
            header_size: 0x14,
            version: 0x0900_0000,
            section_count: 3,
            ..FileHeader::default()
        },
        byte_order: ByteOrder::LittleEndian,
        tag: Some(Pat1 {
            order: 2,
            name: "sample_animation".to_owned(),
            start_frame: 1,
            end_frame: 12,
            child_binding: true,
            flags: 1,
            version_extension: Some(0x1234),
            pre_name_data: Vec::new(),
            groups: vec!["sample_group".to_owned()],
            group_flags: Vec::new(),
            group_padding: Vec::new(),
            unknown_data: Vec::new(),
        }),
        share: Some(Pah1 {
            padding: [0, 0],
            shares: vec![AnimationShare {
                source_pane: "source_pane".to_owned(),
                target_group: "target_group".to_owned(),
                padding: [0, 0],
            }],
        }),
        info: Some(Pai1 {
            frame_size: 12,
            looping: true,
            padding: 0,
            textures: vec!["texture_name".to_owned()],
            entries: vec![PaiEntry {
                name: "target_pane".to_owned(),
                target: AnimationEntryTarget::Unknown(2),
                padding: 0,
                tags: vec![PaiTag {
                    tag: "FLEU".to_owned(),
                    unknown_prefix: Some(44),
                    padding: [0, 0, 0],
                    curves: vec![
                        AnimationCurve {
                            id: 1,
                            target: 2,
                            curve_type: CurveType::Step,
                            key_data_offset: 12,
                            keyframes: vec![Keyframe::Step {
                                frame: 3.0,
                                value: 7,
                                padding: 0,
                            }],
                            ..AnimationCurve::default()
                        },
                        AnimationCurve {
                            id: 2,
                            target: 4,
                            curve_type: CurveType::Hermite,
                            key_data_offset: 12,
                            keyframes: vec![Keyframe::Hermite {
                                frame: 6.0,
                                value: 0.75,
                                slope: -0.5,
                            }],
                            ..AnimationCurve::default()
                        },
                    ],
                }],
            }],
        }),
        ..Bflan::default()
    };

    let mut bflan = bflan;
    let binary = bflan.to_bytes().expect("BFLAN writes");
    bflan.header.file_size = binary.len() as u32;
    let parsed = Bflan::from_bytes(&binary).expect("BFLAN reads");
    assert_eq!(parsed, bflan);

    let mut invalid_offset = binary.clone();
    invalid_offset[32..36].copy_from_slice(&1u32.to_le_bytes());
    assert!(Bflan::from_bytes(&invalid_offset).is_err());

    let mut archive = SarcArchive::new();
    archive.files.insert("anim/sample.bflan".to_owned(), binary);
    archive
        .files
        .insert("timg/sample.bntx".to_owned(), vec![0, 1, 2, 255]);
    let yaml = archive.to_yaml_string().expect("archive writes typed YAML");
    assert!(yaml.contains("format: bflan"));
    assert!(yaml.contains("format: raw_hex"));
    let restored = SarcArchive::from_yaml_str(&yaml).expect("archive reads YAML");
    assert_eq!(restored, archive);
}

#[test]
fn bflyt_binary_and_archive_yaml_round_trip_preserve_nested_panes() {
    fn section(magic: &[u8; 4], data: Vec<u8>) -> Vec<u8> {
        let mut section = magic.to_vec();
        section.extend_from_slice(&((data.len() + 8) as u32).to_le_bytes());
        section.extend(data);
        section
    }

    fn pane(magic: &[u8; 4], name: &str, picture: bool) -> Vec<u8> {
        let mut data = vec![1, 0, 255, 0];
        let mut name_bytes = [0; 24];
        name_bytes[..name.len()].copy_from_slice(name.as_bytes());
        data.extend_from_slice(&name_bytes);
        data.extend_from_slice(&[0; 8]);
        for value in [0.0_f32, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 320.0, 180.0] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        if picture {
            data.extend_from_slice(&[255; 16]);
            data.extend_from_slice(&u16::MAX.to_le_bytes());
            data.extend_from_slice(&[1, 0]);
            for value in [0.0_f32; 8] {
                data.extend_from_slice(&value.to_le_bytes());
            }
        }
        section(magic, data)
    }

    let mut layout_data = vec![1, 0, 0, 0];
    for value in [1280.0_f32, 720.0, 720.0, 1280.0] {
        layout_data.extend_from_slice(&value.to_le_bytes());
    }
    layout_data.extend_from_slice(b"sample\0");
    while layout_data.len() % 4 != 0 {
        layout_data.push(0);
    }

    let mut bytes = b"FLYT\xFF\xFE".to_vec();
    bytes.extend_from_slice(&0x14_u16.to_le_bytes());
    bytes.extend_from_slice(&0x0900_0000_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&5_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend(section(b"lyt1", layout_data));
    bytes.extend(pane(b"pan1", "root", false));
    bytes.extend(section(b"pas1", Vec::new()));
    bytes.extend(pane(b"pic1", "icon", true));
    bytes.extend(section(b"pae1", Vec::new()));
    let file_size = bytes.len() as u32;
    bytes[12..16].copy_from_slice(&file_size.to_le_bytes());

    let parsed = Bflyt::from_bytes(&bytes).expect("BFLYT parses");
    assert_eq!(parsed.layout.name, "sample");
    assert_eq!(parsed.root.name, "root");
    assert_eq!(parsed.root.children[0].name, "icon");
    assert!(matches!(
        parsed.root.children[0].kind,
        ssbu_ui_sandbox::bflyt::PaneKind::Picture { .. }
    ));

    let mut archive = SarcArchive::new();
    archive.files.insert("blyt/sample.bflyt".to_owned(), bytes);
    let yaml = archive
        .to_yaml_string()
        .expect("archive writes typed BFLYT YAML");
    assert!(yaml.contains("format: bflyt"));
    let restored = SarcArchive::from_yaml_str(&yaml).expect("archive reads BFLYT YAML");
    assert_eq!(restored, archive);
}
