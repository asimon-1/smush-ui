use std::io::Cursor;

use smush_ui::bflyt::sections::{Control, Material, User};
use binrw::{BinReaderExt, BinWriterExt};

#[test]
fn round_trips_control_user_and_material_payloads() {
    let control = Control {
        data: vec![1, 2, 3],
    };
    let user = User { data: vec![4, 5] };
    let material = Material {
        data: vec![6, 7, 8, 9],
    };

    let mut output = Cursor::new(Vec::new());
    output.write_be(&control).expect("control should write");
    output.write_be(&user).expect("user data should write");
    output.write_be(&material).expect("material should write");
    let bytes = output.into_inner();

    assert_eq!(&bytes[..4], b"cnt1");
    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 11);
    assert_eq!(&bytes[11..15], b"usd1");
    assert_eq!(&bytes[21..25], b"mat1");

    let mut input = Cursor::new(bytes.clone());
    let _: Control = input.read_be().expect("control should parse");
    let _: User = input.read_be().expect("user data should parse");
    let _: Material = input.read_be().expect("material should parse");
    assert_eq!(input.position(), bytes.len() as u64);
}
