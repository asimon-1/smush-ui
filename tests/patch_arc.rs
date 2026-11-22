#[path = "common/fs.rs"]
mod test_fs;

use binrw::BinReaderExt;
use smush_ui::{
    bflyt::Bflyt,
    patch::{PatchArcOptions, patch_archive},
    sarc::{SarcArchive, SarcEntry},
};
use std::{fs, io::Cursor, path::Path, process::Command};
use test_fs::TestDirectory;

fn archive(path: &Path) -> SarcArchive {
    SarcArchive::from_bytes(&fs::read(path).unwrap()).unwrap()
}

fn base_archive(path: &Path) {
    let original = SarcArchive::new(vec![
        SarcEntry::new("anim/keep.bflan", b"keep exactly".to_vec()),
        SarcEntry::new("anim/old.bflan", b"old".to_vec()),
        SarcEntry::new("anim/replace.bflan", b"before".to_vec()),
    ]);
    fs::write(path, original.to_bytes().unwrap()).unwrap();
}

fn write_layout_json(path: &Path) {
    let layout = Bflyt {
        version: [8, 0, 0, 0],
        filesize: 0,
        num_sections: 0,
        sections: Vec::new(),
    };
    fs::write(path, serde_jsonc2::to_vec_pretty(&layout).unwrap()).unwrap();
}

#[test]
fn patches_raw_and_bflyt_entries_in_one_atomic_command() {
    let workspace = TestDirectory::new("patch-arc-mixed");
    let input = workspace.path().join("input.arc");
    base_archive(&input);
    let patch_dir = workspace.path().join("patch");
    fs::create_dir_all(patch_dir.join("files")).unwrap();
    fs::write(patch_dir.join("files/opaque.bflan"), b"opaque bytes\0\xff").unwrap();
    write_layout_json(&patch_dir.join("layout.json"));
    let manifest = r#"{
      "schema_version": 1,
      "upsert": [
        {"archive_path":"anim/replace.bflan","kind":"file","source":"files/opaque.bflan"},
        {"archive_path":"anim/new.bflan","kind":"file","source":"files/opaque.bflan"},
        {"archive_path":"blyt/info_training.bflyt","kind":"bflyt-json","source":"layout.json"},
        {"archive_path":"blyt/new_1.bflyt","kind":"bflyt-json","source":"layout.json"},
        {"archive_path":"blyt/new_2.bflyt","kind":"bflyt-json","source":"layout.json"},
        {"archive_path":"blyt/new_3.bflyt","kind":"bflyt-json","source":"layout.json"},
        {"archive_path":"blyt/new_4.bflyt","kind":"bflyt-json","source":"layout.json"},
        {"archive_path":"blyt/new_5.bflyt","kind":"bflyt-json","source":"layout.json"}
      ],
      "remove": [{"archive_path":"anim/old.bflan"}]
    }"#;
    fs::write(patch_dir.join("patch.json"), manifest).unwrap();
    let first = workspace.path().join("first.arc");
    let second = workspace.path().join("second.arc");
    patch_archive(
        &input,
        patch_dir.join("patch.json"),
        &first,
        PatchArcOptions,
    )
    .unwrap();
    patch_archive(
        &input,
        patch_dir.join("patch.json"),
        &second,
        PatchArcOptions,
    )
    .unwrap();
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    let patched = archive(&first);
    assert_eq!(patched.entries.len(), 9);
    for name in ["anim/replace.bflan", "anim/new.bflan"] {
        assert_eq!(
            patched
                .entries
                .iter()
                .find(|entry| entry.name.as_deref() == Some(name))
                .unwrap()
                .data,
            b"opaque bytes\0\xff"
        );
    }
    assert_eq!(
        patched
            .entries
            .iter()
            .find(|entry| entry.name.as_deref() == Some("anim/keep.bflan"))
            .unwrap()
            .data,
        b"keep exactly"
    );
    assert!(
        !patched
            .entries
            .iter()
            .any(|entry| entry.name.as_deref() == Some("anim/old.bflan"))
    );
    for name in ["info_training", "new_1", "new_2", "new_3", "new_4", "new_5"] {
        let path = format!("blyt/{name}.bflyt");
        let layout_bytes = &patched
            .entries
            .iter()
            .find(|entry| entry.name.as_deref() == Some(path.as_str()))
            .unwrap()
            .data;
        let layout: Bflyt = Cursor::new(layout_bytes).read_le().unwrap();
        assert_eq!(layout.version, [8, 0, 0, 0]);
        assert!(layout.sections.is_empty());
    }

    let cli = Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .current_dir(workspace.path())
        .arg("patch-arc")
        .arg(&input)
        .arg(patch_dir.join("patch.json"))
        .arg(&second)
        .output()
        .unwrap();
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    patch_archive(
        &input,
        patch_dir.join("patch.json"),
        &input,
        PatchArcOptions,
    )
    .unwrap();
    assert_eq!(archive(&input).entries, patched.entries);
}

