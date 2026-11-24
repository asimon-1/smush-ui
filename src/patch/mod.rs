mod fs;
pub mod manifest;

use std::{
    collections::HashSet,
    io::{self, Cursor},
    path::Path,
};

use binrw::BinWriterExt;

use self::{
    fs::{
        manifest_directory, parse_json_document, read_json_document, read_source,
        safe_relative_path, source_directory,
    },
    manifest::{PatchManifest, UpsertKind},
};
use crate::{
    bflan::Bflan,
    bflyt::Bflyt,
    bntx::{Bntx, fs::PackOptions, patch::apply_bntx_patch},
    sarc::{
        SarcArchive,
        fs::{
            archive_name_to_path, named_entry_index, remove_named, upsert_named,
            write_archive_atomic,
        },
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub struct PatchArcOptions;

/// Applies a versioned manifest to one SARC archive and atomically replaces the output.
pub fn patch_archive(
    input: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
    _options: PatchArcOptions,
) -> io::Result<()> {
    let input = input.as_ref();
    let manifest_path = manifest_path.as_ref();
    let output = output.as_ref();
    let bytes =
        std::fs::read(input).map_err(|error| path_error("read SARC input", input, error))?;
    let original = SarcArchive::from_bytes(&bytes)
        .map_err(|error| invalid_data(format!("{}: invalid SARC: {error}", input.display())))?;
    let manifest: PatchManifest = read_json_document(manifest_path)?;
    if manifest.schema_version != 1 {
        return Err(invalid_data(format!(
            "{}: unsupported SARC patch schema version {}",
            manifest_path.display(),
            manifest.schema_version
        )));
    }
    let root = manifest_directory(manifest_path)?;
    let mut targets = HashSet::new();
    for (index, entry) in manifest.upsert.iter().enumerate() {
        let context = format!(
            "{} upsert[{index}] {:?}",
            manifest_path.display(),
            entry.archive_path
        );
        validate_target(&entry.archive_path, &context, &mut targets)?;
        named_entry_index(&original, &entry.archive_path)
            .map_err(|error| invalid_data(format!("{context}: {error}")))?;
        safe_relative_path(&entry.source)
            .map_err(|error| invalid_input(format!("{context}: {error}")))?;
    }
    for (index, entry) in manifest.patch.iter().enumerate() {
        let context = format!(
            "{} patch[{index}] {:?}",
            manifest_path.display(),
            entry.archive_path
        );
        validate_target(&entry.archive_path, &context, &mut targets)?;
        safe_relative_path(&entry.source)
            .map_err(|error| invalid_input(format!("{context}: {error}")))?;
        require_original(&original, &entry.archive_path, &context)?;
    }
    for (index, entry) in manifest.remove.iter().enumerate() {
        let context = format!(
            "{} remove[{index}] {:?}",
            manifest_path.display(),
            entry.archive_path
        );
        validate_target(&entry.archive_path, &context, &mut targets)?;
        require_original(&original, &entry.archive_path, &context)?;
    }
    let additions = manifest
        .upsert
        .iter()
        .filter(|entry| {
            !original
                .entries
                .iter()
                .any(|existing| existing.name.as_deref() == Some(entry.archive_path.as_str()))
        })
        .count();
    let final_count = original
        .entries
        .len()
        .checked_add(additions)
        .and_then(|count| count.checked_sub(manifest.remove.len()))
        .ok_or_else(|| invalid_input("SARC patch entry count overflow"))?;
    if final_count > 0x3fff {
        return Err(invalid_input("SARC patch result exceeds 0x3fff entries"));
    }
    let mut prepared_upserts = Vec::with_capacity(manifest.upsert.len());
    for (index, entry) in manifest.upsert.iter().enumerate() {
        let context = format!(
            "{} upsert[{index}] {:?}",
            manifest_path.display(),
            entry.archive_path
        );
        let data = match entry.kind {
            UpsertKind::File => read_source(&root, &entry.source, &context)?,
            UpsertKind::BflytJson => {
                let path = root.join(safe_relative_path(&entry.source)?);
                let source = read_source(&root, &entry.source, &context)?;
                let layout: Bflyt = parse_json_document(&path, &source)
                    .map_err(|error| invalid_data(format!("{context}: {error}")))?;
                let mut writer = Cursor::new(Vec::new());
                writer.write_le(&layout).map_err(|error| {
                    invalid_data(format!(
                        "{context} source {}: failed to serialize BFLYT: {error}",
                        path.display()
                    ))
                })?;
                writer.into_inner()
            }
            UpsertKind::BflanJson => {
                let path = root.join(safe_relative_path(&entry.source)?);
                let source = read_source(&root, &entry.source, &context)?;
                let animation: Bflan = parse_json_document(&path, &source)
                    .map_err(|error| invalid_data(format!("{context}: {error}")))?;
                animation.to_bytes().map_err(|error| {
                    invalid_data(format!(
                        "{context} source {}: failed to serialize BFLAN: {error}",
                        path.display()
                    ))
                })?
            }
            UpsertKind::BntxBundle => {
                let directory = source_directory(&root, &entry.source, &context)?;
                let bntx = crate::bntx::fs::read_bundle(&directory, PackOptions::default())
                    .map_err(|error| io::Error::new(error.kind(), format!("{context}: {error}")))?;
                bntx.to_bytes().map_err(|error| {
                    invalid_data(format!(
                        "{context}: failed to serialize BNTX bundle: {error}"
                    ))
                })?
            }
        };
        prepared_upserts.push((entry.archive_path.clone(), data));
    }
    let mut prepared_patches = Vec::with_capacity(manifest.patch.len());
    for (index, entry) in manifest.patch.iter().enumerate() {
        let context = format!(
            "{} patch[{index}] {:?}",
            manifest_path.display(),
            entry.archive_path
        );
        let directory = source_directory(&root, &entry.source, &context)?;
        let original_index =
            named_entry_index(&original, &entry.archive_path)?.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{context}: archive entry does not exist"),
                )
            })?;
        let original_data = &original.entries[original_index].data;
        let bntx = Bntx::from_bytes(original_data)
            .map_err(|error| invalid_data(format!("{context}: invalid BNTX: {error}")))?;
        let patched = apply_bntx_patch(&bntx, &directory)
            .map_err(|error| io::Error::new(error.kind(), format!("{context}: {error}")))?;
        let data = patched.to_bytes().map_err(|error| {
            invalid_data(format!(
                "{context}: failed to serialize patched BNTX: {error}"
            ))
        })?;
        prepared_patches.push((entry.archive_path.clone(), data));
    }
    let mut result = original.clone();
    for (name, data) in prepared_upserts.into_iter().chain(prepared_patches) {
        upsert_named(&mut result, name, data)?;
    }
    for entry in &manifest.remove {
        remove_named(&mut result, &entry.archive_path)?;
    }
    write_archive_atomic(&result, output)
}

fn validate_target<'a>(
    name: &'a str,
    context: &str,
    targets: &mut HashSet<&'a str>,
) -> io::Result<()> {
    archive_name_to_path(name).map_err(|error| invalid_input(format!("{context}: {error}")))?;
    if !targets.insert(name) {
        return Err(invalid_input(format!(
            "{context}: duplicate archive target"
        )));
    }
    Ok(())
}

fn require_original(archive: &SarcArchive, name: &str, context: &str) -> io::Result<()> {
    let index = named_entry_index(archive, name)
        .map_err(|error| invalid_data(format!("{context}: {error}")))?;
    if index.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{context}: archive entry does not exist"),
        ));
    }
    Ok(())
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
