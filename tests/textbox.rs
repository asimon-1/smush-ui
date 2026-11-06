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
    }
}

#[test]
fn textbox_writes_fixed_fields_and_trailing_data() {
    let mut textbox = Textbox::new(pane());
    textbox.text_buffer_length = 16;
    textbox.text_restrict_length = 12;
    textbox.material_index = 1;
    textbox.font_index = 2;
    textbox.origin = 3;
    textbox.line_alignment = 4;
    textbox.active_shadows = 5;
    textbox.italic_tilt = 0.25;
    textbox.top_color = color();
    textbox.bottom_color = color();
    textbox.font_size = Vec2 { x: 12.0, y: 14.0 };
    textbox.character_size = 2.0;
    textbox.line_size = 3.0;
    textbox.shadow_offset = Vec2 { x: 1.0, y: 2.0 };
    textbox.shadow_scale = Vec2 { x: 3.0, y: 4.0 };
    textbox.shadow_top_color = color();
    textbox.shadow_bottom_color = color();
    textbox.shadow_italic_tilt = 0.5;
    textbox.data.text = "Hi".to_owned();
    textbox.data.call_name = Some("Text".to_owned());
    let mut output = Cursor::new(Vec::new());
    output.write_be(&textbox).unwrap();
    let bytes = output.into_inner();

    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 0xb0);
    assert_eq!(
        u16::from_be_bytes(bytes[0x54..0x56].try_into().unwrap()),
        16
    );
    let parsed: Textbox = Cursor::new(bytes).read_be().unwrap();
    assert_eq!(parsed.data.text, "Hi");
    assert_eq!(parsed.data.call_name.as_deref(), Some("Text"));
}
