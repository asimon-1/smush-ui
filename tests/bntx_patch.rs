mod common;

use common::{
    bntx::{synthetic_bntx, test_image},
    fs::TestDirectory,
};
use smush_ui::bntx::{
    AddTextureOptions, Bntx, ChannelMode, ImageFormat, NewTextureFormat, TexturePatch,
    TextureProfile, TextureUpsert, add_texture, apply_texture_patch, encode_texture,
    patch::{PatchSourceFormat, create_bntx_patch, patch_bntx_file},
    write_png,
};
use std::{fs, process::Command};

#[test]
fn generated_bin_delta_round_trips_mixed_edits_and_order() {
    let workspace = TestDirectory::new("bin-delta");
    let base = synthetic_bntx();
    let mut modified = base.clone();
    modified.textures.remove(1);
    modified.textures[0].user_data.clear();
    let mut added = modified.textures[0].clone();
    added.name = "new texture".into();
    modified.textures.insert(0, added);
    modified.textures.swap(1, 2);
    let base_path = workspace.path().join("base.bntx");
    let modified_path = workspace.path().join("modified.bntx");
    let output = workspace.path().join("out.bntx");
    let patch_dir = workspace.path().join("delta");
    fs::write(&base_path, base.to_bytes().unwrap()).unwrap();
    fs::write(&modified_path, modified.to_bytes().unwrap()).unwrap();
    let summary = create_bntx_patch(
        &base_path,
        &modified_path,
        &patch_dir,
        PatchSourceFormat::Bin,
    )
    .unwrap();
    assert_eq!((summary.upserts, summary.removals), (2, 1));
    let manifest = fs::read_to_string(patch_dir.join("bntx-patch.json")).unwrap();
    assert!(!manifest.contains("reserved"));
    assert!(manifest.contains("final_texture_order"));
    patch_bntx_file(&base_path, &patch_dir, &output).unwrap();
    assert_eq!(
        Bntx::from_bytes(&fs::read(&output).unwrap()).unwrap(),
        Bntx::from_bytes(&fs::read(&modified_path).unwrap()).unwrap()
    );
    let conflict = create_bntx_patch(
        &base_path,
        &modified_path,
        &patch_dir,
        PatchSourceFormat::Bin,
    )
    .unwrap_err();
    assert_eq!(conflict.kind(), std::io::ErrorKind::AlreadyExists);
}

#[test]
fn generated_empty_and_reorder_only_deltas() {
    let workspace = TestDirectory::new("empty-delta");
    let base = synthetic_bntx();
    let base_path = workspace.path().join("base.bntx");
    fs::write(&base_path, base.to_bytes().unwrap()).unwrap();
    let empty = workspace.path().join("empty");
    create_bntx_patch(&base_path, &base_path, &empty, PatchSourceFormat::Png).unwrap();
    assert!(!empty.join("textures").exists());
    let output = workspace.path().join("empty.bntx");
    patch_bntx_file(&base_path, &empty, &output).unwrap();
    assert_eq!(Bntx::from_bytes(&fs::read(output).unwrap()).unwrap(), base);
    let mut reordered = base.clone();
    reordered.textures.reverse();
    let reordered_path = workspace.path().join("reordered.bntx");
    fs::write(&reordered_path, reordered.to_bytes().unwrap()).unwrap();
    let reorder = workspace.path().join("reorder");
    create_bntx_patch(
        &base_path,
        &reordered_path,
        &reorder,
        PatchSourceFormat::Bin,
    )
    .unwrap();
    assert!(!reorder.join("textures").exists());
    assert!(
        fs::read_to_string(reorder.join("bntx-patch.json"))
            .unwrap()
            .contains("final_texture_order")
    );
    let output = workspace.path().join("reordered-output.bntx");
    patch_bntx_file(&base_path, &reorder, &output).unwrap();
    assert_eq!(
        Bntx::from_bytes(&fs::read(output).unwrap()).unwrap(),
        reordered
    );
}

