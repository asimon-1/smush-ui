use std::io::Cursor;

use binrw::{BinWrite, Endian};
use ssbu_ui_sandbox::bffnt::wire::{CmapSection, CwdhSection, FinfSection, TglpSectionPrefix};
use ssbu_ui_sandbox::bflan::wire::{
    AnimationCurveHeader, HermiteKeyframe, Pah1Share, Pai1DataPrefix, PaiEntryHeader, PaiTagHeader,
    Pat1DataPrefix, Pat1FrameMetadata, Pat1VersionExtension, StepKeyframe,
};
use ssbu_ui_sandbox::bflim::{BflimFooter, BflimImageInfo};
use ssbu_ui_sandbox::bflyt::wire::{LayoutSectionPrefix, MaterialInfoPrefix, PaneData};
use ssbu_ui_sandbox::binary::FileHeader;
use ssbu_ui_sandbox::{Bflyt, from_yaml_str, to_yaml_string};

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
