use std::io::Cursor;

use smush_ui::bflyt::sections::TextureList;
use binrw::{BinReaderExt, BinWriterExt, NullString};

#[rustfmt::skip]
const TEXTURE_LIST: &[u8] = &[
    b't', b'x', b'l', b'1', // Magic
    0x00, 0x00, 0x00, 0x1e, // Section size
    0x00, 0x02, // Filename count
    0x00, 0x00, 0x00, 0x12, // Offset to "foo"
    0x00, 0x00, 0x00, 0x16, // Offset to "bar.png"
    b'f', b'o', b'o', 0x00,
    b'b', b'a', b'r', b'.', b'p', b'n', b'g', 0x00,
];

#[test]
fn reads_section_relative_filename_offsets() {
    let mut input = Cursor::new(TEXTURE_LIST);
    let texture_list: TextureList = input.read_be().expect("fixture should parse");

    assert_eq!(texture_list.filenames[0].as_slice(), b"foo");
    assert_eq!(texture_list.filenames[1].as_slice(), b"bar.png");
    assert_eq!(input.position(), TEXTURE_LIST.len() as u64);
}

#[test]
fn writes_offsets_and_size_from_filenames() {
    let texture_list = TextureList {
        filenames: vec![NullString::from("foo"), NullString::from("bar.png")],
    };
    let mut output = Cursor::new(Vec::new());

    output
        .write_be(&texture_list)
        .expect("texture list should write");

    assert_eq!(output.into_inner(), TEXTURE_LIST);
}

#[test]
fn round_trips_an_empty_list() {
    let texture_list = TextureList {
        filenames: Vec::new(),
    };
    let mut output = Cursor::new(Vec::new());
    output
        .write_be(&texture_list)
        .expect("empty texture list should write");
    let bytes = output.into_inner();

    assert_eq!(bytes, b"txl1\0\0\0\x0a\0\0");

    let parsed: TextureList = Cursor::new(bytes)
        .read_be()
        .expect("empty list should parse");
    assert!(parsed.filenames.is_empty());
}

#[test]
fn round_trips_nonempty() {
    let mut input = Cursor::new(TEXTURE_LIST);
    let texture_list: TextureList = input.read_be().expect("fixture should parse");
    let mut output = Cursor::new(Vec::new());
    output
        .write_be(&texture_list)
        .expect("fixture should write");
    let bytes = output.into_inner();
    assert_eq!(bytes, TEXTURE_LIST);
}
