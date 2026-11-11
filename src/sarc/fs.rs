use super::{SarcArchive, SarcEntry};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

struct ExtractEntry<'a> {
    relative_path: PathBuf,
    data: &'a [u8],
}

pub fn unpack_to_directory(archive: &SarcArchive, destination: impl AsRef<Path>) -> io::Result<()> {
    let destination = destination.as_ref();
    let entries = validate_extract_entries(archive)?;
    fs::create_dir_all(destination)
        .map_err(|error| path_error("create extraction directory", destination, error))?;

    for entry in entries {
        let output_path = destination.join(&entry.relative_path);
        let parent = output_path.parent().ok_or_else(|| {
            invalid_input(format!(
                "archive entry {} has no output parent",
                entry.relative_path.display()
            ))
        })?;
        fs::create_dir_all(parent)
            .map_err(|error| path_error("create extraction subdirectory", parent, error))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
            .map_err(|error| path_error("create extracted file", &output_path, error))?;
        output
            .write_all(entry.data)
            .map_err(|error| path_error("write extracted file", &output_path, error))?;
    }
    Ok(())
}

pub fn pack_from_directory(
    input_directory: impl AsRef<Path>,
    output_archive: impl AsRef<Path>,
) -> io::Result<()> {
    let input_directory = fs::canonicalize(input_directory.as_ref())
        .map_err(|error| path_error("resolve pack input", input_directory.as_ref(), error))?;
    if !input_directory.is_dir() {
        return Err(invalid_input(format!(
            "pack input {} is not a directory",
            input_directory.display()
        )));
    }
    let output_archive = output_archive.as_ref();

    let mut files = collect_regular_files(&input_directory)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let entries = files
        .into_iter()
        .map(|(name, data)| SarcEntry::new(name, data))
        .collect();

    let bytes = SarcArchive::new(entries).to_bytes().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to serialize packed SARC archive: {error}"),
        )
    })?;
    fs::write(output_archive, bytes)
        .map_err(|error| path_error("write packed SARC archive", output_archive, error))
}

fn validate_extract_entries(archive: &SarcArchive) -> io::Result<Vec<ExtractEntry<'_>>> {
    let mut entries = Vec::with_capacity(archive.entries.len());
    for entry in &archive.entries {
        let name = entry.name.as_deref().ok_or_else(|| {
            invalid_input(format!(
                "cannot extract hash-only SARC entry {:#010x} to a directory",
                entry.hash
            ))
        })?;
        entries.push(ExtractEntry {
            relative_path: archive_name_to_path(name)?,
            data: &entry.data,
        });
    }
    Ok(entries)
}

fn archive_name_to_path(name: &str) -> io::Result<PathBuf> {
    if name.is_empty() {
        return Err(invalid_archive_name(name, "the name is empty"));
    }
    if name.contains('\\') {
        return Err(invalid_archive_name(
            name,
            "backslashes are not valid archive separators",
        ));
    }
    let path = PathBuf::from(name);
    if path.components().any(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        )
    }) {
        return Err(invalid_archive_name(
            name,
            "rooted paths and parent traversal are not allowed",
        ));
    }
    Ok(path)
}

fn is_windows_drive_component(component: &str) -> bool {
    let bytes = component.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn collect_regular_files(root: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .min_depth(1)
        .follow_links(false)
        .follow_root_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|error| {
            let kind = error
                .io_error()
                .map_or(io::ErrorKind::Other, io::Error::kind);
            let path = error.path().unwrap_or(root);
            io::Error::new(
                kind,
                format!("failed to walk pack input {}: {error}", path.display()),
            )
        })?;
        let path = entry.path();
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(invalid_input(format!(
                "symbolic links are not allowed in pack input: {}",
                path.display()
            )));
        }
        if file_type.is_dir() {
            continue;
        }
        if !file_type.is_file() {
            return Err(invalid_input(format!(
                "pack input entry {} is not a regular file",
                path.display()
            )));
        }

        let relative = path.strip_prefix(root).map_err(|_| {
            invalid_input(format!("pack input {} escaped its root", path.display()))
        })?;
        let archive_name = relative_path_to_archive_name(relative)?;
        let data =
            fs::read(path).map_err(|error| path_error("read pack input file", path, error))?;
        files.push((archive_name, data));
    }
    Ok(files)
}

fn relative_path_to_archive_name(path: &Path) -> io::Result<String> {
    let mut name = String::new();
    for (index, component) in path.iter().enumerate() {
        let component = component.to_str().ok_or_else(|| {
            invalid_input(format!("input path {} is not valid UTF-8", path.display()))
        })?;
        if component.contains('\\') {
            return Err(invalid_input(format!(
                "input filename {component:?} contains an archive path separator"
            )));
        }
        if index == 0 && is_windows_drive_component(component) {
            return Err(invalid_input(format!(
                "input filename {component:?} looks like a Windows drive prefix"
            )));
        }
        if index != 0 {
            name.push('/');
        }
        name.push_str(component);
    }
    Ok(name)
}

fn invalid_archive_name(name: &str, reason: &str) -> io::Error {
    invalid_input(format!("invalid archive entry path {name:?}: {reason}"))
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn path_error(action: &str, path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("failed to {action} {}: {error}", path.display()),
    )
}
