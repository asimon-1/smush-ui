use super::{
    AddTextureOptions, Bntx, ChannelMode, NewTextureFormat, Texture, TextureInfo, UserData,
    Version, add_texture, create_bntx, decode_texture, encode_texture,
    image::validate_texture_profile, read_png, supports_image_format, validation, write_png,
};
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

const MANIFEST_NAME: &str = "bntx.json";
const SCHEMA_VERSION: u32 = 1;

/// Options controlling creation of an editable BNTX bundle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnpackOptions {
    pub write_previews: bool,
    pub channel_mode: ChannelMode,
}

impl Default for UnpackOptions {
    fn default() -> Self {
        Self {
            write_previews: true,
            channel_mode: ChannelMode::Resolved,
        }
    }
}

/// Options controlling reconstruction of a BNTX bundle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PackOptions {
    pub from_png: bool,
}

/// The versioned metadata stored in `bntx.json`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BundleManifest {
    pub schema_version: u32,
    pub version: Version,
    pub alignment_exponent: u8,
    pub target_address_size: u8,
    pub flag: u16,
    pub name: String,
    pub platform: super::Platform,
    pub preview_channels: ChannelMode,
    pub source_string_order: Vec<String>,
    pub textures: Vec<BundleTexture>,
}

/// One texture's semantic metadata and ordered bundle sidecars.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BundleTexture {
    pub name: String,
    pub info: TextureInfo,
    pub mip_offsets: Vec<u64>,
    pub user_data: Vec<UserData>,
    pub raw_path: String,
    pub preview_path: String,
    pub preview_hash: Option<String>,
}

struct OutputFile {
    relative_path: PathBuf,
    data: Vec<u8>,
}

/// Writes a self-contained bundle without overwriting existing bundle files.
pub fn unpack_to_directory(
    bntx: &Bntx,
    destination: impl AsRef<Path>,
    options: UnpackOptions,
) -> io::Result<()> {
    validation::validate(bntx)
        .map_err(|error| invalid_data(format!("invalid BNTX model: {error}")))?;

    let destination = destination.as_ref();
    let mut manifest = BundleManifest::from_bntx(bntx, options.channel_mode);
    let mut outputs = Vec::with_capacity(1 + bntx.textures.len() * 2);

    for (texture, entry) in bntx.textures.iter().zip(&mut manifest.textures) {
        let raw_path = manifest_sidecar_path(&entry.raw_path, "bin")?;
        outputs.push(OutputFile {
            relative_path: raw_path,
            data: texture.image_data.clone(),
        });
        if options.write_previews && supports_image_format(texture.info.format) {
            let preview_path = manifest_sidecar_path(&entry.preview_path, "png")?;
            let image = decode_texture(texture, 0, options.channel_mode).map_err(|error| {
                invalid_data(format!(
                    "failed to decode texture {:?}: {error}",
                    texture.name
                ))
            })?;
            let png = write_png(&image).map_err(|error| {
                invalid_data(format!(
                    "failed to encode preview for {:?}: {error}",
                    texture.name
                ))
            })?;
            entry.preview_hash = Some(hash_bytes(&png));
            outputs.push(OutputFile {
                relative_path: preview_path,
                data: png,
            });
        }
    }

    let mut manifest_bytes = serde_jsonc2::to_vec_pretty(&manifest)
        .map_err(|error| invalid_data(format!("failed to serialize BNTX manifest: {error}")))?;
    manifest_bytes.push(b'\n');
    outputs.push(OutputFile {
        relative_path: PathBuf::from(MANIFEST_NAME),
        data: manifest_bytes,
    });

    validate_output_paths(
        destination,
        outputs.iter().map(|output| &output.relative_path),
    )?;
    fs::create_dir_all(destination)
        .map_err(|error| path_error("create bundle directory", destination, error))?;
    for output in outputs {
        let path = destination.join(&output.relative_path);
        let parent = path
            .parent()
            .ok_or_else(|| invalid_input("bundle output has no parent"))?;
        fs::create_dir_all(parent)
            .map_err(|error| path_error("create bundle subdirectory", parent, error))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| path_error("create bundle file", &path, error))?;
        file.write_all(&output.data)
            .map_err(|error| path_error("write bundle file", &path, error))?;
    }
    Ok(())
}

