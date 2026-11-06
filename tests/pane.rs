use std::io::Cursor;

use smush_ui::bflyt::sections::{
    Bounding, HorizontalOrigin, Pane, PaneFlags, PaneInfo, PaneOrigin, Vec2, Vec3, VerticalOrigin,
};
use binrw::{BinReaderExt, BinWriterExt};

fn pane_info() -> PaneInfo {
    PaneInfo {
        flags: PaneFlags {
            is_visible: true,
            transmits_alpha_to_children: true,
            reserved: 0,
        },
        origin: PaneOrigin {
            x: HorizontalOrigin::Left,
            y: VerticalOrigin::Bottom,
            parent_x: HorizontalOrigin::Right,
            parent_y: VerticalOrigin::Top,
        },
        alpha: 0xff,
        part_scaling: 0,
        name: [0; 24],
        user_data: [0; 8],
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
