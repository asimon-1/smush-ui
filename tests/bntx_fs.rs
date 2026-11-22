mod common;

use common::{
    bntx::{BC4_UNORM, BC5_UNORM, BC7_SRGB, BC7_UNORM, channels_for, synthetic_bntx, test_image},
    fs::TestDirectory,
};
use smush_ui::bntx::{
    AddTextureOptions, Bntx, ChannelMode, ImageFormat, NewTextureFormat, RgbaImage,
    SurfaceDimension, TextureDimension, TextureProfile, TileMode, add_texture,
    derive_block_height_exponent,
    fs::{
        BundleManifest, PackOptions, UnpackOptions, add_texture_to_file, create_from_png_directory,
        pack_from_directory, read_bundle, unpack_to_directory,
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

#[test]
fn adds_from_templates_rejects_duplicates_and_replaces_in_place() {
    let mut bntx = synthetic_bntx();
    let original = bntx.clone();
    let image = test_image(11, 9, 7);
    add_texture(
        &mut bntx,
        "templated",
        &image,
        AddTextureOptions {
            profile: Some(TextureProfile::Like("sRGB color".into())),
            replace: false,
        },
    )
    .unwrap();

    assert_eq!(&bntx.textures[..original.textures.len()], original.textures);
    let template = &original.textures[3];
    let added = bntx.textures.last().unwrap();
    let mut expected_info = template.info.clone();
    expected_info.width = image.width;
    expected_info.height = image.height;
    assert!(expected_info.texture_layout.set_block_height_exponent(
        derive_block_height_exponent(expected_info.format, image.width, image.height).unwrap()
    ));
    assert_eq!(added.info, expected_info);
    assert_eq!(added.mip_offsets.len(), template.mip_offsets.len());
    assert!(added.user_data.is_empty());

    let before_duplicate = bntx.clone();
    assert!(add_texture(&mut bntx, "templated", &image, AddTextureOptions::default()).is_err());
    assert_eq!(bntx, before_duplicate);

    let replacement = test_image(8, 6, 9);
    add_texture(
        &mut bntx,
        "same/name",
        &replacement,
        AddTextureOptions {
            replace: true,
            ..AddTextureOptions::default()
        },
    )
    .unwrap();
    assert_eq!(bntx.textures.len(), original.textures.len() + 1);
    assert_eq!(bntx.textures[0].name, "same/name");
    assert_eq!(bntx.textures[0].user_data, original.textures[0].user_data);
    assert_eq!(&bntx.textures[1..4], &original.textures[1..]);
    assert_eq!(
        bntx.textures[0].info.format,
        original.textures[0].info.format
    );
    assert_eq!(
        (bntx.textures[0].info.width, bntx.textures[0].info.height),
        (8, 6)
    );

    let bytes = bntx.to_bytes().unwrap();
    let reparsed = Bntx::from_bytes(&bytes).unwrap();
    assert_eq!(reparsed, bntx);
    assert_eq!(reparsed.to_bytes().unwrap(), bytes);
}

#[test]
fn add_rejects_invalid_names_and_container_overflow_without_mutating() {
    let image = test_image(4, 4, 12);
    let mut bntx = synthetic_bntx();
    let original = bntx.clone();
    for name in [String::new(), "bad\0name".into(), "x".repeat(65_536)] {
        assert!(add_texture(&mut bntx, name, &image, AddTextureOptions::default()).is_err());
        assert_eq!(bntx, original);
    }

    let mut template = bntx.textures[0].clone();
    template.user_data.clear();
    template.mip_offsets = vec![0];
    template.image_data = vec![0];
    bntx.textures = (0..usize::from(u16::MAX))
        .map(|index| {
            let mut texture = template.clone();
            texture.name = format!("texture-{index}");
            texture
        })
        .collect();
    let last_name = bntx.textures.last().unwrap().name.clone();

    let error =
        add_texture(&mut bntx, "overflow", &image, AddTextureOptions::default()).unwrap_err();
    assert!(error.to_string().contains("texture count exceeds 65535"));
    assert_eq!(bntx.textures.len(), usize::from(u16::MAX));
    assert_eq!(bntx.textures.last().unwrap().name, last_name);

    add_texture(
        &mut bntx,
        "texture-0",
        &image,
        AddTextureOptions {
            replace: true,
            ..AddTextureOptions::default()
        },
    )
    .unwrap();
    assert_eq!(bntx.textures.len(), usize::from(u16::MAX));
    assert_eq!(bntx.textures[0].name, "texture-0");
}

#[test]
fn adds_each_explicit_format_with_corpus_defaults() {
    let mut bntx = synthetic_bntx();
    let original = bntx.clone();
    let image = test_image(9, 7, 11);
    for (index, (format, raw)) in [
        (NewTextureFormat::Bc4Unorm, BC4_UNORM),
        (NewTextureFormat::Bc5Unorm, BC5_UNORM),
        (NewTextureFormat::Bc7Unorm, BC7_UNORM),
        (NewTextureFormat::Bc7Srgb, BC7_SRGB),
    ]
    .into_iter()
    .enumerate()
    {
        add_texture(
            &mut bntx,
            format!("explicit-{index}"),
            &image,
            AddTextureOptions {
                profile: Some(TextureProfile::Format(format)),
                replace: false,
            },
        )
        .unwrap();
        let texture = bntx.textures.last().unwrap();
        assert_eq!(texture.info.flags, 1);
        assert_eq!(texture.info.image_dimension, TextureDimension::TwoD);
        assert_eq!(texture.info.tile_mode, TileMode::BlockLinear);
        assert_eq!(texture.info.swizzle, 0);
        assert_eq!(texture.info.sample_count, 1);
        assert_eq!(texture.info.format.to_raw(), raw);
        assert_eq!(texture.info.gpu_access, 0x20);
        assert_eq!((texture.info.width, texture.info.height), (9, 7));
        assert_eq!((texture.info.depth, texture.info.array_count), (1, 1));
        assert_eq!(texture.info.texture_layout2, 0x0001_0007);
        assert_eq!(texture.info.image_alignment, 0x200);
        assert_eq!(texture.info.channels, channels_for(raw));
        assert_eq!(texture.info.surface_dimension, SurfaceDimension::TwoD);
        assert_eq!(texture.mip_offsets, [0]);
        assert!(!texture.image_data.is_empty());
    }
    assert_eq!(&bntx.textures[..original.textures.len()], original.textures);
    let bytes = bntx.to_bytes().unwrap();
    assert_eq!(Bntx::from_bytes(&bytes).unwrap().to_bytes().unwrap(), bytes);
}

#[test]
fn creates_deterministic_sorted_containers_with_canonical_layout() {
    let workspace = TestDirectory::new("create");
    let first_directory = workspace.path().join("first");
    let second_directory = workspace.path().join("second");
    fs::create_dir(&first_directory).unwrap();
    fs::create_dir(&second_directory).unwrap();
    let alpha = write_png(&test_image(7, 5, 1)).unwrap();
    let zeta = write_png(&test_image(9, 7, 2)).unwrap();
    fs::write(first_directory.join("zeta.png"), &zeta).unwrap();
    fs::write(first_directory.join("alpha.png"), &alpha).unwrap();
    fs::write(second_directory.join("alpha.png"), &alpha).unwrap();
    fs::write(second_directory.join("zeta.png"), &zeta).unwrap();
    fs::write(first_directory.join("ignored.txt"), b"ignored").unwrap();

    let first_output = workspace.path().join("first.bntx");
    let second_output = workspace.path().join("second.bntx");
    create_from_png_directory(
        &first_directory,
        &first_output,
        Some("created"),
        NewTextureFormat::Bc7Unorm,
    )
    .unwrap();
    create_from_png_directory(
        &second_directory,
        &second_output,
        Some("created"),
        NewTextureFormat::Bc7Unorm,
    )
    .unwrap();
    let bytes = fs::read(&first_output).unwrap();
    assert_eq!(fs::read(second_output).unwrap(), bytes);

    let bntx = Bntx::from_bytes(&bytes).unwrap();
    assert_eq!(bntx.name, "created");
    assert_eq!(
        bntx.textures
            .iter()
            .map(|texture| texture.name.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "zeta"]
    );
    assert_eq!(bntx.to_bytes().unwrap(), bytes);
    assert_generated_layout(&bytes, 2);

    let default_name_output = workspace.path().join("derived-name.bntx");
    create_from_png_directory(
        &first_directory,
        &default_name_output,
        None,
        NewTextureFormat::Bc7Unorm,
    )
    .unwrap();
    assert_eq!(
        Bntx::from_bytes(&fs::read(default_name_output).unwrap())
            .unwrap()
            .name,
        "derived-name"
    );
}

#[test]
fn file_addition_is_atomic_and_preserves_the_input() {
    let workspace = TestDirectory::new("add-file");
    let input = workspace.path().join("input.bntx");
    let source = workspace.path().join("source.png");
    let output = workspace.path().join("output.bntx");
    let original = synthetic_bntx().to_bytes().unwrap();
    fs::write(&input, &original).unwrap();
    fs::write(&source, write_png(&test_image(8, 8, 4)).unwrap()).unwrap();

    add_texture_to_file(
        &input,
        &source,
        "added",
        &output,
        AddTextureOptions {
            profile: Some(TextureProfile::Like("color:*?".into())),
            replace: false,
        },
    )
    .unwrap();
    assert_eq!(fs::read(&input).unwrap(), original);
    let result = Bntx::from_bytes(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(result.textures.last().unwrap().name, "added");

    fs::write(&output, b"preserved").unwrap();
    assert!(
        add_texture_to_file(
            &input,
            &source,
            "other",
            &output,
            AddTextureOptions {
                profile: Some(TextureProfile::Like("missing".into())),
                replace: false,
            },
        )
        .is_err()
    );
    assert_eq!(fs::read(output).unwrap(), b"preserved");
}

fn assert_generated_layout(bytes: &[u8], texture_count: usize) {
    assert_eq!(&bytes[..8], b"BNTX\0\0\0\0");
    assert_eq!(&bytes[0x20..0x24], b"NX  ");
    assert_eq!(
        u32::from_le_bytes(bytes[0x24..0x28].try_into().unwrap()) as usize,
        texture_count
    );
    let brtd = u64::from_le_bytes(bytes[0x30..0x38].try_into().unwrap()) as usize;
    let relocation = u32::from_le_bytes(bytes[0x18..0x1c].try_into().unwrap()) as usize;
    assert_eq!(&bytes[brtd..brtd + 8], b"BRTD\0\0\0\0");
    assert_eq!((brtd + 0x10) % 0x1000, 0);
    assert_eq!(relocation % 0x1000, 0);
    assert_eq!(&bytes[relocation..relocation + 4], b"_RLT");
    assert_eq!(
        u32::from_le_bytes(bytes[relocation + 8..relocation + 12].try_into().unwrap()),
        2
    );
    let metadata_entries = u32::from_le_bytes(
        bytes[relocation + 0x24..relocation + 0x28]
            .try_into()
            .unwrap(),
    );
    let data_entries = u32::from_le_bytes(
        bytes[relocation + 0x3c..relocation + 0x40]
            .try_into()
            .unwrap(),
    );
    assert!(metadata_entries >= 5);
    assert!(data_entries >= 2);
}

fn read_manifest(bundle: &Path) -> BundleManifest {
    serde_jsonc2::from_slice(&fs::read(bundle.join("bntx.json")).unwrap()).unwrap()
}

fn write_manifest(bundle: &Path, manifest: &BundleManifest) {
    fs::write(
        bundle.join("bntx.json"),
        serde_jsonc2::to_vec_pretty(manifest).unwrap(),
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
