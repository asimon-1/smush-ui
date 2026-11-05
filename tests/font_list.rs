use std::io::Cursor;

use smush_ui::bflyt::sections::FontList;
use binrw::{BinReaderExt, BinWriterExt, NullString};

#[test]
fn round_trips_font_filenames_and_offsets() {
    let font_list = FontList {
        filenames: vec![
            NullString::from("main.bffnt"),
            NullString::from("icons.bffnt"),
        ],
    };
    let mut output = Cursor::new(Vec::new());
    output.write_be(&font_list).expect("font list should write");
    let bytes = output.into_inner();

    assert_eq!(&bytes[..4], b"fnl1");
    assert_eq!(u16::from_be_bytes(bytes[8..10].try_into().unwrap()), 2);
    assert_eq!(u32::from_be_bytes(bytes[10..14].try_into().unwrap()), 0x12);

    let parsed: FontList = Cursor::new(bytes)
        .read_be()
        .expect("font list should parse");
    assert_eq!(parsed.filenames[0].as_slice(), b"main.bffnt");
    assert_eq!(parsed.filenames[1].as_slice(), b"icons.bffnt");
}
