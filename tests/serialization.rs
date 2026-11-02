use std::io::Cursor;

use binrw::Endian;
use ssbu_ui_sandbox::binary::FileHeader;
use ssbu_ui_sandbox::{Bflyt, from_yaml_str, to_yaml_string};

#[test]
fn bflyt_yaml_round_trip_preserves_nested_panes() {
    let source = r#"
version: 2
layout:
  width: 1280.0
  height: 720.0
  origin: center
  name: sample
root:
  name: RootPane
  visible: true
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
