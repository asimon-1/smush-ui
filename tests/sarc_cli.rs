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
    let mut command = Command::new(env!("CARGO_BIN_EXE_smush-ui"));
    command
        .arg("add-arc-file")
        .arg(input)
        .arg(source)
        .arg(archive_path)
        .arg(output);
    command
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
