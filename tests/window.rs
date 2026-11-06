use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::{
    PaneFlags, PaneInfo, PaneOrigin, TextureFlip, Vec2, Vec3, Window, WindowFrame,
};

#[test]
fn window_writes_fixed_fields_and_trailing_data() {
    let mut window = Window::new(PaneInfo {
        flags: PaneFlags::default(),
        origin: PaneOrigin::default(),
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
        size: Vec2 { x: 1.0, y: 1.0 },
    });
    window.rectangle = [0.0, 1.0, 2.0, 3.0];
    window.flags = 3;
    window.data.frames = vec![WindowFrame {
        material_index: 2,
        texture_flip: TextureFlip::Rotate90,
    }];
    let mut output = Cursor::new(Vec::new());
    output.write_be(&window).unwrap();
    let bytes = output.into_inner();

    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 0x8c);
    assert_eq!(bytes[0x64], 1);
    let parsed: Window = Cursor::new(bytes).read_be().unwrap();
    assert_eq!(parsed.data.frames[0].material_index, 2);
}
