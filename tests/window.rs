use std::io::Cursor;

use smush_ui::bflyt::sections::{PaneFlags, PaneInfo, PaneOrigin, Vec2, Vec3, Window};
use binrw::{BinReaderExt, BinWriterExt};

#[test]
fn window_writes_fixed_fields_and_trailing_data() {
    let window = Window {
        pane: PaneInfo {
            flags: PaneFlags::default(),
            origin: PaneOrigin::default(),
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
            size: Vec2 { x: 1.0, y: 1.0 },
        },
        rectangle: [0.0, 1.0, 2.0, 3.0],
        frame_data_count: 2,
        flags: 3,
        padding: [0; 2],
        material_info_offset: 0x70,
        unknown_offset: 0,
        data: vec![0xde, 0xad],
    };
    let mut output = Cursor::new(Vec::new());
    output.write_be(&window).unwrap();
    let bytes = output.into_inner();

    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 0x72);
    assert_eq!(bytes[0x64], 2);
    assert_eq!(&bytes[0x70..], &[0xde, 0xad]);
    let _: Window = Cursor::new(bytes).read_be().unwrap();
}