#[test]
fn json_and_jsonc_share_comment_grammar() {
    let workspace = TestDirectory::new("patch-arc-jsonc");
    let input = workspace.path().join("input.arc");
    base_archive(&input);
    for extension in ["json", "jsonc"] {
        let manifest = workspace.path().join(format!("patch.{extension}"));
        fs::write(&manifest, b"{/* block */\"schema_version\":1,// line\n\"remove\":[{\"archive_path\":\"anim/old.bflan\"}]}").unwrap();
        let output = workspace.path().join(format!("out-{extension}.arc"));
        patch_archive(&input, &manifest, &output, PatchArcOptions).unwrap();
        assert_eq!(archive(&output).entries.len(), 2);
    }
    for extension in ["json", "jsonc"] {
        let manifest = workspace.path().join(format!("bad.{extension}"));
        fs::write(&manifest, r#"{"schema_version":1,"remove":[],}"#).unwrap();
        assert!(
            patch_archive(
                &input,
                &manifest,
                workspace.path().join("bad.arc"),
                PatchArcOptions
            )
            .is_err()
        );
    }
}

#[test]
fn rejects_bad_manifests_before_replacing_output() {
    let workspace = TestDirectory::new("patch-arc-invalid");
    let input = workspace.path().join("input.arc");
    base_archive(&input);
    let output = workspace.path().join("output.arc");
    fs::write(&output, b"sentinel").unwrap();
    let manifest = workspace.path().join("patch.json");
    let cases = [
        r#"{"schema_version":2}"#,
        r#"{"schema_version":1,"typo":0}"#,
        r#"{"schema_version":1,"upsert":[{"archive_path":"new","kind":"file","source":"x","typo":0}]}"#,
        r#"{"schema_version":1,"remove":[{"archive_path":"anim/old.bflan","kind":"file"}]}"#,
        r#"{"schema_version":1,"upsert":[{"archive_path":"new","kind":"bntx","source":"x"}]}"#,
        r#"{"schema_version":1,"upsert":[{"archive_path":"../bad","kind":"file","source":"x"}]}"#,
        r#"{"schema_version":1,"upsert":[{"archive_path":"CON.txt","kind":"file","source":"x"}]}"#,
        r#"{"schema_version":1,"upsert":[{"archive_path":"new","kind":"file","source":"../x"}]}"#,
        r#"{"schema_version":1,"upsert":[{"archive_path":"new","kind":"file","source":"x"},{"archive_path":"new","kind":"file","source":"y"}]}"#,
        r#"{"schema_version":1,"upsert":[{"archive_path":"anim/old.bflan","kind":"file","source":"x"}],"remove":[{"archive_path":"anim/old.bflan"}]}"#,
        r#"{"schema_version":1,"remove":[{"archive_path":"missing"}]}"#,
        r#"{"schema_version":1,"patch":[{"archive_path":"missing","kind":"bntx","source":"x"}]}"#,
        r#"{"schema_version":1,"patch":[{"archive_path":"anim/old.bflan","kind":"file","source":"x"}]}"#,
        r#"{"schema_version":"1"}"#,
        r#"{"schema_version":1,"schema_version":1}"#,
        r#"{"schema_version":1} true"#,
        r#"{schema_version:1}"#,
        r#"{"schema_version":1,"upsert":[{"archive_path":"new","kind":"file","source":"CON.txt"}]}"#,
        r#"{"schema_version":1,"upsert":[{"archive_path":"a//b","kind":"file","source":"x"}]}"#,
    ];
    for (index, case) in cases.iter().enumerate() {
        fs::write(&manifest, case).unwrap();
        assert!(
            patch_archive(&input, &manifest, &output, PatchArcOptions).is_err(),
            "case {index}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"sentinel", "case {index}");
    }
    let unsupported = workspace.path().join("patch.yaml");
    fs::write(&unsupported, r#"{"schema_version":1}"#).unwrap();
    assert!(patch_archive(&input, unsupported, &output, PatchArcOptions).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
}

#[test]
fn rejects_ambiguous_named_entries_before_replacing_output() {
    let workspace = TestDirectory::new("patch-arc-duplicate-names");
    let input = workspace.path().join("input.arc");
    let duplicate = SarcArchive::new(vec![
        SarcEntry::new("dup.bin", b"first".to_vec()),
        SarcEntry::new("dup.bin", b"second".to_vec()),
    ]);
    fs::write(&input, duplicate.to_bytes().unwrap()).unwrap();
    assert_eq!(archive(&input).entries.len(), 2);
    fs::write(workspace.path().join("source.bin"), b"replacement").unwrap();
    let output = workspace.path().join("output.arc");
    fs::write(&output, b"sentinel").unwrap();
    let manifest = workspace.path().join("patch.json");

    for document in [
        r#"{"schema_version":1,"upsert":[{"archive_path":"dup.bin","kind":"file","source":"source.bin"}]}"#,
        r#"{"schema_version":1,"remove":[{"archive_path":"dup.bin"}]}"#,
    ] {
        fs::write(&manifest, document).unwrap();
        let error = patch_archive(&input, &manifest, &output, PatchArcOptions).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("duplicate named SARC entries"));
        assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    }
}

#[test]
fn late_bad_bflyt_source_preserves_input_and_output() {
    let workspace = TestDirectory::new("patch-arc-late");
    let input = workspace.path().join("input.arc");
    base_archive(&input);
    let original = fs::read(&input).unwrap();
    fs::write(workspace.path().join("raw.bin"), b"ready").unwrap();
    fs::write(workspace.path().join("bad.json"), b"not JSON").unwrap();
    let manifest = workspace.path().join("patch.json");
    fs::write(&manifest, r#"{"schema_version":1,"upsert":[{"archive_path":"new","kind":"file","source":"raw.bin"},{"archive_path":"blyt/bad.bflyt","kind":"bflyt-json","source":"bad.json"}]}"#).unwrap();
    let output = workspace.path().join("out.arc");
    fs::write(&output, b"sentinel").unwrap();
    assert!(patch_archive(&input, &manifest, &output, PatchArcOptions).is_err());
    assert_eq!(fs::read(&input).unwrap(), original);
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    assert!(patch_archive(&input, &manifest, &input, PatchArcOptions).is_err());
    assert_eq!(fs::read(&input).unwrap(), original);
}

#[test]
fn stage_six_kinds_are_typed_but_not_applied_yet() {
    let workspace = TestDirectory::new("patch-arc-future-kinds");
    let input = workspace.path().join("input.arc");
    base_archive(&input);
    let manifest = workspace.path().join("patch.json");
    let output = workspace.path().join("output.arc");
    for document in [
        r#"{"schema_version":1,"patch":[{"archive_path":"anim/old.bflan","kind":"bntx","source":"delta"}]}"#,
        r#"{"schema_version":1,"upsert":[{"archive_path":"timg/new.bntx","kind":"bntx-bundle","source":"bundle"}]}"#,
    ] {
        fs::write(&manifest, document).unwrap();
        let error = patch_archive(&input, &manifest, &output, PatchArcOptions).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
        assert!(!output.exists());
    }
}

#[test]
fn source_symlink_is_rejected_before_output_changes() {
    let workspace = TestDirectory::new("patch-arc-symlink");
    let input = workspace.path().join("input.arc");
    base_archive(&input);
    let real = workspace.path().join("real.bin");
    let link = workspace.path().join("link.bin");
    fs::write(&real, b"payload").unwrap();
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&real, &link).is_ok();
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&real, &link).is_ok();
    if !linked {
        return;
    }
    let manifest = workspace.path().join("patch.json");
    fs::write(&manifest, r#"{"schema_version":1,"upsert":[{"archive_path":"new","kind":"file","source":"link.bin"}]}"#).unwrap();
    let output = workspace.path().join("output.arc");
    fs::write(&output, b"sentinel").unwrap();
    let error = patch_archive(&input, &manifest, &output, PatchArcOptions).unwrap_err();
    assert!(error.to_string().contains("symbolic link"));
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
}

#[test]
fn full_archive_can_replace_one_name_with_a_remove_and_upsert() {
    let workspace = TestDirectory::new("patch-arc-count");
    let input = workspace.path().join("input.arc");
    let entries = (0..0x3fff)
        .map(|index| SarcEntry::new(format!("f/{index:05}.bin"), vec![1]))
        .collect();
    fs::write(&input, SarcArchive::new(entries).to_bytes().unwrap()).unwrap();
    fs::write(workspace.path().join("new.bin"), b"new").unwrap();
    let manifest = workspace.path().join("patch.json");
    fs::write(&manifest, r#"{"schema_version":1,"upsert":[{"archive_path":"f/new.bin","kind":"file","source":"new.bin"}],"remove":[{"archive_path":"f/00000.bin"}]}"#).unwrap();
    let output = workspace.path().join("output.arc");
    patch_archive(&input, &manifest, &output, PatchArcOptions).unwrap();
    let patched = archive(&output);
    assert_eq!(patched.entries.len(), 0x3fff);
    assert!(
        patched
            .entries
            .iter()
            .any(|entry| entry.name.as_deref() == Some("f/new.bin") && entry.data == b"new")
    );
    assert!(
        !patched
            .entries
            .iter()
            .any(|entry| entry.name.as_deref() == Some("f/00000.bin"))
    );
}