/// Reads and validates a bundle, optionally replacing textures with changed PNG previews.
pub fn read_bundle(directory: impl AsRef<Path>, options: PackOptions) -> io::Result<Bntx> {
    let directory = directory.as_ref();
    validate_bundle_root(directory)?;
    let manifest_path = directory.join(MANIFEST_NAME);
    let manifest_bytes = read_regular_file(&manifest_path, "manifest")?;
    let manifest: BundleManifest = serde_jsonc2::from_slice(&manifest_bytes)
        .map_err(|error| invalid_data(format!("invalid BNTX manifest: {error}")))?;
    let paths = manifest.validate()?;

    let mut textures = Vec::with_capacity(manifest.textures.len());
    for (entry, (raw_path, preview_path)) in manifest.textures.iter().zip(paths) {
        let image_data = read_regular_file(&directory.join(raw_path), "raw texture sidecar")?;
        let mut texture = Texture {
            name: entry.name.clone(),
            info: entry.info.clone(),
            mip_offsets: entry.mip_offsets.clone(),
            user_data: entry.user_data.clone(),
            image_data,
        };
        if options.from_png {
            validate_texture_profile(&texture.info).map_err(|error| {
                invalid_data(format!(
                    "cannot encode PNG for texture {:?}: {error}",
                    entry.name
                ))
            })?;
            let preview_path = directory.join(preview_path);
            let png = read_regular_file(&preview_path, "PNG texture sidecar")?;
            let current_hash = hash_bytes(&png);
            if entry.preview_hash.as_deref() == Some(current_hash.as_str()) {
                textures.push(texture);
                continue;
            }
            let image = read_png(&png).map_err(|error| {
                invalid_data(format!("invalid PNG for texture {:?}: {error}", entry.name))
            })?;
            let encoded =
                encode_texture(&texture, &image, manifest.preview_channels).map_err(|error| {
                    invalid_data(format!(
                        "failed to encode PNG for texture {:?}: {error}",
                        entry.name
                    ))
                })?;
            texture.image_data = encoded.image_data;
            texture.mip_offsets = encoded.mip_offsets;
            if !texture
                .info
                .texture_layout
                .set_block_height_exponent(encoded.block_height_exponent)
            {
                return Err(invalid_data(format!(
                    "invalid encoded block height for texture {:?}",
                    entry.name
                )));
            }
        }
        textures.push(texture);
    }

    let bntx = Bntx {
        version: manifest.version,
        alignment_exponent: manifest.alignment_exponent,
        target_address_size: manifest.target_address_size,
        flag: manifest.flag,
        name: manifest.name,
        platform: manifest.platform,
        textures,
        source_string_order: manifest.source_string_order,
    };
    validation::validate(&bntx)
        .map_err(|error| invalid_data(format!("invalid reconstructed BNTX: {error}")))?;
    Ok(bntx)
}

/// Packs a bundle to an atomic BNTX output file.
pub fn pack_from_directory(
    directory: impl AsRef<Path>,
    output: impl AsRef<Path>,
    options: PackOptions,
) -> io::Result<()> {
    let output = output.as_ref();
    let bntx = read_bundle(directory, options)?;
    write_bntx_atomic(&bntx, output)
}

