use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::{
    Color, LineTransform, PaneFlags, PaneInfo, PaneOrigin, PerCharacterTransform,
    PerCharacterTransformLoopMode, TextAlignment, TextHorizontalOrigin, TextOrigin,
    TextVerticalOrigin, Textbox, Vec2, Vec3,
};

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
fn textbox_writes_metadata_and_transform_payloads() {
    let mut textbox = Textbox::new(pane());
    textbox.text_buffer_length = 16;
    textbox.text_restrict_length = 12;
    textbox.material_index = 1;
    textbox.font_index = 2;
    textbox.origin = TextOrigin::new(TextVerticalOrigin::Top, TextHorizontalOrigin::Left);
    textbox.line_alignment = TextAlignment::Center;
    textbox.flags.set_enable_shadow(true);
    textbox.flags.set_enable_line_transform(true);
    textbox.flags.set_enable_per_character_transform(true);
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
    textbox.data.line_transforms = Some(vec![
        LineTransform {
            offset: 1.5,
            width: 2.5,
        },
        LineTransform {
            offset: 3.5,
            width: 4.5,
        },
    ]);
    textbox.data.per_character_transform = Some(PerCharacterTransform {
        curve_time_offset: 1.0,
        curve_time_width: 2.0,
        loop_mode: PerCharacterTransformLoopMode::Loop,
        rotation_origin: TextVerticalOrigin::Bottom,
        rotation_origin_offset: 3.0,
        fixed_space_width: 4.0,
        fixed_space_origin: TextHorizontalOrigin::Right,
        animation_data: None,
    });
    let mut output = Cursor::new(Vec::new());
    output.write_be(&textbox).unwrap();
    let bytes = output.into_inner();

    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 0xec);
    assert_eq!(
        u16::from_be_bytes(bytes[0x54..0x56].try_into().unwrap()),
        16
    );
    assert_eq!(bytes[0x5c], 0b101);
    assert_eq!(bytes[0x5d], 2);
    assert_eq!(
        u16::from_be_bytes(bytes[0x5e..0x60].try_into().unwrap()),
        0x51
    );
    assert_eq!(
        u32::from_be_bytes(bytes[0xa0..0xa4].try_into().unwrap()),
        0xb8
    );
    assert_eq!(
        u32::from_be_bytes(bytes[0xa4..0xa8].try_into().unwrap()),
        0xcc
    );

    let parsed: Textbox = Cursor::new(bytes).read_be().unwrap();
    assert_eq!(parsed.origin.vertical(), TextVerticalOrigin::Top);
    assert_eq!(parsed.origin.horizontal(), TextHorizontalOrigin::Left);
    assert_eq!(parsed.line_alignment, TextAlignment::Center);
    assert!(parsed.flags.enable_shadow());
    assert!(parsed.flags.enable_line_transform());
    assert!(parsed.flags.enable_per_character_transform());
    assert_eq!(parsed.data.text, "Hi");
    assert_eq!(parsed.data.call_name.as_deref(), Some("Text"));
    let line_transforms = parsed.data.line_transforms.unwrap();
    assert_eq!(line_transforms.len(), 2);
    assert_eq!(line_transforms[1].width, 4.5);
    let transform = parsed.data.per_character_transform.unwrap();
    assert_eq!(transform.loop_mode, PerCharacterTransformLoopMode::Loop);
    assert_eq!(transform.rotation_origin, TextVerticalOrigin::Bottom);
    assert_eq!(transform.fixed_space_origin, TextHorizontalOrigin::Right);
}
