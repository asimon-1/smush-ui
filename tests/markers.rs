use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::{GroupEnd, GroupStart, PaneEnd, PaneStart};

#[test]
fn marker_sections_are_an_eight_byte_header() {
    let mut output = Cursor::new(Vec::new());
    output.write_be(&PaneStart::new()).unwrap();
    output.write_be(&PaneEnd::new()).unwrap();
    output.write_be(&GroupStart::new()).unwrap();
    output.write_be(&GroupEnd::new()).unwrap();
    let bytes = output.into_inner();

    assert_eq!(bytes.len(), 32);
    for header in bytes.as_chunks::<8>().0 {
        assert_eq!(u32::from_be_bytes(header[4..8].try_into().unwrap()), 8);
    }

    let mut input = Cursor::new(bytes);
    let _: PaneStart = input.read_be().unwrap();
    let _: PaneEnd = input.read_be().unwrap();
    let _: GroupStart = input.read_be().unwrap();
    let _: GroupEnd = input.read_be().unwrap();
}
