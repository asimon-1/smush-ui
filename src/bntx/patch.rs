use std::{collections::HashSet, fs, io, path::Path};

use serde::{Deserialize, Serialize};

use super::{
    Bntx, NewTextureFormat, TexturePatch, TextureProfile, TextureUpsert, apply_texture_patch,
    fs::{manifest_sidecar_path, read_regular_file, validate_bundle_root, write_bntx_atomic},
    read_png, validation,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BntxPatchManifest {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upsert: Vec<BntxUpsertEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<BntxRemoveEntry>,
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BntxRemoveEntry {
    pub name: String,
}

pub fn patch_bntx_file(
    input: impl AsRef<Path>,
    patch_directory: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> io::Result<()> {
    let input = input.as_ref();
    let patch_directory = patch_directory.as_ref();
    let output = output.as_ref();
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

    let bytes = fs::read(input)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", input.display())))?;
    let original = Bntx::from_bytes(&bytes)
        .map_err(|error| invalid_data(format!("{}: {error}", input.display())))?;

    let mut targets = HashSet::new();
    for (index, entry) in manifest.upsert.iter().enumerate() {
        let BntxUpsertEntry::Png {
            name, like, format, ..
        } = entry;
        let context = format!("{} upsert[{index}] {name:?}", manifest_path.display());
        validation::validate_texture_name(name)
            .map_err(|error| invalid_data(format!("{context}: {error}")))?;
        if !targets.insert(name.as_str()) {
            return Err(invalid_data(format!("{context}: duplicate texture target")));
        }
        if like.is_some() && format.is_some() {
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

    let mut patch = TexturePatch {
        upsert: Vec::with_capacity(manifest.upsert.len()),
        remove: manifest
            .remove
            .iter()
            .map(|entry| entry.name.clone())
            .collect(),
    };
    for (index, entry) in manifest.upsert.iter().enumerate() {
        let BntxUpsertEntry::Png {
            name,
            source,
            like,
            format,
        } = entry;
        let context = format!("{} upsert[{index}] {name:?}", manifest_path.display());
        let relative = manifest_sidecar_path(source, "png")
            .map_err(|error| invalid_data(format!("{context}: {error}")))?;
        let path = patch_directory.join(relative);
        let png = read_regular_file(&path, "BNTX patch PNG")
            .map_err(|error| io::Error::new(error.kind(), format!("{context}: {error}")))?;
        let image = read_png(&png)
            .map_err(|error| invalid_data(format!("{context} {}: {error}", path.display())))?;
        let profile = like
            .as_ref()
            .map(|name| TextureProfile::Like(name.clone()))
            .or_else(|| format.map(TextureProfile::Format));
        patch.upsert.push(TextureUpsert {
            name: name.clone(),
            image,
            profile,
        });
    }
    let result = apply_texture_patch(&original, &patch)
        .map_err(|error| invalid_data(format!("{}: {error}", manifest_path.display())))?;
    write_bntx_atomic(&result, output)
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
