use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::sections::{
    Control, ControlData, Material, User, UserData, UserDataEntry, UserDataValue,
};

#[test]
fn round_trips_control_user_and_material_payloads() {
    let control = Control::new(ControlData {
        control_name: "Control".to_owned(),
        reserve0_name: "Reserve".to_owned(),
        pane_bindings: vec!["Pane".to_owned()],
        core_anims: vec!["CoreAnim".to_owned()],
        pane_names: vec!["PaneName".to_owned()],
        anim_names: vec!["AnimName".to_owned()],
    });
    let user = User::new(UserData {
        entries: vec![UserDataEntry {
            name: "Value".to_owned(),
            value: UserDataValue::Integers(vec![4, 5]),
            reserved: 0,
        }],
    });
    let material = Material::new(Vec::new());

    let mut output = Cursor::new(Vec::new());
    output.write_be(&control).expect("control should write");
    output.write_be(&user).expect("user data should write");
    output.write_be(&material).expect("material should write");
    let bytes = output.into_inner();

    assert_eq!(&bytes[..4], b"cnt1");
    let control_size = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    assert_eq!(&bytes[control_size..control_size + 4], b"usd1");

    let mut input = Cursor::new(bytes.clone());
    let parsed_control: Control = input.read_be().expect("control should parse");
    assert_eq!(parsed_control.data.control_name, "Control");
    assert_eq!(parsed_control.data.pane_bindings, ["Pane"]);
    let parsed_user: User = input.read_be().expect("user data should parse");
    assert_eq!(parsed_user.data.entries[0].name, "Value");
    let parsed_material: Material = input.read_be().expect("material should parse");
    assert!(parsed_material.materials.is_empty());
    assert_eq!(input.position(), bytes.len() as u64);
}
