use smush_ui::sarc::{SarcArchive, SarcEntry};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/sarc-cli-tests")
            .join(format!(
                "{label}-{}-{unique}-{sequence}",
                std::process::id()
            ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn help_lists_and_describes_every_command() {
    let output = cli_command().arg("--help").output().unwrap();
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in [
        "to-json",
        "from-json",
        "unpack-arc",
        "pack-arc",
        "add-arc-file",
    ] {
        assert!(
            stdout.contains(command),
            "missing {command:?} in:\n{stdout}"
        );

        let output = cli_command().args([command, "--help"]).output().unwrap();
        assert_success(&output);
        assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
    }

    let output = cli_command().arg("unpack-arc").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage:"));
    assert!(stderr.contains("unpack-arc"));
}

#[test]
fn unpacks_and_repacks_an_archive() {
    let workspace = TestDirectory::new("unpack-pack");
    let input = workspace.path().join("input.arc");
    let unpacked = workspace.path().join("unpacked");
    let repacked = workspace.path().join("repacked.arc");
    let original = SarcArchive::new(vec![
        SarcEntry::new("anim/open.bflan", b"animation".to_vec()),
        SarcEntry::new("blyt/layout.bflyt", b"layout".to_vec()),
        SarcEntry::new("timg/texture.bntx", b"texture".to_vec()),
    ])
    .to_bytes()
    .unwrap();
    fs::write(&input, &original).unwrap();

    let output = cli_command()
        .arg("unpack-arc")
        .arg(&input)
        .arg(&unpacked)
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(
        fs::read(unpacked.join("anim/open.bflan")).unwrap(),
        b"animation"
    );
    assert_eq!(
        fs::read(unpacked.join("blyt/layout.bflyt")).unwrap(),
        b"layout"
    );
    assert_eq!(
        fs::read(unpacked.join("timg/texture.bntx")).unwrap(),
        b"texture"
    );

    let output = cli_command()
        .arg("pack-arc")
        .arg(&unpacked)
        .arg(&repacked)
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(fs::read(repacked).unwrap(), original);
}

#[test]
fn unpack_rejects_malformed_archives_with_the_input_path() {
    let workspace = TestDirectory::new("malformed");
    let input = workspace.path().join("malformed.arc");
    let output_directory = workspace.path().join("output");
    fs::write(&input, b"not a SARC archive").unwrap();

    let output = cli_command()
        .arg("unpack-arc")
        .arg(&input)
        .arg(&output_directory)
        .output()
        .unwrap();

    assert_failure_contains(&output, &input.display().to_string());
    assert!(!output_directory.exists());
}

#[test]
fn unpack_rejects_unsafe_entry_paths_with_the_entry_name() {
    let workspace = TestDirectory::new("unsafe-entry");
    let input = workspace.path().join("unsafe.arc");
    let output_directory = workspace.path().join("output");
    let bytes = SarcArchive::new(vec![SarcEntry::new("../outside", b"unsafe".to_vec())])
        .to_bytes()
        .unwrap();
    fs::write(&input, bytes).unwrap();

    let output = cli_command()
        .arg("unpack-arc")
        .arg(&input)
        .arg(&output_directory)
        .output()
        .unwrap();

    assert_failure_contains(&output, "../outside");
    assert!(!output_directory.exists());
}

#[test]
fn unpack_does_not_overwrite_an_existing_output_file() {
    let workspace = TestDirectory::new("existing-output");
    let input = workspace.path().join("input.arc");
    let output_directory = workspace.path().join("output");
    let existing = output_directory.join("nested/file.bin");
    let bytes = SarcArchive::new(vec![SarcEntry::new("nested/file.bin", b"archive".to_vec())])
        .to_bytes()
        .unwrap();
    fs::write(&input, bytes).unwrap();
    fs::create_dir_all(existing.parent().unwrap()).unwrap();
    fs::write(&existing, b"existing").unwrap();

    let output = cli_command()
        .arg("unpack-arc")
        .arg(&input)
        .arg(&output_directory)
        .output()
        .unwrap();

    assert_failure_contains(&output, &existing.display().to_string());
    assert_eq!(fs::read(existing).unwrap(), b"existing");
}

#[test]
fn adds_a_file_to_a_separate_output_archive() {
    let workspace = TestDirectory::new("separate-output");
    let input = workspace.path().join("input.arc");
    let source = workspace.path().join("source.bin");
    let output = workspace.path().join("output.arc");
    let original = SarcArchive::new(vec![SarcEntry::new("existing", b"old".to_vec())])
        .to_bytes()
        .unwrap();
    fs::write(&input, &original).unwrap();
    fs::write(&source, b"added").unwrap();

    let result = add_command(&input, &source, "nested/added.bin", &output)
        .output()
        .unwrap();
    assert_success(&result);

    assert_eq!(fs::read(&input).unwrap(), original);
    let archive = SarcArchive::from_bytes(&fs::read(output).unwrap()).unwrap();
    assert_eq!(entry_data(&archive, "existing"), b"old");
    assert_eq!(entry_data(&archive, "nested/added.bin"), b"added");
}

#[test]
fn duplicate_failure_leaves_input_and_output_unchanged() {
    let workspace = TestDirectory::new("duplicate");
    let input = workspace.path().join("input.arc");
    let source = workspace.path().join("source.bin");
    let output = workspace.path().join("output.arc");
    let original = SarcArchive::new(vec![SarcEntry::new("existing", b"old".to_vec())])
        .to_bytes()
        .unwrap();
    fs::write(&input, &original).unwrap();
    fs::write(&source, b"replacement").unwrap();
    fs::write(&output, b"untouched").unwrap();

    let result = add_command(&input, &source, "existing", &output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(input).unwrap(), original);
    assert_eq!(fs::read(output).unwrap(), b"untouched");
}

#[test]
fn serialization_failure_leaves_input_and_output_unchanged() {
    let workspace = TestDirectory::new("serialization-failure");
    let input = workspace.path().join("input.arc");
    let source = workspace.path().join("source.bin");
    let output = workspace.path().join("output.arc");
    let named_entry = SarcEntry::new("collision", b"added".to_vec());
    let original = SarcArchive::new(vec![SarcEntry::hash_only(
        named_entry.hash,
        b"anonymous".to_vec(),
    )])
    .to_bytes()
    .unwrap();
    fs::write(&input, &original).unwrap();
    fs::write(&source, &named_entry.data).unwrap();
    fs::write(&output, b"untouched").unwrap();

    let result = add_command(&input, &source, "collision", &output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(input).unwrap(), original);
    assert_eq!(fs::read(output).unwrap(), b"untouched");
}

#[test]
fn replaces_an_entry_atomically_in_place() {
    let workspace = TestDirectory::new("in-place");
    let archive_path = workspace.path().join("archive.arc");
    let source = workspace.path().join("source.bin");
    let original = SarcArchive::new(vec![
        SarcEntry::new("existing", b"old".to_vec()),
        SarcEntry::new("unrelated", b"keep".to_vec()),
    ]);
    fs::write(&archive_path, original.to_bytes().unwrap()).unwrap();
    fs::write(&source, b"replacement").unwrap();

    let result = add_command(&archive_path, &source, "existing", &archive_path)
        .arg("--replace")
        .output()
        .unwrap();
    assert_success(&result);

    let archive = SarcArchive::from_bytes(&fs::read(archive_path).unwrap()).unwrap();
    assert_eq!(entry_data(&archive, "existing"), b"replacement");
    assert_eq!(entry_data(&archive, "unrelated"), b"keep");
}

fn add_command(input: &Path, source: &Path, archive_path: &str, output: &Path) -> Command {
    let mut command = cli_command();
    command
        .arg("add-arc-file")
        .arg(input)
        .arg(source)
        .arg(archive_path)
        .arg(output);
    command
}

fn cli_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_smush-ui"))
}

fn entry_data<'a>(archive: &'a SarcArchive, name: &str) -> &'a [u8] {
    archive
        .entries
        .iter()
        .find(|entry| entry.name.as_deref() == Some(name))
        .unwrap()
        .data
        .as_slice()
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_failure_contains(output: &std::process::Output, expected: &str) {
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected),
        "stderr did not contain {expected:?}:\n{stderr}"
    );
}
