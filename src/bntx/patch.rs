use std::{
    collections::{HashMap, HashSet},
    fs, io,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use super::{
    Bntx, ChannelMode, ImageFormat, NewTextureFormat, Texture, TextureInfo, TextureProfile,
    UserData,
    creation::build_texture,
    decode_texture,
    fs::{
        manifest_sidecar_path, read_regular_file, sidecar_stem, validate_bundle_root,
        write_bntx_atomic,
    },
    read_png, supports_image_format, validation, write_png,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BntxPatchManifest {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upsert: Vec<BntxUpsertEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<BntxRemoveEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_texture_order: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "source_format", rename_all = "lowercase", deny_unknown_fields)]
pub enum BntxUpsertEntry {
    Png {
        name: String,
        source: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        like: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<NewTextureFormat>,
    },
    Bin {
        name: String,
        source: String,
        info: TextureInfo,
        mip_offsets: Vec<u64>,
        user_data: Vec<UserData>,
    },
}

impl BntxUpsertEntry {
    fn name(&self) -> &str {
        match self {
            Self::Png { name, .. } | Self::Bin { name, .. } => name,
        }
    }
    fn source(&self) -> &str {
        match self {
            Self::Png { source, .. } | Self::Bin { source, .. } => source,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BntxRemoveEntry {
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PatchSourceFormat {
    Bin,
    #[default]
    Png,
}

#[derive(Debug, Default)]
pub struct CreatePatchSummary {
    pub upserts: usize,
    pub removals: usize,
    pub png_fallbacks: Vec<String>,
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

pub fn create_bntx_patch(
    base: impl AsRef<Path>,
    modified: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    source_format: PatchSourceFormat,
) -> io::Result<CreatePatchSummary> {
    let base = read_bntx(base.as_ref())?;
    let modified = read_bntx(modified.as_ref())?;
    validation::validate(&base)
        .map_err(|error| invalid_data(format!("invalid base BNTX: {error}")))?;
    validation::validate(&modified)
        .map_err(|error| invalid_data(format!("invalid modified BNTX: {error}")))?;
    if base.version != modified.version
        || base.alignment_exponent != modified.alignment_exponent
        || base.target_address_size != modified.target_address_size
        || base.flag != modified.flag
        || base.name != modified.name
        || base.platform != modified.platform
    {
        return Err(invalid_data(
            "BNTX container metadata differs; schema version 1 cannot represent it",
        ));
    }
    let destination = destination.as_ref();
    preflight_destination(destination)?;
    let original: HashMap<_, _> = base
        .textures
        .iter()
        .map(|texture| (texture.name.as_str(), texture))
        .collect();
    let final_names: HashSet<_> = modified
        .textures
        .iter()
        .map(|texture| texture.name.as_str())
        .collect();
    let removed_names: Vec<_> = base
        .textures
        .iter()
        .filter(|texture| !final_names.contains(texture.name.as_str()))
        .map(|texture| texture.name.clone())
        .collect();
    let mut manifest = BntxPatchManifest {
        schema_version: 1,
        upsert: Vec::new(),
        remove: Vec::new(),
        final_texture_order: None,
    };
    let mut sidecars = Vec::new();
    let mut summary = CreatePatchSummary::default();
    for (index, texture) in modified.textures.iter().enumerate() {
        if original
            .get(texture.name.as_str())
            .is_some_and(|old| *old == texture)
        {
            continue;
        }
        let stem = sidecar_stem(index, &texture.name);
        let png = if source_format == PatchSourceFormat::Png {
            png_source(&base, texture)
        } else {
            None
        };
        if let Some((png, profile)) = png {
            let source = format!("textures/{stem}.png");
            sidecars.push((source.clone(), png));
            let (like, format) = match profile {
                Some(TextureProfile::Like(name)) => (Some(name), None),
                Some(TextureProfile::Format(format)) => (None, Some(format)),
                None => (None, None),
            };
            manifest.upsert.push(BntxUpsertEntry::Png {
                name: texture.name.clone(),
                source,
                like,
                format,
            });
        } else {
            if source_format == PatchSourceFormat::Png {
                summary.png_fallbacks.push(texture.name.clone());
            }
            let source = format!("textures/{stem}.bin");
            sidecars.push((source.clone(), texture.image_data.clone()));
            manifest.upsert.push(BntxUpsertEntry::Bin {
                name: texture.name.clone(),
                source,
                info: texture.info.clone(),
                mip_offsets: texture.mip_offsets.clone(),
                user_data: texture.user_data.clone(),
            });
        }
    }
    manifest.remove = removed_names
        .iter()
        .map(|name| BntxRemoveEntry { name: name.clone() })
        .collect();
    let mut projected: Vec<_> = base
        .textures
        .iter()
        .filter(|texture| final_names.contains(texture.name.as_str()))
        .map(|texture| texture.name.clone())
        .collect();
    for entry in &manifest.upsert {
        if !projected.iter().any(|name| name == entry.name()) {
            projected.push(entry.name().to_owned());
        }
    }
    let intended: Vec<_> = modified
        .textures
        .iter()
        .map(|texture| texture.name.clone())
        .collect();
    if projected != intended {
        manifest.final_texture_order = Some(intended);
    }
    summary.upserts = manifest.upsert.len();
    summary.removals = manifest.remove.len();
    let mut json = serde_jsonc2::to_vec_pretty(&manifest)
        .map_err(|error| invalid_data(format!("serialize BNTX patch manifest: {error}")))?;
    json.push(b'\n');
    sidecars.push(("bntx-patch.json".into(), json));
    for (path, _) in &sidecars {
        if path != "bntx-patch.json" {
            manifest_sidecar_path(path, if path.ends_with(".png") { "png" } else { "bin" })?;
        }
    }
    let staged = create_staging_directory(destination)?;
    let written = (|| -> io::Result<()> {
        for (relative, data) in sidecars {
            let path = staged.join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(&data)?;
            file.sync_all()?;
        }
        preflight_destination(destination)?;
        fs::rename(&staged, destination)?;
        Ok(())
    })();
    if written.is_err() {
        let _ = fs::remove_dir_all(&staged);
    }
    written?;
    Ok(summary)
}

fn read_bntx(path: &Path) -> io::Result<Bntx> {
    let bytes = read_regular_file(path, "BNTX input")?;
    Bntx::from_bytes(&bytes).map_err(|error| invalid_data(format!("{}: {error}", path.display())))
}

fn new_format(format: ImageFormat) -> Option<NewTextureFormat> {
    [
        NewTextureFormat::Bc1Unorm,
        NewTextureFormat::Bc1Srgb,
        NewTextureFormat::Bc4Unorm,
        NewTextureFormat::Bc5Unorm,
        NewTextureFormat::Bc7Unorm,
        NewTextureFormat::Bc7Srgb,
    ]
    .into_iter()
    .find(|candidate| candidate.image_format() == format)
}

fn png_source(base: &Bntx, texture: &Texture) -> Option<(Vec<u8>, Option<TextureProfile>)> {
    if !supports_image_format(texture.info.format) {
        return None;
    }
    let image = decode_texture(texture, 0, ChannelMode::Resolved).ok()?;
    let mut profiles = Vec::new();
    if base.textures.iter().any(|old| old.name == texture.name) {
        profiles.push(None);
    }
    if let Some(format) = new_format(texture.info.format) {
        profiles.push(Some(TextureProfile::Format(format)));
    }
    profiles.extend(
        base.textures
            .iter()
            .filter(|old| old.info.format == texture.info.format)
            .map(|old| Some(TextureProfile::Like(old.name.clone()))),
    );
    for profile in profiles {
        let Ok(candidate) = build_texture(base, &texture.name, &image, profile.as_ref()) else {
            continue;
        };
        if candidate.info == texture.info
            && candidate.user_data == texture.user_data
            && candidate.mip_offsets == texture.mip_offsets
        {
            return Some((write_png(&image).ok()?, profile));
        }
    }
    None
}

fn preflight_destination(destination: &Path) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("patch destination {} already exists", destination.display()),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut parent = destination.parent();
    while let Some(path) = parent {
        if path.as_os_str().is_empty() {
            break;
        }
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", path.display()))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(invalid_input(format!(
                "{} is not a regular directory",
                path.display()
            )));
        }
        parent = path.parent();
    }
    Ok(())
}

fn create_staging_directory(destination: &Path) -> io::Result<PathBuf> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = destination
        .file_name()
        .ok_or_else(|| invalid_input("patch destination needs a directory name"))?
        .to_string_lossy();
    for _ in 0..100 {
        let next = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{name}.tmp-{}-{next}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "cannot allocate BNTX patch staging directory",
    ))
}

