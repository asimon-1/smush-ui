mod common;

use common::{bntx::synthetic_bntx, fs::TestDirectory};
use smush_ui::bntx::{
    Bntx, ChannelMode, ImageFormat, RgbaImage,
    fs::{
        BundleManifest, PackOptions, UnpackOptions, pack_from_directory, read_bundle,
        unpack_to_directory,
    },
    read_png, write_png,
};
use std::{fs, path::Path};

type ManifestMutation = fn(&mut BundleManifest);

#[test]
fn unpacks_and_losslessly_repacks_a_self_contained_multi_format_bundle() {
    let workspace = TestDirectory::new("round-trip");
    let bundle = workspace.path().join("bundle");
    let output = workspace.path().join("repacked.bntx");
    let bntx = synthetic_bntx();
    let expected = bntx.to_bytes().unwrap();

    unpack_to_directory(&bntx, &bundle, UnpackOptions::default()).unwrap();
    let manifest = read_manifest(&bundle);
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.name, "synthetic_bundle");
    assert_eq!(manifest.preview_channels, ChannelMode::Resolved);
    assert_eq!(manifest.textures.len(), 4);
    assert_eq!(
        manifest
            .textures
            .iter()
            .map(|texture| texture.name.as_str())
            .collect::<Vec<_>>(),
        ["same/name", "same^name", "color:*?", "sRGB color"]
    );

    let mut sidecars = std::collections::HashSet::new();
    for (texture, entry) in bntx.textures.iter().zip(&manifest.textures) {
        assert!(entry.raw_path.starts_with("textures/"));
        assert!(entry.preview_path.starts_with("textures/"));
        assert!(sidecars.insert(entry.raw_path.to_ascii_lowercase()));
        assert!(sidecars.insert(entry.preview_path.to_ascii_lowercase()));
        assert_eq!(
            fs::read(bundle.join(&entry.raw_path)).unwrap(),
            texture.image_data
        );
        assert!(bundle.join(&entry.preview_path).is_file());
        assert!(entry.preview_hash.is_some());
    }

    pack_from_directory(&bundle, &output, PackOptions::default()).unwrap();
    assert_eq!(fs::read(output).unwrap(), expected);
}

#[test]
fn reencodes_only_changed_pngs_and_preserves_order_and_metadata() {
    let workspace = TestDirectory::new("png-edit");
    let bundle = workspace.path().join("bundle");
    let output = workspace.path().join("edited.bntx");
    let original = synthetic_bntx();
    unpack_to_directory(&original, &bundle, UnpackOptions::default()).unwrap();
    let manifest = read_manifest(&bundle);

    let edited_index = 2;
    let png_path = bundle.join(&manifest.textures[edited_index].preview_path);
    let mut image = read_png(&fs::read(&png_path).unwrap()).unwrap();
    for pixel in image.pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[12, 220, 44, 180]);
    }
    fs::write(&png_path, write_png(&image).unwrap()).unwrap();

    pack_from_directory(&bundle, &output, PackOptions { from_png: true }).unwrap();
    let edited = Bntx::from_bytes(&fs::read(output).unwrap()).unwrap();
    assert_eq!(edited.textures.len(), original.textures.len());
    for (index, (before, after)) in original.textures.iter().zip(&edited.textures).enumerate() {
        assert_eq!(after.name, before.name);
        assert_eq!(after.info, before.info);
        assert_eq!(after.user_data, before.user_data);
        assert_eq!(after.mip_offsets, before.mip_offsets);
        if index == edited_index {
            assert_ne!(after.image_data, before.image_data);
        } else {
            assert_eq!(after.image_data, before.image_data);
        }
    }
}

#[test]
fn rejects_unsafe_duplicate_and_missing_manifest_inputs() {
    let workspace = TestDirectory::new("invalid-manifest");
    let bntx = synthetic_bntx();

    let cases: [(&str, ManifestMutation); 6] = [
        ("traversal", |manifest: &mut BundleManifest| {
            manifest.textures[0].raw_path = "textures/../escape.bin".into();
        }),
        ("absolute", |manifest: &mut BundleManifest| {
            manifest.textures[0].raw_path = "C:/escape.bin".into();
        }),
        ("duplicate-path", |manifest: &mut BundleManifest| {
            manifest.textures[1].raw_path = manifest.textures[0].raw_path.clone();
        }),
        ("duplicate-name", |manifest: &mut BundleManifest| {
            manifest.textures[1].name = manifest.textures[0].name.clone();
        }),
        ("alternate-data-stream", |manifest: &mut BundleManifest| {
            manifest.textures[0].raw_path = "textures/data:stream.bin".into();
        }),
        ("windows-device", |manifest: &mut BundleManifest| {
            manifest.textures[0].raw_path = "textures/CON.bin".into();
        }),
    ];
    for (label, mutate) in cases {
        let bundle = workspace.path().join(label);
        unpack_to_directory(&bntx, &bundle, UnpackOptions::default()).unwrap();
        let mut manifest = read_manifest(&bundle);
        mutate(&mut manifest);
        write_manifest(&bundle, &manifest);
        assert!(
            read_bundle(&bundle, PackOptions::default()).is_err(),
            "{label}"
        );
    }

    let missing = workspace.path().join("missing");
    unpack_to_directory(&bntx, &missing, UnpackOptions::default()).unwrap();
    let manifest = read_manifest(&missing);
    fs::remove_file(missing.join(&manifest.textures[0].raw_path)).unwrap();
    assert!(read_bundle(&missing, PackOptions::default()).is_err());

    let missing_preview = workspace.path().join("missing-preview");
    unpack_to_directory(&bntx, &missing_preview, UnpackOptions::default()).unwrap();
    let manifest = read_manifest(&missing_preview);
    fs::remove_file(missing_preview.join(&manifest.textures[0].preview_path)).unwrap();
    assert!(read_bundle(&missing_preview, PackOptions { from_png: true }).is_err());

    let no_preview = workspace.path().join("no-preview");
    unpack_to_directory(
        &bntx,
        &no_preview,
        UnpackOptions {
            write_previews: false,
            ..UnpackOptions::default()
        },
    )
    .unwrap();
    assert!(read_bundle(&no_preview, PackOptions { from_png: true }).is_err());
}

