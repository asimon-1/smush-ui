mod common;

use common::{
    bntx::{BC4_UNORM, BC5_UNORM, BC7_UNORM, synthetic_bntx, test_image},
    fs::TestDirectory,
};
use smush_ui::bntx::{
    Bntx, ChannelMode, RgbaImage, derive_block_height_exponent, fs::BundleManifest, read_png,
    write_png,
};
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
    assert!(stdout.contains("add-bntx-texture"));
    assert!(stdout.contains("create-bntx"));

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

    let output = cli_command()
        .args(["add-bntx-texture", "--help"])
        .output()
        .unwrap();
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--like"));
    assert!(stdout.contains("--format"));
    assert!(stdout.contains("--replace"));

    let output = cli_command()
        .args(["create-bntx", "--help"])
        .output()
        .unwrap();
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--name"));
    assert!(stdout.contains("--format"));
}

#[test]
fn adds_to_separate_output_and_replaces_in_place() {
    let workspace = TestDirectory::new("cli-add");
    let input = workspace.path().join("input.bntx");
    let source = workspace.path().join("source.png");
    let separate = workspace.path().join("separate.bntx");
    let original = synthetic_bntx();
    let original_bytes = original.to_bytes().unwrap();
    fs::write(&input, &original_bytes).unwrap();
    fs::write(&source, write_png(&test_image(11, 9, 7)).unwrap()).unwrap();

    assert_success(
        &cli_command()
            .arg("add-bntx-texture")
            .arg(&input)
            .arg(&source)
            .arg("templated")
            .arg(&separate)
            .args(["--like", "sRGB color"])
            .output()
            .unwrap(),
    );
    assert_eq!(fs::read(&input).unwrap(), original_bytes);
    let added = read_bntx(&separate);
    assert_eq!(
        &added.textures[..original.textures.len()],
        original.textures
    );
    assert_eq!(added.textures.last().unwrap().name, "templated");
    assert_eq!(
        added.textures.last().unwrap().info.format,
        original.textures[3].info.format
    );

    let before = added.clone();
    fs::write(&source, write_png(&test_image(8, 6, 9)).unwrap()).unwrap();
    assert_success(
        &cli_command()
            .arg("add-bntx-texture")
            .arg(&separate)
            .arg(&source)
            .arg("same/name")
            .arg(&separate)
            .arg("--replace")
            .output()
            .unwrap(),
    );
    let replaced = read_bntx(&separate);
    assert_eq!(replaced.textures.len(), before.textures.len());
    assert_eq!(&replaced.textures[1..], &before.textures[1..]);
    assert_eq!(replaced.textures[0].user_data, before.textures[0].user_data);
    let mut expected_info = before.textures[0].info.clone();
    expected_info.width = 8;
    expected_info.height = 6;
    assert!(expected_info.texture_layout.set_block_height_exponent(
        derive_block_height_exponent(expected_info.format, 8, 6).unwrap()
    ));
    assert_eq!(replaced.textures[0].info, expected_info);
    assert_ne!(
        replaced.textures[0].image_data,
        before.textures[0].image_data
    );
    assert_eq!(replaced.to_bytes().unwrap(), fs::read(separate).unwrap());
}

