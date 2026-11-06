use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::{
    Bounding, HorizontalOrigin, Pane, PaneFlags, PaneInfo, PaneOrigin, Vec2, Vec3, VerticalOrigin,
};

fn pane_info() -> PaneInfo {
    let mut flags = PaneFlags::default();
    flags.set_is_visible(true);
    flags.set_transmits_alpha_to_children(true);
    PaneInfo {
        flags,
        origin: PaneOrigin::new(
            HorizontalOrigin::Left,
            VerticalOrigin::Bottom,
            HorizontalOrigin::Right,
            VerticalOrigin::Top,
        ),
        alpha: 0xff,
        part_scaling: 0,
        name: String::new(),
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
        size: Vec2 { x: 320.0, y: 240.0 },
    }
}

#[test]
fn pane_and_bounding_have_the_documented_size() {
    let mut output = Cursor::new(Vec::new());
    output.write_be(&Pane::new(pane_info())).unwrap();
    output.write_be(&Bounding::new(pane_info())).unwrap();
    let bytes = output.into_inner();

    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 0x54);
    assert_eq!(
        u32::from_be_bytes(bytes[0x58..0x5c].try_into().unwrap()),
        0x54
    );
    assert_eq!(bytes[8], 0x03);
    assert_eq!(bytes[9], 0x69);

    let mut input = Cursor::new(bytes);
    let _: Pane = input.read_be().unwrap();
    let _: Bounding = input.read_be().unwrap();
    assert_eq!(input.position(), 0xa8);
}