/// Adds or replaces one PNG texture and atomically writes the resulting BNTX.
pub fn add_texture_to_file(
    input: impl AsRef<Path>,
    source_png: impl AsRef<Path>,
    texture_name: impl Into<String>,
    output: impl AsRef<Path>,
    options: AddTextureOptions,
) -> io::Result<()> {
    let input = input.as_ref();
    let source_png = source_png.as_ref();
    let output = output.as_ref();
    let bytes = fs::read(input).map_err(|error| path_error("read BNTX input", input, error))?;
    let mut bntx = Bntx::from_bytes(&bytes).map_err(|error| {
        invalid_data(format!(
            "failed to parse BNTX input {}: {error}",
            input.display()
        ))
    })?;
    let png = read_regular_file(source_png, "source PNG")?;
    let image = read_png(&png).map_err(|error| {
        invalid_data(format!(
            "invalid source PNG {}: {error}",
            source_png.display()
        ))
    })?;
    add_texture(&mut bntx, texture_name, &image, options)
        .map_err(|error| invalid_data(format!("failed to add BNTX texture: {error}")))?;
    write_bntx_atomic(&bntx, output)
}

/// Creates a canonical BNTX from the PNG files directly inside a directory.
pub fn create_from_png_directory(
    input_directory: impl AsRef<Path>,
    output: impl AsRef<Path>,
    name: Option<&str>,
    format: NewTextureFormat,
) -> io::Result<()> {
    let input_directory = input_directory.as_ref();
    let output = output.as_ref();
    let images = read_png_directory(input_directory)?;
    if images.is_empty() {
        return Err(invalid_input(format!(
            "PNG input directory {} contains no PNG files",
            input_directory.display()
        )));
    }
    let name = match name {
        Some(name) => name.to_owned(),
        None => output
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                invalid_input(format!(
                    "BNTX output {} has no valid UTF-8 file stem for the container name",
                    output.display()
                ))
            })?
            .to_owned(),
    };
    let bntx = create_bntx(name, images, format)
        .map_err(|error| invalid_data(format!("failed to create BNTX: {error}")))?;
    write_bntx_atomic(&bntx, output)
}

pub(super) fn write_bntx_atomic(bntx: &Bntx, output: &Path) -> io::Result<()> {
    let bytes = bntx
        .to_bytes()
        .map_err(|error| invalid_data(format!("failed to serialize BNTX: {error}")))?;
    let mut file = AtomicWriteFile::options()
        .open(output)
        .map_err(|error| path_error("create atomic BNTX output", output, error))?;
    file.write_all(&bytes)
        .map_err(|error| path_error("write atomic BNTX output", output, error))?;
    file.commit()
        .map_err(|error| path_error("replace BNTX output", output, error))
}

fn read_png_directory(directory: &Path) -> io::Result<Vec<(String, super::RgbaImage)>> {
    let metadata = fs::symlink_metadata(directory)
        .map_err(|error| path_error("inspect PNG input directory", directory, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid_input(format!(
            "PNG input {} must be a regular directory, not a symbolic link",
            directory.display()
        )));
    }

    let entries = fs::read_dir(directory)
        .map_err(|error| path_error("read PNG input directory", directory, error))?;
    let mut pngs = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| path_error("read PNG input directory entry", directory, error))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| path_error("inspect PNG input entry", &path, error))?;
        if file_type.is_symlink() {
            return Err(invalid_input(format!(
                "symbolic links are not allowed in PNG input directories: {}",
                path.display()
            )));
        }
        if file_type.is_dir() {
            continue;
        }
        if !file_type.is_file() {
            return Err(invalid_input(format!(
                "PNG input entry {} is not a regular file",
                path.display()
            )));
        }
        let file_name = entry.file_name();
        let file_path = Path::new(&file_name);
        if !file_path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("png"))
        {
            continue;
        }
        let file_name = file_name.into_string().map_err(|_| {
            invalid_input(format!(
                "PNG input entry {} is not valid UTF-8",
                path.display()
            ))
        })?;
        let file_path = Path::new(&file_name);
        let stem = file_path
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                invalid_input(format!(
                    "PNG input {} has no valid file stem",
                    path.display()
                ))
            })?
            .to_owned();
        pngs.push((file_name, stem, path));
    }
    pngs.sort_by(|left, right| left.0.cmp(&right.0));

    let mut names = HashSet::with_capacity(pngs.len());
    let mut images = Vec::with_capacity(pngs.len());
    for (_, name, path) in pngs {
        if !names.insert(name.clone()) {
            return Err(invalid_input(format!(
                "duplicate PNG texture name {name:?} in {}",
                directory.display()
            )));
        }
        let bytes = read_regular_file(&path, "PNG input")?;
        let image = read_png(&bytes).map_err(|error| {
            invalid_data(format!("invalid PNG input {}: {error}", path.display()))
        })?;
        images.push((name, image));
    }
    Ok(images)
}

