use serde::de::DeserializeOwned;
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::{
    common::{invalid_data, invalid_input, path_error},
    sarc::fs::validate_portable_component,
};

pub(super) fn read_json_document<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    validate_json_extension(path)?;
    let bytes = read_regular_file(path, "JSON document")?;
    parse_json_document(path, &bytes)
}

pub(super) fn parse_json_document<T: DeserializeOwned>(path: &Path, bytes: &[u8]) -> io::Result<T> {
    validate_json_extension(path)?;
    serde_jsonc2::from_slice(bytes)
        .map_err(|error| invalid_data(format!("{}: {error}", path.display())))
}

fn validate_json_extension(path: &Path) -> io::Result<()> {
    match path.extension().and_then(|value| value.to_str()) {
        Some("json" | "jsonc") => Ok(()),
        _ => Err(invalid_input(format!(
            "{} must have a .json or .jsonc extension",
            path.display()
        ))),
    }
}

pub(super) fn manifest_directory(manifest_path: &Path) -> io::Result<PathBuf> {
    let parent = manifest_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    reject_symlink_ancestors(manifest_path)?;
    fs::canonicalize(parent)
        .map_err(|error| path_error("resolve manifest directory", parent, error))
}

pub(super) fn read_source(root: &Path, relative: &str, context: &str) -> io::Result<Vec<u8>> {
    let path = resolve_source_path(root, relative, context)?;
    read_regular_file(&path, context)
}

pub(super) fn source_directory(root: &Path, relative: &str, context: &str) -> io::Result<PathBuf> {
    let path = resolve_source_path(root, relative, context)?;
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| path_error("inspect patch directory", &path, error))?;
    if !metadata.is_dir() {
        return Err(invalid_input(format!(
            "{context}: source {} is not a directory",
            path.display()
        )));
    }
    Ok(path)
}

fn resolve_source_path(root: &Path, relative: &str, context: &str) -> io::Result<PathBuf> {
    let relative = safe_relative_path(relative)
        .map_err(|error| invalid_input(format!("{context}: {error}")))?;
    let path = root.join(relative);
    reject_symlink_ancestors(&path)
        .map_err(|error| invalid_input(format!("{context}: {error}")))?;
    let canonical = fs::canonicalize(&path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "{context}: failed to resolve source {}: {error}",
                path.display()
            ),
        )
    })?;
    if !canonical.starts_with(root) {
        return Err(invalid_input(format!(
            "{context}: source {} escapes manifest directory",
            path.display()
        )));
    }
    Ok(path)
}

pub(super) fn safe_relative_path(value: &str) -> io::Result<PathBuf> {
    if value.is_empty() || value.contains('\\') {
        return Err(invalid_input(format!("unsafe source path {value:?}")));
    }
    let path = PathBuf::from(value);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid_input(format!("unsafe source path {value:?}")));
    }
    for component in value.split('/') {
        validate_portable_component(component)?;
    }
    Ok(path)
}

fn reject_symlink_ancestors(path: &Path) -> io::Result<()> {
    for ancestor in path.ancestors().filter(|item| !item.as_os_str().is_empty()) {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(invalid_input(format!(
                    "symbolic link is not allowed: {}",
                    ancestor.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(path_error("inspect patch path", ancestor, error)),
        }
    }
    Ok(())
}

fn read_regular_file(path: &Path, label: &str) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| path_error("inspect patch input", path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid_input(format!(
            "{label} {} is not a regular file",
            path.display()
        )));
    }
    fs::read(path).map_err(|error| path_error("read patch input", path, error))
}