#[test]
fn generated_replacement_and_removal_deltas() {
    let workspace = TestDirectory::new("simple-deltas");
    let base = synthetic_bntx();
    let base_path = workspace.path().join("base.bntx");
    fs::write(&base_path, base.to_bytes().unwrap()).unwrap();
    let mut replacement = base.clone();
    replacement.textures[1].info.flags ^= 1;
    let replacement_path = workspace.path().join("replacement.bntx");
    fs::write(&replacement_path, replacement.to_bytes().unwrap()).unwrap();
    let replacement_dir = workspace.path().join("replacement");
    let summary = create_bntx_patch(
        &base_path,
        &replacement_path,
        &replacement_dir,
        PatchSourceFormat::Bin,
    )
    .unwrap();
    assert_eq!((summary.upserts, summary.removals), (1, 0));
    let output = workspace.path().join("replacement-output.bntx");
    patch_bntx_file(&base_path, &replacement_dir, &output).unwrap();
    assert_eq!(
        Bntx::from_bytes(&fs::read(output).unwrap()).unwrap(),
        replacement
    );

    let mut removed = base.clone();
    removed.textures.remove(1);
    let removed_path = workspace.path().join("removed.bntx");
    fs::write(&removed_path, removed.to_bytes().unwrap()).unwrap();
    let removed_dir = workspace.path().join("removed");
    let summary = create_bntx_patch(
        &base_path,
        &removed_path,
        &removed_dir,
        PatchSourceFormat::Png,
    )
    .unwrap();
    assert_eq!((summary.upserts, summary.removals), (0, 1));
    assert!(!removed_dir.join("textures").exists());
    let output = workspace.path().join("removed-output.bntx");
    patch_bntx_file(&base_path, &removed_dir, &output).unwrap();
    assert_eq!(
        Bntx::from_bytes(&fs::read(output).unwrap()).unwrap(),
        removed
    );
}