#[test]
fn rejects_output_conflicts_bad_pngs_unsupported_encoders_and_symlinks() {
    let workspace = TestDirectory::new("invalid-files");
    let bntx = synthetic_bntx();

    let conflict = workspace.path().join("conflict");
    fs::create_dir_all(conflict.join("textures")).unwrap();
    fs::write(conflict.join("textures/0000-same_name.bin"), b"existing").unwrap();
    assert!(unpack_to_directory(&bntx, &conflict, UnpackOptions::default()).is_err());
    assert!(!conflict.join("bntx.json").exists());
    assert_eq!(
        fs::read(conflict.join("textures/0000-same_name.bin")).unwrap(),
        b"existing"
    );

    let wrong_dimensions = workspace.path().join("wrong-dimensions");
    unpack_to_directory(&bntx, &wrong_dimensions, UnpackOptions::default()).unwrap();
    let manifest = read_manifest(&wrong_dimensions);
    let png_path = wrong_dimensions.join(&manifest.textures[0].preview_path);
    fs::write(
        png_path,
        write_png(&RgbaImage::new(1, 1, vec![0; 4]).unwrap()).unwrap(),
    )
    .unwrap();
    let output = workspace.path().join("preserved.bntx");
    fs::write(&output, b"preserved").unwrap();
    assert!(
        pack_from_directory(&wrong_dimensions, &output, PackOptions { from_png: true }).is_err()
    );
    assert_eq!(fs::read(&output).unwrap(), b"preserved");

    let unsupported = workspace.path().join("unsupported");
    let mut unsupported_bntx = bntx.clone();
    unsupported_bntx.textures[0].info.format = ImageFormat::from_raw(0xdead_beef);
    unpack_to_directory(&unsupported_bntx, &unsupported, UnpackOptions::default()).unwrap();
    let manifest = read_manifest(&unsupported);
    assert!(manifest.textures[0].preview_hash.is_none());
    assert!(
        !unsupported
            .join(&manifest.textures[0].preview_path)
            .exists()
    );
    assert!(read_bundle(&unsupported, PackOptions { from_png: true }).is_err());

    let no_preview_conflict = workspace.path().join("no-preview-conflict");
    let existing_preview = no_preview_conflict.join("textures/0000-same_name.png");
    fs::create_dir_all(existing_preview.parent().unwrap()).unwrap();
    fs::write(&existing_preview, b"existing preview").unwrap();
    unpack_to_directory(
        &bntx,
        &no_preview_conflict,
        UnpackOptions {
            write_previews: false,
            ..UnpackOptions::default()
        },
    )
    .unwrap();
    assert_eq!(fs::read(existing_preview).unwrap(), b"existing preview");

    let symlink = workspace.path().join("symlink");
    unpack_to_directory(&bntx, &symlink, UnpackOptions::default()).unwrap();
    let link = symlink.join("unexpected-link");
    if create_file_symlink(&symlink.join("bntx.json"), &link).is_ok() {
        assert!(read_bundle(&symlink, PackOptions::default()).is_err());
    }

    let real_ancestor = workspace.path().join("real-ancestor");
    let linked_ancestor = workspace.path().join("linked-ancestor");
    fs::create_dir(&real_ancestor).unwrap();
    if create_directory_symlink(&real_ancestor, &linked_ancestor).is_ok() {
        assert!(
            unpack_to_directory(
                &bntx,
                linked_ancestor.join("bundle"),
                UnpackOptions::default()
            )
            .is_err()
        );
        assert!(!real_ancestor.join("bundle").exists());
    }
}

fn read_manifest(bundle: &Path) -> BundleManifest {
    serde_json::from_slice(&fs::read(bundle.join("bntx.json")).unwrap()).unwrap()
}

fn write_manifest(bundle: &Path, manifest: &BundleManifest) {
    fs::write(
        bundle.join("bntx.json"),
        serde_json::to_vec_pretty(manifest).unwrap(),
    )
    .unwrap();
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
