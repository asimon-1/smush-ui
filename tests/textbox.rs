use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::{Color, PaneFlags, PaneInfo, PaneOrigin, Textbox, Vec2, Vec3};

fn color() -> Color {
    Color {
        red: 1,
        green: 2,
        blue: 3,
        alpha: 4,
    }
}
fn pane() -> PaneInfo {
    PaneInfo {
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
    }
}

#[test]
fn textbox_writes_fixed_fields_and_trailing_data() {
    let textbox = Textbox {
        pane: pane(),
        text_buffer_length: 16,
        text_restrict_length: 12,
        material_index: 1,
        font_index: 2,
        origin: 3,
        line_alignment: 4,
        active_shadows: 5,
        padding: 0,
        italic_tilt: 0.25,
        text_offset: 0xa0,
        top_color: color(),
        bottom_color: color(),
        font_size: Vec2 { x: 12.0, y: 14.0 },
        character_size: 2.0,
        line_size: 3.0,
        textbox_name_offset: 0,
        shadow_offset: Vec2 { x: 1.0, y: 2.0 },
        shadow_scale: Vec2 { x: 3.0, y: 4.0 },
        shadow_top_color: color(),
        shadow_bottom_color: color(),
        shadow_italic_tilt: 0.5,
        data: vec![0, 1, 2],
    };
    let mut output = Cursor::new(Vec::new());
    output.write_be(&textbox).unwrap();
    let bytes = output.into_inner();

    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 0xa3);
    assert_eq!(
        u16::from_be_bytes(bytes[0x54..0x56].try_into().unwrap()),
        16
    );
    assert_eq!(&bytes[0xa0..], &[0, 1, 2]);
    let _: Textbox = Cursor::new(bytes).read_be().unwrap();
}
