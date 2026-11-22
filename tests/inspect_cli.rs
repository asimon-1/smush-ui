#[path = "common/bntx.rs"]
mod common_bntx;

use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use binrw::BinWriterExt;
use common_bntx::synthetic_bntx;
use serde_jsonc2::{Value, jsonc};
use smush_ui::{
    bflyt::Bflyt,
    hash::sha256_hex,
    sarc::{SarcArchive, SarcEntry},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let index = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("inspect-cli-tests")
            .join(format!("{label}-{}-{index}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn list_arc_has_exact_stable_json_and_text_output() {
    let workspace = TestDirectory::new("list-arc");
    let archive_path = workspace.0.join("input.arc");
    let entry = SarcEntry::new("a.bin", b"abc".to_vec());
    write_archive(&archive_path, SarcArchive::new(vec![entry.clone()]));

    let first = cli(&["list-arc", path(&archive_path), "--format", "json"]);
    let second = cli(&["list-arc", path(&archive_path), "--format", "json"]);
    assert_success(&first);
    assert_eq!(first.stdout, second.stdout);
    assert!(first.stdout.ends_with(b"\n"));
    let actual: Value = serde_jsonc2::from_slice(&first.stdout).unwrap();
    assert_eq!(
        jsonc!({
            "schema_version": 1,
            "result": {
                "entry_count": 1,
                "entries": [{
                    "identity": { "kind": "path", "path": "a.bin" },
                    "name_hash": format!("{:08x}", entry.hash),
                    "stored_index": 0,
                    "payload_size": 3,
                    "payload_sha256": sha256_hex(b"abc"),
                    "detected_kind": "opaque",
                    "extension": "bin"
                }]
            }
        }),
        actual
    );

    let text = cli(&["list-arc", path(&archive_path)]);
    assert_success(&text);
    assert_eq!(
        format!(
            "entries: 1\n[0] a.bin size=3 sha256={} kind=opaque extension=bin\n",
            sha256_hex(b"abc")
        )
        .as_bytes(),
        text.stdout
    );
}

#[test]
fn diff_check_uses_status_three_and_parse_errors_remain_status_one() {
    let workspace = TestDirectory::new("check-status");
    let before = workspace.0.join("before.arc");
    let after = workspace.0.join("after.arc");
    let malformed = workspace.0.join("malformed.arc");
    write_archive(
        &before,
        SarcArchive::new(vec![SarcEntry::new("a.bin", vec![1])]),
    );
    write_archive(
        &after,
        SarcArchive::new(vec![SarcEntry::new("a.bin", vec![2])]),
    );
    fs::write(&malformed, b"not a SARC").unwrap();

    let ordinary = cli(&["diff-arc", path(&before), path(&after)]);
    assert_success(&ordinary);
    assert_eq!(
        format!(
            "entries: 1 -> 1; added=0 removed=0 changed=1\n~ a.bin size=1->1 sha256={}->{}\n",
            sha256_hex(&[1]),
            sha256_hex(&[2])
        )
        .as_bytes(),
        ordinary.stdout
    );
    let json_arguments = ["diff-arc", path(&before), path(&after), "--format", "json"];
    let first_json = cli(&json_arguments);
    let second_json = cli(&json_arguments);
    assert_success(&first_json);
    assert_eq!(first_json.stdout, second_json.stdout);
    assert_eq!(
        jsonc!({
            "schema_version": 1,
            "result": {
                "before_entry_count": 1,
                "after_entry_count": 1,
                "added": [],
                "removed": [],
                "changed": [{
                    "identity": { "kind": "path", "path": "a.bin" },
                    "before_stored_index": 0,
                    "after_stored_index": 0,
                    "before_payload_size": 1,
                    "after_payload_size": 1,
                    "before_payload_sha256": sha256_hex(&[1]),
                    "after_payload_sha256": sha256_hex(&[2])
                }]
            }
        }),
        serde_jsonc2::from_slice::<Value>(&first_json.stdout).unwrap()
    );
    let checked = cli(&["diff-arc", path(&before), path(&after), "--check"]);
    assert_eq!(Some(3), checked.status.code());
    assert!(checked.stderr.is_empty());
    let identical_checked = cli(&["diff-arc", path(&before), path(&before), "--check"]);
    assert_success(&identical_checked);
    assert!(identical_checked.stderr.is_empty());

    let failed = cli(&["list-arc", path(&malformed)]);
    assert_eq!(Some(1), failed.status.code());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("parse SARC input"));
}

#[test]
fn bntx_commands_list_diff_and_check_with_deterministic_json() {
    let workspace = TestDirectory::new("bntx");
    let before_path = workspace.0.join("before.bntx");
    let after_path = workspace.0.join("after.bntx");
    let empty_path = workspace.0.join("empty.bntx");
    let single_path = workspace.0.join("single.bntx");
    let before = synthetic_bntx();
    let mut after = before.clone();
    after.textures[0].image_data[0] ^= 0xff;
    fs::write(&before_path, before.to_bytes().unwrap()).unwrap();
    fs::write(&after_path, after.to_bytes().unwrap()).unwrap();
    let mut empty = before.clone();
    empty.textures.clear();
    fs::write(&empty_path, empty.to_bytes().unwrap()).unwrap();
    let mut single = before.clone();
    single.textures.truncate(1);
    fs::write(&single_path, single.to_bytes().unwrap()).unwrap();

    let empty_list_arguments = ["list-bntx", path(&empty_path), "--format", "json"];
    let empty_list = cli(&empty_list_arguments);
    let repeated_empty_list = cli(&empty_list_arguments);
    assert_success(&empty_list);
    assert_eq!(empty_list.stdout, repeated_empty_list.stdout);
    assert_eq!(
        jsonc!({
            "schema_version": 1,
            "result": { "texture_count": 0, "textures": [] }
        }),
        serde_jsonc2::from_slice::<Value>(&empty_list.stdout).unwrap()
    );
    let empty_list_text = cli(&["list-bntx", path(&empty_path)]);
    assert_success(&empty_list_text);
    assert_eq!(b"textures: 0\n", empty_list_text.stdout.as_slice());

    let single_list_arguments = ["list-bntx", path(&single_path), "--format", "json"];
    let single_list = cli(&single_list_arguments);
    let repeated_single_list = cli(&single_list_arguments);
    assert_success(&single_list);
    assert_eq!(single_list.stdout, repeated_single_list.stdout);
    let texture = &single.textures[0];
    assert_eq!(
        jsonc!({
            "schema_version": 1,
            "result": {
                "texture_count": 1,
                "textures": [{
                    "name": "same/name",
                    "index": 0,
                    "width": 9,
                    "height": 7,
                    "depth": 1,
                    "raw_format": 0x1d01,
                    "channel_format": "Bc4",
                    "type_format": "Unorm",
                    "channels": ["One", "One", "One", "Red"],
                    "mip_count": 1,
                    "mip_offsets": texture.mip_offsets.clone(),
                    "image_size": texture.image_data.len(),
                    "image_sha256": sha256_hex(&texture.image_data),
                    "metadata": serde_jsonc2::to_value(&texture.info).unwrap(),
                    "user_data": serde_jsonc2::to_value(&texture.user_data).unwrap()
                }]
            }
        }),
        serde_jsonc2::from_slice::<Value>(&single_list.stdout).unwrap()
    );
    let single_list_text = cli(&["list-bntx", path(&single_path)]);
    assert_success(&single_list_text);
    assert_eq!(
        format!(
            "textures: 1\n[0] same/name 9x7x1 format=0x00001d01 Bc4/Unorm mips=1 image_size={} sha256={}\n",
            texture.image_data.len(),
            sha256_hex(&texture.image_data)
        )
        .as_bytes(),
        single_list_text.stdout
    );

    let empty_diff_arguments = [
        "diff-bntx",
        path(&empty_path),
        path(&empty_path),
        "--format",
        "json",
    ];
    let empty_diff = cli(&empty_diff_arguments);
    let repeated_empty_diff = cli(&empty_diff_arguments);
    assert_success(&empty_diff);
    assert_eq!(empty_diff.stdout, repeated_empty_diff.stdout);
    assert_eq!(
        jsonc!({
            "schema_version": 1,
            "result": {
                "before_texture_count": 0,
                "after_texture_count": 0,
                "added": [],
                "removed": [],
                "changed": [],
                "unchanged_count": 0
            }
        }),
        serde_jsonc2::from_slice::<Value>(&empty_diff.stdout).unwrap()
    );
    let empty_diff_text = cli(&["diff-bntx", path(&empty_path), path(&empty_path)]);
    assert_success(&empty_diff_text);
    assert_eq!(
        b"textures: 0 -> 0; added=0 removed=0 changed=0 unchanged=0\n",
        empty_diff_text.stdout.as_slice()
    );

    let first = cli(&["list-bntx", path(&before_path), "--format", "json"]);
    let second = cli(&["list-bntx", path(&before_path), "--format", "json"]);
    assert_success(&first);
    assert_eq!(first.stdout, second.stdout);
    let list: Value = serde_jsonc2::from_slice(&first.stdout).unwrap();
    assert_eq!(Some(1), list.get("schema_version").and_then(Value::as_u64));
    assert_eq!(
        Some(4),
        list.pointer("/result/texture_count")
            .and_then(Value::as_u64)
    );

    let difference = cli(&[
        "diff-bntx",
        path(&before_path),
        path(&after_path),
        "--format",
        "json",
        "--check",
    ]);
    assert_eq!(Some(3), difference.status.code());
    let difference: Value = serde_jsonc2::from_slice(&difference.stdout).unwrap();
    assert_eq!(
        jsonc!({
            "schema_version": 1,
            "result": {
                "before_texture_count": 4,
                "after_texture_count": 4,
                "added": [],
                "removed": [],
                "changed": [{
                    "name": "same/name",
                    "before_index": 0,
                    "after_index": 0,
                    "raw_image_changed": true,
                    "metadata_changed": false,
                    "mip_layout_changed": false,
                    "user_data_changed": false,
                    "order_changed": false
                }],
                "unchanged_count": 3
            }
        }),
        difference
    );
    let identical_checked = cli(&[
        "diff-bntx",
        path(&before_path),
        path(&before_path),
        "--check",
    ]);
    assert_success(&identical_checked);
    assert!(identical_checked.stderr.is_empty());
    let difference_text = cli(&["diff-bntx", path(&before_path), path(&after_path)]);
    assert_success(&difference_text);
    assert_eq!(
        b"textures: 4 -> 4; added=0 removed=0 changed=1 unchanged=3\n~ same/name index=0->0 image=true metadata=false mips=false user_data=false order=false\n",
        difference_text.stdout.as_slice()
    );
}

#[test]
fn archive_expansions_are_independent_combinable_and_fall_back() {
    let workspace = TestDirectory::new("expansion");
    let before_path = workspace.0.join("before.arc");
    let after_path = workspace.0.join("after.arc");
    let before_bflyt = layout_bytes(&["first", "second"]);
    let after_bflyt = layout_bytes(&["second", "first"]);
    let before_bntx = synthetic_bntx();
    let mut after_bntx = before_bntx.clone();
    after_bntx.textures.swap(0, 1);
    write_archive(
        &before_path,
        SarcArchive::new(vec![
            SarcEntry::new("blyt/main.bflyt", before_bflyt),
            SarcEntry::new("timg/main.bntx", before_bntx.to_bytes().unwrap()),
            SarcEntry::new("timg/broken.bntx", b"BNTX\0\0\0\0before".to_vec()),
            SarcEntry::new("mixed/resource.bin", layout_bytes(&["mixed"])),
        ]),
    );
    write_archive(
        &after_path,
        SarcArchive::new(vec![
            SarcEntry::new("blyt/main.bflyt", after_bflyt),
            SarcEntry::new("timg/main.bntx", after_bntx.to_bytes().unwrap()),
            SarcEntry::new("timg/broken.bntx", b"BNTX\0\0\0\0after".to_vec()),
            SarcEntry::new("mixed/resource.bin", synthetic_bntx().to_bytes().unwrap()),
        ]),
    );

    let bflyt_only = diff_arc_json(&before_path, &after_path, &["--expand-bflyt"]);
    let changes = bflyt_only
        .pointer("/result/changed")
        .and_then(Value::as_array)
        .unwrap();
    assert!(changes.iter().any(|change| change.get("bflyt").is_some()));
    assert!(changes.iter().all(|change| change.get("bntx").is_none()));
    let paths = changes
        .iter()
        .filter_map(|change| change.pointer("/bflyt/changes").and_then(Value::as_array))
        .flatten()
        .filter_map(|change| change.get("path").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(paths.contains(&"/sections/0/Layout/name"));
    assert!(paths.contains(&"/sections/1/Layout/name"));

    let bntx_only = diff_arc_json(&before_path, &after_path, &["--expand-bntx"]);
    let changes = bntx_only
        .pointer("/result/changed")
        .and_then(Value::as_array)
        .unwrap();
    assert!(changes.iter().any(|change| change.get("bntx").is_some()));
    assert!(changes.iter().all(|change| change.get("bflyt").is_none()));

    let combined = diff_arc_json(
        &before_path,
        &after_path,
        &["--expand-bflyt", "--expand-bntx"],
    );
    let changes = combined
        .pointer("/result/changed")
        .and_then(Value::as_array)
        .unwrap();
    assert!(changes.iter().any(|change| change.get("bflyt").is_some()));
    assert!(changes.iter().any(|change| change.get("bntx").is_some()));
    let broken = changes
        .iter()
        .find(|change| {
            change.pointer("/identity/path").and_then(Value::as_str) == Some("timg/broken.bntx")
        })
        .unwrap();
    assert!(broken.get("bntx").is_none());
    let nonmatching = changes
        .iter()
        .find(|change| {
            change.pointer("/identity/path").and_then(Value::as_str) == Some("mixed/resource.bin")
        })
        .unwrap();
    assert!(nonmatching.get("bflyt").is_none());
    assert!(nonmatching.get("bntx").is_none());
}

fn layout_bytes(names: &[&str]) -> Vec<u8> {
    let sections = names
        .iter()
        .map(|name| {
            jsonc!({
                "Layout": {
                    "is_drawn_from_middle": true,
                    "width": 1280.0,
                    "height": 720.0,
                    "max_parts_height": 0.0,
                    "max_parts_width": 0.0,
                    "name": name
                }
            })
        })
        .collect::<Vec<_>>();
    let layout: Bflyt = serde_jsonc2::from_value(jsonc!({
        "version": [2, 2, 0, 0],
        "sections": sections
    }))
    .unwrap();
    let mut bytes = Cursor::new(Vec::new());
    bytes.write_le(&layout).unwrap();
    bytes.into_inner()
}

fn diff_arc_json(before: &Path, after: &Path, flags: &[&str]) -> Value {
    let mut arguments = vec!["diff-arc", path(before), path(after), "--format", "json"];
    arguments.extend_from_slice(flags);
    let output = cli(&arguments);
    assert_success(&output);
    serde_jsonc2::from_slice(&output.stdout).unwrap()
}

fn write_archive(path: &Path, archive: SarcArchive) {
    fs::write(path, archive.to_bytes().unwrap()).unwrap();
}

fn cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .args(arguments)
        .output()
        .unwrap()
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "status: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