#[test]
fn malformed_final_order_preserves_existing_output() {
    let workspace = TestDirectory::new("order-reject");
    let base = synthetic_bntx();
    let base_path = workspace.path().join("base.bntx");
    fs::write(&base_path, base.to_bytes().unwrap()).unwrap();
    let patch_dir = workspace.path().join("patch");
    create_bntx_patch(&base_path, &base_path, &patch_dir, PatchSourceFormat::Bin).unwrap();
    let manifest_path = patch_dir.join("bntx-patch.json");
    let original = fs::read_to_string(&manifest_path).unwrap();
    let output = workspace.path().join("output.bntx");
    fs::write(&output, b"preserve").unwrap();
    for order in ["[]", "[\"unknown\", \"unknown\", \"unknown\", \"unknown\"]"] {
        let malformed = format!(
            "{{\"final_texture_order\": {order},{}",
            original.trim_start_matches('{')
        );
        fs::write(&manifest_path, malformed).unwrap();
        assert!(patch_bntx_file(&base_path, &patch_dir, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"preserve");
    }
}

#[test]
fn bin_info_rejects_unknown_fields() {
    let workspace = TestDirectory::new("bin-strict");
    let base = synthetic_bntx();
    let base_path = workspace.path().join("base.bntx");
    fs::write(&base_path, base.to_bytes().unwrap()).unwrap();
    let patch_dir = workspace.path().join("patch");
    create_bntx_patch(&base_path, &base_path, &patch_dir, PatchSourceFormat::Bin).unwrap();
    let mut changed = base.clone();
    changed.textures[0].info.flags ^= 1;
    let changed_path = workspace.path().join("changed.bntx");
    fs::write(&changed_path, changed.to_bytes().unwrap()).unwrap();
    let strict = workspace.path().join("strict");
    create_bntx_patch(&base_path, &changed_path, &strict, PatchSourceFormat::Bin).unwrap();
    let manifest_path = strict.join("bntx-patch.json");
    let original = fs::read_to_string(&manifest_path).unwrap();
    for field in ["reserved", "arbitrary"] {
        let altered = original.replacen("\"flags\":", &format!("\"{field}\": 0, \"flags\":"), 1);
        fs::write(&manifest_path, altered).unwrap();
        assert!(patch_bntx_file(&base_path, &strict, workspace.path().join("out.bntx")).is_err());
    }
}

#[test]
fn default_generator_uses_png_when_one_mip_profile_is_reconstructible() {
    let workspace = TestDirectory::new("png-delta");
    let base = synthetic_bntx();
    let mut modified = base.clone();
    let image = test_image(8, 8, 29);
    add_texture(
        &mut modified,
        "new",
        &image,
        AddTextureOptions {
            profile: Some(TextureProfile::Format(NewTextureFormat::Bc7Unorm)),
            ..Default::default()
        },
    )
    .unwrap();
    let mut unsupported = modified.textures[0].clone();
    unsupported.name = "bc1".into();
    unsupported.info.format = ImageFormat::from_raw(0x1a06);
    modified.textures.push(unsupported);
    let base_path = workspace.path().join("base.bntx");
    let modified_path = workspace.path().join("modified.bntx");
    fs::write(&base_path, base.to_bytes().unwrap()).unwrap();
    fs::write(&modified_path, modified.to_bytes().unwrap()).unwrap();
    let patch_dir = workspace.path().join("delta");
    let summary = create_bntx_patch(
        &base_path,
        &modified_path,
        &patch_dir,
        PatchSourceFormat::Png,
    )
    .unwrap();
    assert_eq!(summary.png_fallbacks, ["bc1"]);
    let manifest = fs::read_to_string(patch_dir.join("bntx-patch.json")).unwrap();
    assert!(manifest.contains("\"source_format\": \"png\""));
    assert!(manifest.contains("\"source_format\": \"bin\""));
    let output = workspace.path().join("out.bntx");
    patch_bntx_file(&base_path, &patch_dir, &output).unwrap();
    let patched = Bntx::from_bytes(&fs::read(output).unwrap()).unwrap();
    assert_eq!(
        &patched.textures[..base.textures.len()],
        base.textures.as_slice()
    );
    assert_eq!(
        patched.textures.last().unwrap(),
        modified.textures.last().unwrap()
    );
    assert_eq!(
        patched.textures[base.textures.len()].info,
        modified.textures[base.textures.len()].info
    );
    assert_eq!(patched.textures[base.textures.len()].mip_offsets.len(), 1);
}

#[test]
fn png_edits_use_one_mip_level() {
    let mut bntx = synthetic_bntx();
    let image = test_image(64, 64, 3);
    add_texture(
        &mut bntx,
        "template",
        &image,
        AddTextureOptions {
            profile: Some(TextureProfile::Format(NewTextureFormat::Bc4Unorm)),
            ..Default::default()
        },
    )
    .unwrap();
    let template = bntx.textures.last_mut().unwrap();
    template.mip_offsets = vec![0; 2];
    let encoded = encode_texture(template, &image, ChannelMode::Resolved).unwrap();
    template.image_data = encoded.image_data;
    template.mip_offsets = encoded.mip_offsets;
    let untouched = template.clone();
    let mut replacement = template.clone();
    replacement.name = "replacement".into();
    bntx.textures.push(replacement);
    add_texture(
        &mut bntx,
        "inherited",
        &image,
        AddTextureOptions {
            profile: Some(TextureProfile::Like("template".into())),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bntx.textures.last().unwrap().mip_offsets.len(), 1);
    add_texture(
        &mut bntx,
        "replacement",
        &image,
        AddTextureOptions {
            replace: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        bntx.textures
            .iter()
            .find(|texture| texture.name == "replacement")
            .unwrap()
            .mip_offsets
            .len(),
        1
    );
    assert_eq!(
        bntx.textures
            .iter()
            .find(|texture| texture.name == "template"),
        Some(&untouched)
    );
    let patched = apply_texture_patch(
        &bntx,
        &TexturePatch {
            upsert: vec![TextureUpsert {
                name: "patched".into(),
                image: image.clone(),
                profile: Some(TextureProfile::Like("template".into())),
            }],
            remove: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(patched.textures.last().unwrap().mip_offsets.len(), 1);
    assert_eq!(
        patched
            .textures
            .iter()
            .find(|texture| texture.name == "template"),
        Some(&untouched)
    );
    let reparsed = Bntx::from_bytes(&patched.to_bytes().unwrap()).unwrap();
    assert_eq!(reparsed.textures, patched.textures);
}

#[test]
fn batch_upserts_preserve_order_and_reject_duplicate_or_missing_targets() {
    let original = synthetic_bntx();
    let image = test_image(8, 8, 3);
    let upsert = |name: &str| TextureUpsert {
        name: name.into(),
        image: image.clone(),
        profile: Some(TextureProfile::Format(NewTextureFormat::Bc4Unorm)),
    };
    let patch = TexturePatch {
        upsert: vec![upsert("new_a"), upsert("same/name"), upsert("new_b")],
        remove: vec!["same^name".into()],
    };
    let result = apply_texture_patch(&original, &patch).unwrap();
    assert_eq!(result.textures[0].name, "same/name");
    assert_eq!(result.textures[result.textures.len() - 2].name, "new_a");
    assert_eq!(result.textures.last().unwrap().name, "new_b");
    assert!(
        !result
            .textures
            .iter()
            .any(|texture| texture.name == "same^name")
    );
    assert_eq!(original, synthetic_bntx());
    assert!(
        apply_texture_patch(
            &original,
            &TexturePatch {
                upsert: vec![upsert("new_a"), upsert("new_a")],
                remove: vec![]
            }
        )
        .is_err()
    );
    assert!(
        apply_texture_patch(
            &original,
            &TexturePatch {
                upsert: vec![upsert("new_a")],
                remove: vec!["new_a".into()]
            }
        )
        .is_err()
    );
    assert!(
        apply_texture_patch(
            &original,
            &TexturePatch {
                upsert: vec![],
                remove: vec!["missing".into()]
            }
        )
        .is_err()
    );
    assert!(
        apply_texture_patch(
            &original,
            &TexturePatch {
                upsert: vec![TextureUpsert {
                    profile: Some(TextureProfile::Like("missing".into())),
                    ..upsert("new_a")
                }],
                remove: vec![]
            }
        )
        .is_err()
    );

    let mut unsupported = original.clone();
    unsupported.textures[0].info.format = ImageFormat::from_raw(0x1a01);
    let error = apply_texture_patch(
        &unsupported,
        &TexturePatch {
            upsert: vec![TextureUpsert {
                profile: None,
                ..upsert("same/name")
            }],
            remove: vec![],
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("upsert[0] \"same/name\""));
    assert!(error.to_string().contains("unsupported BNTX image format"));

    let mut full = original.clone();
    let mut template = full.textures[0].clone();
    template.user_data.clear();
    template.image_data = vec![0];
    template.mip_offsets = vec![0];
    full.textures = (0..usize::from(u16::MAX))
        .map(|index| {
            let mut texture = template.clone();
            texture.name = format!("existing_{index}");
            texture
        })
        .collect();
    let error = apply_texture_patch(
        &full,
        &TexturePatch {
            upsert: vec![upsert("overflow")],
            remove: vec![],
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("texture count exceeds 65535"));
}

#[test]
fn batch_of_more_than_one_hundred_textures_round_trips() {
    let original = synthetic_bntx();
    let image = test_image(4, 4, 1);
    let patch = TexturePatch {
        upsert: (0..101)
            .map(|index| TextureUpsert {
                name: format!("texture_{index:03}"),
                image: image.clone(),
                profile: Some(TextureProfile::Format(NewTextureFormat::Bc4Unorm)),
            })
            .collect(),
        remove: Vec::new(),
    };
    let result = apply_texture_patch(&original, &patch).unwrap();
    assert_eq!(result.textures.len(), original.textures.len() + 101);
    assert_eq!(result.textures[original.textures.len()].name, "texture_000");
    assert_eq!(result.textures.last().unwrap().name, "texture_100");
    assert_eq!(
        &result.textures[..original.textures.len()],
        original.textures.as_slice()
    );
    let reparsed = Bntx::from_bytes(&result.to_bytes().unwrap()).unwrap();
    assert_eq!(reparsed.textures, result.textures);

    let workspace = TestDirectory::new("patch-large-batch");
    let input = workspace.path().join("input.bntx");
    let output = workspace.path().join("output.bntx");
    let directory = workspace.path().join("patch");
    fs::create_dir_all(directory.join("textures")).unwrap();
    fs::write(&input, original.to_bytes().unwrap()).unwrap();
    fs::write(
        directory.join("textures/shared.png"),
        write_png(&image).unwrap(),
    )
    .unwrap();
    let entries = (0..101).map(|index| format!(r#"{{"name":"texture_{index:03}","source_format":"png","source":"textures/shared.png","format":"bc4-unorm"}}"#)).collect::<Vec<_>>().join(",");
    fs::write(
        directory.join("bntx-patch.json"),
        format!(r#"{{"schema_version":1,"upsert":[{entries}]}}"#),
    )
    .unwrap();
    patch_bntx_file(&input, &directory, &output).unwrap();
    let listed = Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .args(["list-bntx", output.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed: serde_jsonc2::Value = serde_jsonc2::from_slice(&listed.stdout).unwrap();
    assert_eq!(
        listed
            .pointer("/result/texture_count")
            .and_then(|count| count.as_u64()),
        Some((original.textures.len() + 101) as u64)
    );
    let diffed = Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .args([
            "diff-bntx",
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(diffed.status.success());
    let diffed: serde_jsonc2::Value = serde_jsonc2::from_slice(&diffed.stdout).unwrap();
    assert_eq!(
        diffed
            .pointer("/result/added")
            .and_then(|added| added.as_array())
            .map(Vec::len),
        Some(101)
    );
    let from_manifest = Bntx::from_bytes(&fs::read(output).unwrap()).unwrap();
    assert_eq!(from_manifest.textures, result.textures);
}

#[test]
fn manifest_rejects_unknown_fields_duplicates_and_ambiguous_files() {
    let workspace = TestDirectory::new("patch-manifest");
    let input = workspace.path().join("input.bntx");
    let output = workspace.path().join("output.bntx");
    let patch_dir = workspace.path().join("patch");
    fs::create_dir_all(patch_dir.join("textures")).unwrap();
    fs::write(&input, synthetic_bntx().to_bytes().unwrap()).unwrap();
    fs::write(&output, b"sentinel").unwrap();
    fs::write(
        patch_dir.join("textures/new.png"),
        write_png(&test_image(8, 8, 2)).unwrap(),
    )
    .unwrap();
    let manifest_path = patch_dir.join("bntx-patch.json");
    for invalid in [
        r#"{"schema_version":2}"#,
        r#"{"schema_version":1,"unknown":true}"#,
        r#"{"schema_version":1,"remove":[{"name":"same/name","source":"x"}]}"#,
        r#"{"schema_version":1,"remove":[{"name":"same/name"},{"name":"same/name"}]}"#,
        r#"{"schema_version":1,"remove":[{"name":"missing"}]}"#,
        r#"{"schema_version":1,"upsert":[{"name":"new","source_format":"png","source":"textures/new.png","like":"same/name","format":"bc4-unorm"}]}"#,
        r#"{"schema_version":1,"upsert":[{"name":"new","source_format":"png","source":"textures/new.png","mip_count":7}]}"#,
        r#"{"schema_version":1,"upsert":[{"name":"new","source_format":"png","source":"../new.png"}]}"#,
        r#"{"schema_version":1,}"#,
    ] {
        fs::write(&manifest_path, invalid).unwrap();
        assert!(
            patch_bntx_file(&input, &patch_dir, &output).is_err(),
            "{invalid}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    }
    fs::write(&manifest_path, r#"{"schema_version":1}"#).unwrap();
    fs::write(
        patch_dir.join("bntx-patch.jsonc"),
        r#"{"schema_version":1}"#,
    )
    .unwrap();
    assert!(patch_bntx_file(&input, &patch_dir, &output).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    fs::remove_file(&manifest_path).unwrap();
    fs::remove_file(patch_dir.join("bntx-patch.jsonc")).unwrap();
    fs::write(patch_dir.join("bntx-patch.JSON"), r#"{"schema_version":1}"#).unwrap();
    let error = patch_bntx_file(&input, &patch_dir, &output).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported BNTX patch manifest filename")
    );
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
}

#[test]
fn patch_directory_is_atomic_and_accepts_jsonc() {
    let workspace = TestDirectory::new("patch-bntx");
    let input = workspace.path().join("input.bntx");
    let output = workspace.path().join("output.bntx");
    let patch_dir = workspace.path().join("patch");
    fs::create_dir_all(patch_dir.join("textures")).unwrap();
    let original = synthetic_bntx().to_bytes().unwrap();
    fs::write(&input, &original).unwrap();
    fs::write(
        patch_dir.join("textures/new.png"),
        write_png(&test_image(8, 8, 2)).unwrap(),
    )
    .unwrap();
    let manifest = r#"{
        // A PNG upsert and an existing texture removal.
        "schema_version": 1,
        "upsert": [{"name":"new_texture","source_format":"png","source":"textures/new.png","format":"bc4-unorm"}],
        "remove": [{"name":"same^name"}]
    }"#;
    fs::write(patch_dir.join("bntx-patch.jsonc"), manifest).unwrap();
    patch_bntx_file(&input, &patch_dir, &output).unwrap();
    let result = Bntx::from_bytes(&fs::read(&output).unwrap()).unwrap();
    assert!(
        result
            .textures
            .iter()
            .any(|texture| texture.name == "new_texture")
    );
    assert!(
        !result
            .textures
            .iter()
            .any(|texture| texture.name == "same^name")
    );
    assert_eq!(fs::read(&input).unwrap(), original);

    let cli_output = workspace.path().join("cli-output.bntx");
    let cli = Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .args([
            "patch-bntx",
            input.to_str().unwrap(),
            patch_dir.to_str().unwrap(),
            cli_output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    assert_eq!(fs::read(&cli_output).unwrap(), fs::read(&output).unwrap());

    fs::write(&output, b"sentinel").unwrap();
    let late_failure = r#"{"schema_version":1,"upsert":[
        {"name":"first","source_format":"png","source":"textures/new.png","format":"bc4-unorm"},
        {"name":"second","source_format":"png","source":"textures/missing.png","format":"bc4-unorm"}
    ]}"#;
    fs::write(patch_dir.join("bntx-patch.jsonc"), late_failure).unwrap();
    let error = patch_bntx_file(&input, &patch_dir, &output).unwrap_err();
    assert!(error.to_string().contains("upsert[1] \"second\""));
    assert!(error.to_string().contains("bntx-patch.jsonc"));
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    assert_eq!(fs::read(&input).unwrap(), original);
    assert!(patch_bntx_file(&input, &patch_dir, &input).is_err());
    assert_eq!(fs::read(&input).unwrap(), original);

    fs::write(patch_dir.join("textures/missing.png"), b"bad PNG").unwrap();
    let error = patch_bntx_file(&input, &patch_dir, &output).unwrap_err();
    assert!(error.to_string().contains("upsert[1] \"second\""));
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");

    fs::write(patch_dir.join("bntx-patch.jsonc"), manifest).unwrap();
    patch_bntx_file(&input, &patch_dir, &input).unwrap();
    assert!(
        Bntx::from_bytes(&fs::read(&input).unwrap())
            .unwrap()
            .textures
            .iter()
            .any(|texture| texture.name == "new_texture")
    );
}

#[test]
fn cli_exposes_patch_and_add_commands() {
    let help = Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .arg("add-bntx-texture")
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(!String::from_utf8_lossy(&help.stdout).contains("--mip-count"));
    let help = Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .arg("patch-bntx")
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());

    let workspace = TestDirectory::new("patch-cli");
    let input = workspace.path().join("input.bntx");
    let png = workspace.path().join("image.png");
    let output = workspace.path().join("output.bntx");
    fs::write(&input, synthetic_bntx().to_bytes().unwrap()).unwrap();
    fs::write(&png, write_png(&test_image(64, 64, 4)).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_smush-ui"))
        .args([
            "add-bntx-texture",
            input.to_str().unwrap(),
            png.to_str().unwrap(),
            "added",
            output.to_str().unwrap(),
            "--format",
            "bc4-unorm",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bntx = Bntx::from_bytes(&fs::read(output).unwrap()).unwrap();
    assert_eq!(bntx.textures.last().unwrap().mip_offsets.len(), 1);
}