#[test]
fn add_format_and_default_select_the_requested_profiles() {
    let workspace = TestDirectory::new("cli-add-formats");
    let input = workspace.path().join("input.bntx");
    let source = workspace.path().join("source.png");
    let bc4_output = workspace.path().join("bc4.bntx");
    let default_output = workspace.path().join("default.bntx");
    fs::write(&input, synthetic_bntx().to_bytes().unwrap()).unwrap();
    fs::write(&source, write_png(&test_image(7, 5, 4)).unwrap()).unwrap();

    assert_success(
        &cli_command()
            .arg("add-bntx-texture")
            .arg(&input)
            .arg(&source)
            .arg("bc4")
            .arg(&bc4_output)
            .args(["--format", "bc4-unorm"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        read_bntx(&bc4_output)
            .textures
            .last()
            .unwrap()
            .info
            .format
            .to_raw(),
        BC4_UNORM
    );

    assert_success(
        &cli_command()
            .arg("add-bntx-texture")
            .arg(&input)
            .arg(&source)
            .arg("default")
            .arg(&default_output)
            .output()
            .unwrap(),
    );
    assert_eq!(
        read_bntx(&default_output)
            .textures
            .last()
            .unwrap()
            .info
            .format
            .to_raw(),
        BC7_UNORM
    );
}

#[test]
fn creates_one_or_many_textures_with_names_formats_and_sorted_order() {
    let workspace = TestDirectory::new("cli-create");
    let single_directory = workspace.path().join("single");
    let many_directory = workspace.path().join("many");
    fs::create_dir(&single_directory).unwrap();
    fs::create_dir(&many_directory).unwrap();
    fs::write(
        single_directory.join("only.png"),
        write_png(&test_image(5, 7, 1)).unwrap(),
    )
    .unwrap();
    fs::write(
        many_directory.join("zeta.png"),
        write_png(&test_image(9, 7, 2)).unwrap(),
    )
    .unwrap();
    fs::write(
        many_directory.join("alpha.png"),
        write_png(&test_image(7, 5, 3)).unwrap(),
    )
    .unwrap();

    let single_output = workspace.path().join("derived.bntx");
    assert_success(
        &cli_command()
            .arg("create-bntx")
            .arg(&single_directory)
            .arg(&single_output)
            .output()
            .unwrap(),
    );
    let single = read_bntx(&single_output);
    assert_eq!(single.name, "derived");
    assert_eq!(single.textures.len(), 1);
    assert_eq!(single.textures[0].name, "only");
    assert_eq!(single.textures[0].info.format.to_raw(), BC7_UNORM);

    let many_output = workspace.path().join("many.bntx");
    assert_success(
        &cli_command()
            .arg("create-bntx")
            .arg(&many_directory)
            .arg(&many_output)
            .args(["--name", "explicit", "--format", "bc5-unorm"])
            .output()
            .unwrap(),
    );
    let many = read_bntx(&many_output);
    assert_eq!(many.name, "explicit");
    assert_eq!(
        many.textures
            .iter()
            .map(|texture| texture.name.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "zeta"]
    );
    assert!(
        many.textures
            .iter()
            .all(|texture| texture.info.format.to_raw() == BC5_UNORM)
    );
    assert_eq!(many.to_bytes().unwrap(), fs::read(many_output).unwrap());
}

#[test]
fn add_and_create_failures_preserve_existing_destinations() {
    let workspace = TestDirectory::new("cli-create-failures");
    let input = workspace.path().join("input.bntx");
    let source = workspace.path().join("source.png");
    let output = workspace.path().join("output.bntx");
    fs::write(&input, synthetic_bntx().to_bytes().unwrap()).unwrap();
    fs::write(&source, write_png(&test_image(7, 5, 8)).unwrap()).unwrap();

    for arguments in [
        vec!["same/name", "--like", "sRGB color"],
        vec!["new", "--like", "missing"],
    ] {
        fs::write(&output, b"preserved").unwrap();
        let result = cli_command()
            .arg("add-bntx-texture")
            .arg(&input)
            .arg(&source)
            .arg(arguments[0])
            .arg(&output)
            .args(&arguments[1..])
            .output()
            .unwrap();
        assert_failure_contains(
            &result,
            if arguments[0] == "new" {
                "missing"
            } else {
                "already exists"
            },
        );
        assert_eq!(fs::read(&output).unwrap(), b"preserved");
    }

    fs::write(&source, b"not a PNG").unwrap();
    fs::write(&output, b"preserved").unwrap();
    let result = cli_command()
        .arg("add-bntx-texture")
        .arg(&input)
        .arg(&source)
        .arg("new")
        .arg(&output)
        .output()
        .unwrap();
    assert_failure_contains(&result, &source.display().to_string());
    assert_eq!(fs::read(&output).unwrap(), b"preserved");

    let not_directory = workspace.path().join("not-directory");
    fs::write(&not_directory, b"ordinary file").unwrap();
    let result = cli_command()
        .arg("create-bntx")
        .arg(&not_directory)
        .arg(&output)
        .output()
        .unwrap();
    assert_failure_contains(&result, "regular directory");
    assert_eq!(fs::read(&output).unwrap(), b"preserved");

    let result = cli_command()
        .arg("add-bntx-texture")
        .arg(&input)
        .arg(&source)
        .arg("new")
        .arg(&output)
        .args(["--like", "sRGB color", "--format", "bc7-unorm"])
        .output()
        .unwrap();
    assert_failure_contains(&result, "cannot be used with");
    assert_eq!(fs::read(output).unwrap(), b"preserved");
}

#[test]
fn create_rejects_symbolic_links_without_replacing_the_output() {
    let workspace = TestDirectory::new("cli-create-symlinks");
    let real_directory = workspace.path().join("real");
    let linked_directory = workspace.path().join("linked");
    let output = workspace.path().join("output.bntx");
    fs::create_dir(&real_directory).unwrap();
    fs::write(
        real_directory.join("texture.png"),
        write_png(&test_image(4, 4, 1)).unwrap(),
    )
    .unwrap();
    fs::write(&output, b"preserved").unwrap();

    if create_directory_symlink(&real_directory, &linked_directory).is_ok() {
        let result = cli_command()
            .arg("create-bntx")
            .arg(&linked_directory)
            .arg(&output)
            .output()
            .unwrap();
        assert_failure_contains(&result, "symbolic link");
        assert_eq!(fs::read(&output).unwrap(), b"preserved");
    }

    let linked_png = real_directory.join("linked.png");
    if create_file_symlink(&real_directory.join("texture.png"), &linked_png).is_ok() {
        let result = cli_command()
            .arg("create-bntx")
            .arg(&real_directory)
            .arg(&output)
            .output()
            .unwrap();
        assert_failure_contains(&result, "symbolic links");
        assert_eq!(fs::read(output).unwrap(), b"preserved");
    }
}

#[cfg(unix)]
#[test]
fn create_rejects_duplicate_stems_and_non_utf8_names() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let workspace = TestDirectory::new("cli-create-unix-names");
    let duplicate_directory = workspace.path().join("duplicates");
    let invalid_directory = workspace.path().join("invalid-utf8");
    let output = workspace.path().join("output.bntx");
    fs::create_dir(&duplicate_directory).unwrap();
    fs::create_dir(&invalid_directory).unwrap();
    let png = write_png(&test_image(4, 4, 1)).unwrap();
    fs::write(duplicate_directory.join("same.png"), &png).unwrap();
    fs::write(duplicate_directory.join("same.PNG"), &png).unwrap();
    fs::write(
        invalid_directory.join(OsString::from_vec(b"bad-\xff.png".to_vec())),
        &png,
    )
    .unwrap();
    fs::write(&output, b"preserved").unwrap();

    for (directory, expected) in [
        (&duplicate_directory, "duplicate texture name"),
        (&invalid_directory, "not valid UTF-8"),
    ] {
        let result = cli_command()
            .arg("create-bntx")
            .arg(directory)
            .arg(&output)
            .output()
            .unwrap();
        assert_failure_contains(&result, expected);
        assert_eq!(fs::read(&output).unwrap(), b"preserved");
    }

    let ignored_directory = workspace.path().join("ignored-non-png");
    let ignored_output = workspace.path().join("ignored-non-png.bntx");
    fs::create_dir(&ignored_directory).unwrap();
    fs::write(ignored_directory.join("valid.png"), &png).unwrap();
    fs::write(
        ignored_directory.join(OsString::from_vec(b"bad-\xff.txt".to_vec())),
        b"ignored",
    )
    .unwrap();
    assert_success(
        &cli_command()
            .arg("create-bntx")
            .arg(&ignored_directory)
            .arg(&ignored_output)
            .output()
            .unwrap(),
    );
    let bntx = read_bntx(&ignored_output);
    assert_eq!(bntx.textures.len(), 1);
    assert_eq!(bntx.textures[0].name, "valid");
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

fn read_bntx(path: &Path) -> Bntx {
    Bntx::from_bytes(&fs::read(path).unwrap()).unwrap()
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[cfg(unix)]
fn create_directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
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
