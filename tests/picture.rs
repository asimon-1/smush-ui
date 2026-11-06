use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::{
    Color, MaterialInfo, PaneFlags, PaneInfo, PaneOrigin, PartScalingFlags, Picture, Vec2, Vec3,
};

fn pane() -> PaneInfo {
    PaneInfo {
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
    }
}
fn color() -> Color {
    Color {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0xff,
    }
}

#[test]
fn picture_writes_material_info_after_pane_info() {
    let picture = Picture::new(
        pane(),
        MaterialInfo::new(
            color(),
            color(),
            color(),
            color(),
            3,
            vec![[
                Vec2 { x: 0.0, y: 0.0 },
                Vec2 { x: 1.0, y: 0.0 },
                Vec2 { x: 0.0, y: 1.0 },
                Vec2 { x: 1.0, y: 1.0 },
            ]],
        ),
    );
    let mut output = Cursor::new(Vec::new());
    output.write_be(&picture).unwrap();
    let bytes = output.into_inner();

    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 0x88);
    assert_eq!(u16::from_be_bytes(bytes[0x64..0x66].try_into().unwrap()), 3);
    assert_eq!(u16::from_be_bytes(bytes[0x66..0x68].try_into().unwrap()), 1);

    let _: Picture = Cursor::new(bytes).read_be().unwrap();
}
