use std::io::Cursor;

use bilge::prelude::*;
use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::Section;
use smush_ui::bflyt::sections::{
    Bounding, Color, MaterialInfo, Pane, PaneFlags, PaneInfo, PaneOrigin, PaneUsageFlags,
    PartScalingFlags, Parts, PartsData, PartsMaterialUsageFlags, PartsPaneBasicInfo, PartsProperty,
    Picture, Textbox, TextboxUsageFlags, User, UserData, Vec2, Vec3, Window,
};

fn pane(name: &str) -> PaneInfo {
    PaneInfo {
        flags: PaneFlags::default(),
        origin: PaneOrigin::default(),
        alpha: 0xff,
        part_scaling: PartScalingFlags::default(),
        name: name.to_owned(),
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

fn property_with_override(pane_override: Section) -> PartsProperty {
    PartsProperty {
        name: "target".to_owned(),
        usage_flags: TextboxUsageFlags::default(),
        basic_usage_flags: PaneUsageFlags::default(),
        material_usage_flags: PartsMaterialUsageFlags::default(),
        user_data_type: 0,
        pane_override: Some(Box::new(pane_override)),
        user_data_override: None,
        is_user_data_sentinel: false,
        pane_basic_info: None,
    }
}

#[test]
fn parts_round_trips_every_embedded_pane_kind() {
    let color = || Color {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0xff,
    };
    let overrides = vec![
        Section::Pane(Pane::new(pane("pane"))),
        Section::Bounding(Bounding::new(pane("bounding"))),
        Section::Picture(Picture::new(
            pane("picture"),
            MaterialInfo::new(color(), color(), color(), color(), 0, Vec::new()),
        )),
        Section::Parts(Parts::new(
            pane("nested_parts"),
            Vec2 { x: 1.0, y: 1.0 },
            Vec::new(),
            PartsData {
                part_name: "Nested".to_owned(),
            },
        )),
        Section::Textbox(Textbox::new(pane("textbox"))),
        Section::Window(Window::new(pane("window"))),
    ];

    for pane_override in overrides {
        let expected = std::mem::discriminant(&pane_override);
        let parts = Parts::new(
            pane("parts"),
            Vec2 { x: 1.0, y: 1.0 },
            vec![property_with_override(pane_override)],
            PartsData {
                part_name: "Part".to_owned(),
            },
        );
        let mut output = Cursor::new(Vec::new());
        output.write_le(&parts).unwrap();
        let restored: Parts = Cursor::new(output.into_inner()).read_le().unwrap();
        let actual =
            std::mem::discriminant(restored.properties[0].pane_override.as_deref().unwrap());

        assert_eq!(actual, expected);
    }
}

#[test]
fn parts_round_trips_embedded_user_data() {
    let parts = Parts::new(
        pane("parts"),
        Vec2 { x: 1.0, y: 1.0 },
        vec![PartsProperty {
            name: "target".to_owned(),
            usage_flags: TextboxUsageFlags::default(),
            basic_usage_flags: PaneUsageFlags::default(),
            material_usage_flags: PartsMaterialUsageFlags::default(),
            user_data_type: 0,
            pane_override: None,
            user_data_override: Some(User::new(UserData {
                entries: Vec::new(),
            })),
            is_user_data_sentinel: false,
            pane_basic_info: None,
        }],
        PartsData {
            part_name: "Part".to_owned(),
        },
    );
    let mut output = Cursor::new(Vec::new());
    output.write_le(&parts).unwrap();
    let bytes = output.into_inner();

    assert_eq!(
        u32::from_le_bytes(bytes[0x80..0x84].try_into().unwrap()),
        0x90
    );
    assert_eq!(&bytes[0x90..0x94], b"usd1");

    let restored: Parts = Cursor::new(bytes).read_le().unwrap();
    assert!(restored.properties[0].user_data_override.is_some());
    assert!(!restored.properties[0].is_user_data_sentinel);
}

#[test]
fn parts_round_trips_typed_property_overrides() {
    let usage_flags = TextboxUsageFlags::from(u8::new(0x81));
    let mut basic_usage_flags = PaneUsageFlags::default();
    basic_usage_flags.set_use_position(true);
    basic_usage_flags.set_use_alpha(true);
    let mut material_usage_flags = PartsMaterialUsageFlags::default();
    material_usage_flags.set_use_texture(true);
    let parts = Parts::new(
        pane("parts"),
        Vec2 { x: 1.0, y: 1.0 },
        vec![PartsProperty {
            name: "target".to_owned(),
            usage_flags,
            basic_usage_flags,
            material_usage_flags,
            user_data_type: 4,
            pane_override: Some(Box::new(Section::Bounding(Bounding::new(pane("override"))))),
            user_data_override: None,
            is_user_data_sentinel: true,
            pane_basic_info: Some(PartsPaneBasicInfo::new(
                "user".to_owned(),
                Vec3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
                Vec3 {
                    x: 4.0,
                    y: 5.0,
                    z: 6.0,
                },
                Vec2 { x: 2.0, y: 3.0 },
                Vec2 { x: 4.0, y: 5.0 },
                0x80,
            )),
        }],
        PartsData {
            part_name: "Part".to_owned(),
        },
    );
    let mut output = Cursor::new(Vec::new());
    output.write_be(&parts).unwrap();
    let bytes = output.into_inner();

    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 0x118);
    assert_eq!(u32::from_be_bytes(bytes[0x54..0x58].try_into().unwrap()), 1);
    assert_eq!(
        u32::from_be_bytes(bytes[0x7c..0x80].try_into().unwrap()),
        0x90
    );
    assert_eq!(u32::from_be_bytes(bytes[0x80..0x84].try_into().unwrap()), 1);
    assert_eq!(
        u32::from_be_bytes(bytes[0x84..0x88].try_into().unwrap()),
        0xe4
    );
    assert_eq!(&bytes[0x88..0x90], b"Part\0\0\0\0");

    let restored: Parts = Cursor::new(bytes).read_be().unwrap();
    let property = &restored.properties[0];
    assert!(property.usage_flags.use_text());
    assert_eq!(property.usage_flags.reserved_i().value(), 0x10);
    assert!(property.basic_usage_flags.use_position());
    assert!(property.basic_usage_flags.use_alpha());
    assert!(property.material_usage_flags.use_texture());
    assert!(property.is_user_data_sentinel);
    assert!(matches!(
        property.pane_override.as_deref(),
        Some(Section::Bounding(_))
    ));
    let basic = property.pane_basic_info.as_ref().unwrap();
    assert_eq!(basic.user_name, "user");
    assert_eq!(basic.position.x, 1.0);
    assert_eq!(basic.alpha, 0x80);
}