impl BundleManifest {
    fn from_bntx(bntx: &Bntx, preview_channels: ChannelMode) -> Self {
        let textures = bntx
            .textures
            .iter()
            .enumerate()
            .map(|(index, texture)| {
                let stem = sidecar_stem(index, &texture.name);
                BundleTexture {
                    name: texture.name.clone(),
                    info: texture.info.clone(),
                    mip_offsets: texture.mip_offsets.clone(),
                    user_data: texture.user_data.clone(),
                    raw_path: format!("textures/{stem}.bin"),
                    preview_path: format!("textures/{stem}.png"),
                    preview_hash: None,
                }
            })
            .collect();
        Self {
            schema_version: SCHEMA_VERSION,
            version: bntx.version,
            alignment_exponent: bntx.alignment_exponent,
            target_address_size: bntx.target_address_size,
            flag: bntx.flag,
            name: bntx.name.clone(),
            platform: bntx.platform,
            preview_channels,
            source_string_order: bntx.source_string_order.clone(),
            textures,
        }
    }

    fn validate(&self) -> io::Result<Vec<(PathBuf, PathBuf)>> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(invalid_data(format!(
                "unsupported BNTX bundle schema version {}",
                self.schema_version
            )));
        }
        let mut names = HashSet::with_capacity(self.textures.len());
        let mut paths = HashSet::with_capacity(self.textures.len() * 2);
        let mut validated = Vec::with_capacity(self.textures.len());
        for texture in &self.textures {
            if !names.insert(texture.name.as_str()) {
                return Err(invalid_data(format!(
                    "duplicate texture name {:?} in BNTX manifest",
                    texture.name
                )));
            }
            let raw = manifest_sidecar_path(&texture.raw_path, "bin")?;
            let preview = manifest_sidecar_path(&texture.preview_path, "png")?;
            if texture.preview_hash.as_ref().is_some_and(|hash| {
                hash.len() != 16 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            }) {
                return Err(invalid_data(format!(
                    "invalid preview hash for texture {:?}",
                    texture.name
                )));
            }
            for path in [&raw, &preview] {
                let key = path.to_string_lossy().to_ascii_lowercase();
                if !paths.insert(key) {
                    return Err(invalid_data(format!(
                        "duplicate sidecar path {} in BNTX manifest",
                        path.display()
                    )));
                }
            }
            validated.push((raw, preview));
        }
        Ok(validated)
    }
}

pub(super) fn sidecar_stem(index: usize, name: &str) -> String {
    let mut safe = name
        .chars()
        .map(|character| {
            if is_portable_sidecar_character(character) {
                character
            } else {
                '_'
            }
        })
        .take(80)
        .collect::<String>();
    if safe.is_empty() {
        safe.push_str("texture");
    }
    format!("{index:04}-{safe}")
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub(super) fn manifest_sidecar_path(value: &str, extension: &str) -> io::Result<PathBuf> {
    if value.is_empty() || value.contains('\\') {
        return Err(invalid_data(format!(
            "unsafe bundle sidecar path {value:?}"
        )));
    }
    let path = PathBuf::from(value);
    let components = path.components().collect::<Vec<_>>();
    let file_name = path.file_name().and_then(|value| value.to_str());
    if components.len() != 2
        || components[0].as_os_str() != "textures"
        || !matches!(components[1], Component::Normal(_))
        || path.extension().and_then(|value| value.to_str()) != Some(extension)
        || !file_name.is_some_and(is_portable_sidecar_name)
    {
        return Err(invalid_data(format!(
            "unsafe bundle sidecar path {value:?}"
        )));
    }
    Ok(path)
}

fn is_portable_sidecar_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(is_portable_sidecar_character)
        && !name.ends_with([' ', '.'])
        && !is_windows_device_name(name)
}

