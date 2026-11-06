use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::{
    PaneFlags, PaneInfo, PaneOrigin, Parts, PartsData, PartsProperty, Vec2, Vec3,
};

#[test]
fn parts_round_trips_properties_and_trailing_data() {
    let parts = Parts::new(
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
        },
        Vec2 { x: 1.0, y: 1.0 },
        vec![PartsProperty {
            name: String::new(),
            usage_flags: 1,
            basic_usage_flags: 2,
            material_usage_flags: 3,
            system_ext_user_data_override_flags: 4,
            property_offset: 0,
            ext_user_data_offset: 0,
            pane_basic_info_offset: 0,
        }],
        PartsData {
            part_name: "Part".to_owned(),
            referenced_data: vec![0xaa, 0xbb],
        },
    );
    let mut output = Cursor::new(Vec::new());
    output.write_be(&parts).unwrap();
    let bytes = output.into_inner();

    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 0x92);
    assert_eq!(u32::from_be_bytes(bytes[0x54..0x58].try_into().unwrap()), 1);
    assert_eq!(&bytes[0x88..], b"Part\0\0\0\0\xaa\xbb");

    let _: Parts = Cursor::new(bytes).read_be().unwrap();
}
