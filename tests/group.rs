use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::Group;

#[test]
fn round_trips_fixed_width_group_names() {
    let group = Group::new("root".to_owned(), [0; 2], vec!["child".to_owned()]);
    let mut output = Cursor::new(Vec::new());
    output.write_be(&group).expect("group should write");
    let bytes = output.into_inner();

    assert_eq!(&bytes[..4], b"grp1");
    assert_eq!(u16::from_be_bytes(bytes[0x20..0x22].try_into().unwrap()), 1);
    assert_eq!(bytes.len(), 0x3c);

    let parsed: Group = Cursor::new(bytes).read_be().expect("group should parse");
    assert_eq!(parsed.name, "root");
    assert_eq!(parsed.children[0], "child");
}
