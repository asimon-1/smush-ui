mod common;

use common::{bntx::synthetic_bntx, fs::TestDirectory};
use smush_ui::bntx::{Bntx, ChannelMode, RgbaImage, fs::BundleManifest, read_png, write_png};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

#[test]
fn help_lists_bntx_commands_and_flags() {
    let output = cli_command().arg("--help").output().unwrap();
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("unpack-bntx"));
    assert!(stdout.contains("pack-bntx"));

    let output = cli_command()
        .args(["unpack-bntx", "--help"])
        .output()
        .unwrap();
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--no-preview"));
    assert!(stdout.contains("--raw-channels"));

    let output = cli_command()
        .args(["pack-bntx", "--help"])
        .output()
        .unwrap();
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("--from-png"));
}

#[test]
fn unpacks_and_losslessly_repacks_a_bntx() {
    let workspace = TestDirectory::new("round-trip");
    let input = workspace.path().join("input.bntx");
    let bundle = workspace.path().join("bundle");
    let output = workspace.path().join("output.bntx");
    let original = synthetic_bntx().to_bytes().unwrap();
    fs::write(&input, &original).unwrap();

    assert_success(
        &cli_command()
            .arg("unpack-bntx")
            .arg(&input)
            .arg(&bundle)
            .output()
            .unwrap(),
    );
    let manifest = read_manifest(&bundle);
    assert_eq!(manifest.preview_channels, ChannelMode::Resolved);
    assert!(manifest.textures.iter().all(|entry| {
        entry.preview_hash.is_some() && bundle.join(&entry.preview_path).is_file()
    }));

    assert_success(
        &cli_command()
            .arg("pack-bntx")
            .arg(&bundle)
            .arg(&output)
            .output()
            .unwrap(),
    );
    assert_eq!(fs::read(output).unwrap(), original);
}

#[test]
fn unpack_flags_control_preview_creation_and_channels() {
    let workspace = TestDirectory::new("unpack-flags");
    let input = workspace.path().join("input.bntx");
    fs::write(&input, synthetic_bntx().to_bytes().unwrap()).unwrap();

    let no_preview = workspace.path().join("no-preview");
    assert_success(
        &cli_command()
            .arg("unpack-bntx")
            .arg(&input)
            .arg(&no_preview)
            .arg("--no-preview")
            .output()
            .unwrap(),
    );
    let manifest = read_manifest(&no_preview);
    assert!(manifest.textures.iter().all(|entry| {
        entry.preview_hash.is_none() && !no_preview.join(&entry.preview_path).exists()
    }));
    let no_preview_output = workspace.path().join("no-preview.bntx");
    let result = cli_command()
        .arg("pack-bntx")
        .arg(&no_preview)
        .arg(&no_preview_output)
        .arg("--from-png")
        .output()
        .unwrap();
    assert_failure_contains(&result, "0000-same_name.png");
    assert!(!no_preview_output.exists());

    let raw_channels = workspace.path().join("raw-channels");
    assert_success(
        &cli_command()
            .arg("unpack-bntx")
            .arg(&input)
            .arg(&raw_channels)
            .arg("--raw-channels")
            .output()
            .unwrap(),
    );
    let manifest = read_manifest(&raw_channels);
    assert_eq!(manifest.preview_channels, ChannelMode::Raw);
    assert!(manifest.textures.iter().all(|entry| {
        entry.preview_hash.is_some() && raw_channels.join(&entry.preview_path).is_file()
    }));
}

#[test]
fn from_png_reencodes_edits_and_failures_preserve_the_output() {
    let workspace = TestDirectory::new("from-png");
    let input = workspace.path().join("input.bntx");
    let bundle = workspace.path().join("bundle");
    let output = workspace.path().join("output.bntx");
    let original = synthetic_bntx();
    fs::write(&input, original.to_bytes().unwrap()).unwrap();

    assert_success(
        &cli_command()
            .arg("unpack-bntx")
            .arg(&input)
            .arg(&bundle)
            .output()
            .unwrap(),
    );
    let manifest = read_manifest(&bundle);
    let edited_index = 2;
    let png_path = bundle.join(&manifest.textures[edited_index].preview_path);
    let mut image = read_png(&fs::read(&png_path).unwrap()).unwrap();
    for pixel in image.pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[10, 30, 220, 170]);
    }
    fs::write(&png_path, write_png(&image).unwrap()).unwrap();

    assert_success(
        &cli_command()
            .arg("pack-bntx")
            .arg(&bundle)
            .arg(&output)
            .arg("--from-png")
            .output()
            .unwrap(),
    );
    let edited = Bntx::from_bytes(&fs::read(&output).unwrap()).unwrap();
    for (index, (before, after)) in original.textures.iter().zip(&edited.textures).enumerate() {
        if index == edited_index {
            assert_ne!(after.image_data, before.image_data);
        } else {
            assert_eq!(after.image_data, before.image_data);
        }
    }

    fs::write(
        &png_path,
        write_png(&RgbaImage::new(1, 1, vec![0; 4]).unwrap()).unwrap(),
    )
    .unwrap();
    fs::write(&output, b"preserved").unwrap();
    let result = cli_command()
        .arg("pack-bntx")
        .arg(&bundle)
        .arg(&output)
        .arg("--from-png")
        .output()
        .unwrap();
    assert_failure_contains(&result, "color:*?");
    assert_eq!(fs::read(output).unwrap(), b"preserved");
}

#[test]
fn malformed_input_and_output_conflicts_are_reported_without_partial_outputs() {
    let workspace = TestDirectory::new("failures");
    let malformed = workspace.path().join("malformed.bntx");
    let malformed_output = workspace.path().join("malformed-output");
    fs::write(&malformed, b"not a BNTX").unwrap();

    let result = cli_command()
        .arg("unpack-bntx")
        .arg(&malformed)
        .arg(&malformed_output)
        .output()
        .unwrap();
    assert_failure_contains(&result, &malformed.display().to_string());
    assert!(!malformed_output.exists());

    let input = workspace.path().join("input.bntx");
    let conflict = workspace.path().join("conflict");
    fs::write(&input, synthetic_bntx().to_bytes().unwrap()).unwrap();
    fs::create_dir_all(conflict.join("textures")).unwrap();
    let existing = conflict.join("textures/0000-same_name.bin");
    fs::write(&existing, b"existing").unwrap();

    let result = cli_command()
        .arg("unpack-bntx")
        .arg(&input)
        .arg(&conflict)
        .output()
        .unwrap();
    assert_failure_contains(&result, &existing.display().to_string());
    assert_eq!(fs::read(existing).unwrap(), b"existing");
    assert!(!conflict.join("bntx.json").exists());
}

fn read_manifest(bundle: &Path) -> BundleManifest {
    serde_json::from_slice(&fs::read(bundle.join("bntx.json")).unwrap()).unwrap()
}

fn cli_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_smush-ui"))
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_failure_contains(output: &Output, expected: &str) {
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected),
        "stderr did not contain {expected:?}:\n{stderr}"
    );
}