fn is_portable_sidecar_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
}

fn is_windows_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    stem.eq_ignore_ascii_case("con")
        || stem.eq_ignore_ascii_case("prn")
        || stem.eq_ignore_ascii_case("aux")
        || stem.eq_ignore_ascii_case("nul")
        || ["com", "lpt"].into_iter().any(|prefix| {
            stem.get(..3)
                .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
                && stem
                    .as_bytes()
                    .get(3)
                    .is_some_and(|byte| matches!(byte, b'1'..=b'9'))
                && stem.len() == 4
        })
}

pub(super) fn validate_bundle_root(root: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| path_error("inspect bundle directory", root, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid_input(format!(
            "bundle input {} must be a regular directory, not a symbolic link",
            root.display()
        )));
    }
    for entry in WalkDir::new(root)
        .min_depth(1)
        .follow_links(false)
        .follow_root_links(false)
    {
        let entry = entry.map_err(|error| {
            invalid_input(format!(
                "failed to inspect bundle tree {}: {error}",
                root.display()
            ))
        })?;
        if entry.file_type().is_symlink() {
            return Err(invalid_input(format!(
                "symbolic links are not allowed in BNTX bundles: {}",
                entry.path().display()
            )));
        }
        if !entry.file_type().is_dir() && !entry.file_type().is_file() {
            return Err(invalid_input(format!(
                "bundle entry {} is not a regular file",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

pub(super) fn read_regular_file(path: &Path, label: &str) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| path_error(&format!("inspect {label}"), path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid_input(format!(
            "{label} {} is not a regular file",
            path.display()
        )));
    }
    fs::read(path).map_err(|error| path_error(&format!("read {label}"), path, error))
}

fn validate_output_paths<'a>(
    destination: &Path,
    paths: impl IntoIterator<Item = &'a PathBuf>,
) -> io::Result<()> {
    validate_directory_ancestors(destination, "bundle destination")?;
    for relative in paths {
        let path = destination.join(relative);
        if metadata_if_exists(&path, "inspect bundle output")?.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("bundle output {} already exists", path.display()),
            ));
        }
        let mut parent = path.parent();
        while let Some(candidate) = parent {
            if candidate == destination {
                break;
            }
            if let Some(metadata) = metadata_if_exists(candidate, "inspect bundle output parent")? {
                validate_directory_metadata(candidate, &metadata, "bundle output parent")?;
            }
            parent = candidate.parent();
        }
    }
    Ok(())
}

fn validate_directory_ancestors(path: &Path, label: &str) -> io::Result<()> {
    for ancestor in path
        .ancestors()
        .filter(|ancestor| !ancestor.as_os_str().is_empty())
    {
        if let Some(metadata) = metadata_if_exists(ancestor, &format!("inspect {label}"))? {
            validate_directory_metadata(ancestor, &metadata, label)?;
        }
    }
    Ok(())
}

fn validate_directory_metadata(
    path: &Path,
    metadata: &fs::Metadata,
    label: &str,
) -> io::Result<()> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid_input(format!(
            "{label} {} is not a regular directory",
            path.display()
        )));
    }
    Ok(())
}

fn metadata_if_exists(path: &Path, action: &str) -> io::Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(path_error(action, path, error)),
    }
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn path_error(action: &str, path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("failed to {action} {}: {error}", path.display()),
    )
}
