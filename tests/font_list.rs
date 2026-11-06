use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::FontList;

#[test]
fn round_trips_font_filenames_and_offsets() {
    let font_list = FontList::new(vec!["main.bffnt".to_owned(), "icons.bffnt".to_owned()]);
    let mut output = Cursor::new(Vec::new());
    output.write_le(&font_list).expect("font list should write");
    let bytes = output.into_inner();

    assert_eq!(&bytes[..4], b"fnl1");
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 8);

    let parsed: FontList = Cursor::new(bytes)
        .read_le()
        .expect("font list should parse");
    assert_eq!(parsed.filenames[0], "main.bffnt");
    assert_eq!(parsed.filenames[1], "icons.bffnt");
}
