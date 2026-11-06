use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::{
    PaneFlags, PaneInfo, PaneOrigin, PartScalingFlags, TextureFlip, Vec2, Vec3, Window,
    WindowFrame, WindowFrameMode,
};

#[test]
fn window_writes_fixed_fields_and_trailing_data() {
    let mut window = Window::new(PaneInfo {
        flags: PaneFlags::default(),
        origin: PaneOrigin::default(),
        alpha: 0xff,
        part_scaling: PartScalingFlags::default(),
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
    window.content_expansion_left = -1;
    window.content_expansion_right = 2;
    window.content_expansion_top = -3;
    window.content_expansion_bottom = 4;
    window.frame_thickness_left = 5;
    window.frame_thickness_right = 6;
    window.frame_thickness_top = 7;
    window.frame_thickness_bottom = 8;
    window.flags.set_use_top_left_frame_material_for_all(true);
    window.flags.set_tint_frames_with_content_colors(true);
    window
        .flags
        .set_frame_mode(WindowFrameMode::HorizontalFrameOnly);
    window.flags.set_hide_content(true);
    window.data.frames = vec![WindowFrame {
        material_index: 2,
        texture_flip: TextureFlip::Rotate90,
    }];
    let mut output = Cursor::new(Vec::new());
    output.write_be(&window).unwrap();
    let bytes = output.into_inner();

    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 0x8c);
    assert_eq!(
        i16::from_be_bytes(bytes[0x54..0x56].try_into().unwrap()),
        -1
    );
    assert_eq!(i16::from_be_bytes(bytes[0x62..0x64].try_into().unwrap()), 8);
    assert_eq!(bytes[0x64], 1);
    assert_eq!(bytes[0x65], 0x1b);
    let parsed: Window = Cursor::new(bytes).read_be().unwrap();
    assert_eq!(parsed.content_expansion_left, -1);
    assert_eq!(parsed.frame_thickness_bottom, 8);
    assert_eq!(
        parsed.flags.frame_mode(),
        WindowFrameMode::HorizontalFrameOnly
    );
    assert!(parsed.flags.hide_content());
    assert_eq!(parsed.data.frames[0].material_index, 2);
}
