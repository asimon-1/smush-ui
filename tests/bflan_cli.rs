#[path = "common/fs.rs"]
mod test_fs;

use std::{fs, process::Command};

use smush_ui::{
    bflan::{Bflan, Pai1, PaiFlags, Pat1, Section},
    sarc::{SarcArchive, SarcEntry},
};
use test_fs::TestDirectory;

fn sample() -> Bflan {
    Bflan::new(vec![
        Section::Pat1(Pat1 {
            order: 1,
            user_data: None,
            start_frame: 0,
            end_frame: 10,
            child_binding: false,
            name: "Open".into(),
            groups: vec!["main".into()],
        }),
        Section::Pai1(Pai1 {
            frame_count: 10,
            flags: PaiFlags::default(),
            textures: vec![],
            entries: vec![],
        }),
    ])
}

#[test]
fn cli_converts_and_edits_bflan_json() {
    let directory = TestDirectory::new("bflan-cli");
    let input = directory.path().join("input.bflan");
    let json = directory.path().join("animation.json");
    let output = directory.path().join("output.bflan");
    fs::write(&input, sample().to_bytes().unwrap()).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .args(["bflan", "to-json"])
        .arg(&input)
        .arg(&json)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let mut value: serde_jsonc2::Value =
        serde_jsonc2::from_slice(&fs::read(&json).unwrap()).unwrap();
    assert_eq!(value["sections"][0]["Pat1"]["name"], "Open");
    value["sections"][0]["Pat1"]["name"] = "Close".into();
    fs::write(&json, serde_jsonc2::to_vec_pretty(&value).unwrap()).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .args(["bflan", "from-json"])
        .arg(&json)
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let parsed = Bflan::from_bytes(&fs::read(output).unwrap()).unwrap();
    let Section::Pat1(tag) = &parsed.sections[0] else {
        panic!("missing PAT1")
    };
    assert_eq!(tag.name, "Close");
}

#[test]
fn archive_text_diff_names_the_bflan_change() {
    let directory = TestDirectory::new("bflan-diff-cli");
    let before_path = directory.path().join("before.arc");
    let after_path = directory.path().join("after.arc");
    let before = sample();
    let mut after = sample();
    let Section::Pat1(tag) = &mut after.sections[0] else {
        panic!("missing PAT1")
    };
    tag.name = "Close".into();
    let archive = |animation: Bflan| {
        SarcArchive::new(vec![SarcEntry::new(
            "anim/open.bflan",
            animation.to_bytes().unwrap(),
        )])
        .to_bytes()
        .unwrap()
    };
    fs::write(&before_path, archive(before)).unwrap();
    fs::write(&after_path, archive(after)).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .args(["arc", "diff"])
        .arg(&before_path)
        .arg(&after_path)
        .arg("--expand-bflan")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("bflan Changed /sections/0/Pat1/name")
    );
}