pub fn patch_bntx_file(
    input: impl AsRef<Path>,
    patch_directory: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> io::Result<()> {
    let input = input.as_ref();
    let patch_directory = patch_directory.as_ref();
    let output = output.as_ref();
    let bytes = fs::read(input)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", input.display())))?;
    let original = Bntx::from_bytes(&bytes)
        .map_err(|error| invalid_data(format!("{}: {error}", input.display())))?;
    let result = apply_bntx_patch(&original, patch_directory)?;
    write_bntx_atomic(&result, output)
}

pub(crate) fn apply_bntx_patch(original: &Bntx, patch_directory: &Path) -> io::Result<Bntx> {
    validate_bundle_root(patch_directory)?;
    let mut json = false;
    let mut jsonc = false;
    for entry in fs::read_dir(patch_directory)? {
        let name = entry?.file_name();
        if name == "bntx-patch.json" {
            json = true;
        } else if name == "bntx-patch.jsonc" {
            jsonc = true;
        } else if name
            .to_string_lossy()
            .eq_ignore_ascii_case("bntx-patch.json")
            || name
                .to_string_lossy()
                .eq_ignore_ascii_case("bntx-patch.jsonc")
        {
            return Err(invalid_input(format!(
                "unsupported BNTX patch manifest filename {name:?}"
            )));
        }
    }
    if json && jsonc {
        return Err(invalid_input(
            "both BNTX patch manifest filenames are present",
        ));
    }
    let manifest_path = if json {
        patch_directory.join("bntx-patch.json")
    } else if jsonc {
        patch_directory.join("bntx-patch.jsonc")
    } else {
        return Err(invalid_input(format!(
            "{} contains no BNTX patch manifest",
            patch_directory.display()
        )));
    };
    let manifest_bytes = read_regular_file(&manifest_path, "BNTX patch manifest")?;
    let manifest: BntxPatchManifest = serde_jsonc2::from_slice(&manifest_bytes)
        .map_err(|error| invalid_data(format!("{}: {error}", manifest_path.display())))?;
    if manifest.schema_version != 1 {
        return Err(invalid_data(format!(
            "{}: unsupported BNTX patch schema version {}",
            manifest_path.display(),
            manifest.schema_version
        )));
    }

    let mut targets = HashSet::new();
    for (index, entry) in manifest.upsert.iter().enumerate() {
        let name = entry.name();
        let context = format!("{} upsert[{index}] {name:?}", manifest_path.display());
        validation::validate_texture_name(name)
            .map_err(|error| invalid_data(format!("{context}: {error}")))?;
        if !targets.insert(name) {
            return Err(invalid_data(format!("{context}: duplicate texture target")));
        }
        let extension = if matches!(entry, BntxUpsertEntry::Png { .. }) {
            "png"
        } else {
            "bin"
        };
        manifest_sidecar_path(entry.source(), extension)
            .map_err(|error| invalid_data(format!("{context}: {error}")))?;
        if let BntxUpsertEntry::Png { like, format, .. } = entry
            && like.is_some()
            && format.is_some()
        {
            return Err(invalid_data(format!("{context}: like and format conflict")));
        }
    }
    for (index, entry) in manifest.remove.iter().enumerate() {
        let context = format!(
            "{} remove[{index}] {:?}",
            manifest_path.display(),
            entry.name
        );
        validation::validate_texture_name(&entry.name)
            .map_err(|error| invalid_data(format!("{context}: {error}")))?;
        if !targets.insert(entry.name.as_str()) {
            return Err(invalid_data(format!("{context}: duplicate texture target")));
        }
        if !original
            .textures
            .iter()
            .any(|texture| texture.name == entry.name)
        {
            return Err(invalid_data(format!("{context}: texture does not exist")));
        }
    }

    let mut prepared = Vec::with_capacity(manifest.upsert.len());
    for (index, entry) in manifest.upsert.iter().enumerate() {
        let name = entry.name();
        let context = format!("{} upsert[{index}] {name:?}", manifest_path.display());
        let extension = if matches!(entry, BntxUpsertEntry::Png { .. }) {
            "png"
        } else {
            "bin"
        };
        let relative = manifest_sidecar_path(entry.source(), extension)
            .map_err(|error| invalid_data(format!("{context}: {error}")))?;
        let path = patch_directory.join(relative);
        let bytes = read_regular_file(&path, "BNTX patch sidecar")
            .map_err(|error| io::Error::new(error.kind(), format!("{context}: {error}")))?;
        let texture = match entry {
            BntxUpsertEntry::Png { like, format, .. } => {
                let image = read_png(&bytes).map_err(|error| {
                    invalid_data(format!("{context} {}: {error}", path.display()))
                })?;
                let profile = like
                    .as_ref()
                    .map(|name| TextureProfile::Like(name.clone()))
                    .or_else(|| format.map(TextureProfile::Format));
                build_texture(original, name, &image, profile.as_ref())
                    .map_err(|error| invalid_data(format!("{context}: {error}")))?
            }
            BntxUpsertEntry::Bin {
                info,
                mip_offsets,
                user_data,
                ..
            } => Texture {
                name: name.to_owned(),
                info: info.clone(),
                mip_offsets: mip_offsets.clone(),
                user_data: user_data.clone(),
                image_data: bytes,
            },
        };
        validation::validate_texture(&texture)
            .map_err(|error| invalid_data(format!("{context}: {error}")))?;
        prepared.push(texture);
    }
    let mut result = original.clone();
    for texture in prepared {
        let name = texture.name.as_str();
        if let Some(position) = result.textures.iter().position(|item| item.name == name) {
            result.textures[position] = texture;
        } else {
            result.textures.push(texture);
        }
    }
    result.textures.retain(|texture| {
        !manifest
            .remove
            .iter()
            .any(|entry| entry.name == texture.name)
    });
    if let Some(order) = &manifest.final_texture_order {
        if order.len() != result.textures.len()
            || order.iter().collect::<HashSet<_>>().len() != order.len()
        {
            return Err(invalid_data(format!(
                "{}: final_texture_order must name every final texture exactly once",
                manifest_path.display()
            )));
        }
        let mut by_name: HashMap<_, _> = result
            .textures
            .into_iter()
            .map(|texture| (texture.name.clone(), texture))
            .collect();
        result.textures = order
            .iter()
            .map(|name| {
                by_name.remove(name).ok_or_else(|| {
                    invalid_data(format!(
                        "{}: final_texture_order contains unknown texture {name:?}",
                        manifest_path.display()
                    ))
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
    }
    validation::validate(&result)
        .map_err(|error| invalid_data(format!("{}: {error}", manifest_path.display())))?;
    Ok(result)
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
