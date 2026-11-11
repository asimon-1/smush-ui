use smush_ui::sarc::{
    SarcArchive, SarcEntry,
    fs::{insert_file, pack_from_directory, unpack_to_directory},
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
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
            .join("target/sarc-fs-tests")
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
fn unpacks_nested_entries_and_repackages_the_same_archive() {
    let workspace = TestDirectory::new("round-trip");
    let destination = workspace.path().join("unpacked");
    let output = workspace.path().join("repacked.arc");
    let archive = SarcArchive::new(vec![
        SarcEntry::new("timg/texture.bntx", b"texture".to_vec()),
        SarcEntry::new("anim/open.bflan", b"animation".to_vec()),
        SarcEntry::new("blyt/menu.bflyt", b"layout".to_vec()),
    ]);

    unpack_to_directory(&archive, &destination).unwrap();

    assert_eq!(
        fs::read(destination.join("anim/open.bflan")).unwrap(),
        b"animation"
    );
    assert_eq!(
        fs::read(destination.join("blyt/menu.bflyt")).unwrap(),
        b"layout"
    );
    assert_eq!(
        fs::read(destination.join("timg/texture.bntx")).unwrap(),
        b"texture"
    );

    pack_from_directory(&destination, &output).unwrap();
    assert_eq!(fs::read(output).unwrap(), archive.to_bytes().unwrap());
}

#[test]
fn packs_regular_files_deterministically() {
    let workspace = TestDirectory::new("deterministic");
    let first = workspace.path().join("first");
    let second = workspace.path().join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();

    write_file(&first, "z/last.info", b"last");
    write_file(&first, "a/first.bflyt", b"first");
    write_file(&first, "middle.bflan", b"middle");
    write_file(&second, "middle.bflan", b"middle");
    write_file(&second, "a/first.bflyt", b"first");
    write_file(&second, "z/last.info", b"last");

    let first_output = workspace.path().join("first.arc");
    let second_output = workspace.path().join("second.arc");
    pack_from_directory(&first, &first_output).unwrap();
    pack_from_directory(&second, &second_output).unwrap();

    let first_bytes = fs::read(&first_output).unwrap();
    assert_eq!(first_bytes, fs::read(second_output).unwrap());
    let archive = SarcArchive::from_bytes(&first_bytes).unwrap();
    let files = archive
        .entries
        .into_iter()
        .map(|entry| (entry.name.unwrap(), entry.data))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(files["a/first.bflyt"], b"first");
    assert_eq!(files["middle.bflan"], b"middle");
    assert_eq!(files["z/last.info"], b"last");
}

#[test]
fn rejects_unsafe_archive_paths_before_creating_the_destination() {
    let workspace = TestDirectory::new("unsafe-paths");
    let invalid_names = ["", "/absolute", "../outside", "a/../../outside", "a\\b"];

    for (index, name) in invalid_names.into_iter().enumerate() {
        let destination = workspace.path().join(format!("invalid-{index}"));
        let archive = SarcArchive::new(vec![SarcEntry::new(name, vec![1])]);
        assert!(
            unpack_to_directory(&archive, &destination).is_err(),
            "{name:?}"
        );
        assert!(!destination.exists(), "{name:?}");
    }
}

#[test]
fn rejects_hash_only_duplicate_and_parent_file_conflicts() {
    let workspace = TestDirectory::new("extraction-conflicts");

    let cases = [
        SarcArchive::new(vec![SarcEntry::hash_only(7, vec![1])]),
        SarcArchive::new(vec![
            SarcEntry::new("same", vec![1]),
            SarcEntry::new("same", vec![2]),
        ]),
        SarcArchive::new(vec![
            SarcEntry::new("parent", vec![1]),
            SarcEntry::new("parent/child", vec![2]),
        ]),
    ];

    for (index, archive) in cases.into_iter().enumerate() {
        let destination = workspace.path().join(format!("conflict-{index}"));
        assert!(unpack_to_directory(&archive, &destination).is_err());
    }
}

#[test]
fn extracts_into_nonempty_destinations_without_overwriting_files() {
    let workspace = TestDirectory::new("destination");
    let archive = SarcArchive::new(vec![SarcEntry::new("file", vec![1])]);
    let nonempty = workspace.path().join("nonempty");
    write_file(&nonempty, "existing", b"existing");

    unpack_to_directory(&archive, &nonempty).unwrap();
    assert_eq!(fs::read(nonempty.join("existing")).unwrap(), b"existing");
    assert_eq!(fs::read(nonempty.join("file")).unwrap(), [1]);

    let conflict = SarcArchive::new(vec![SarcEntry::new("existing", b"replacement".to_vec())]);
    assert!(unpack_to_directory(&conflict, &nonempty).is_err());
    assert_eq!(fs::read(nonempty.join("existing")).unwrap(), b"existing");
}

#[test]
fn rejects_symbolic_links_when_the_platform_allows_creating_one() {
    let workspace = TestDirectory::new("symlink");
    let input = workspace.path().join("input");
    write_file(&input, "target", b"data");
    let link = input.join("link");
    if let Err(error) = create_file_symlink(&input.join("target"), &link) {
        eprintln!("skipping symlink assertion: {error}");
        return;
    }

    let output = workspace.path().join("output.arc");
    assert!(pack_from_directory(&input, &output).is_err());
    assert!(!output.exists());
}

#[test]
fn inserts_nested_files_and_preserves_existing_entries() {
    let workspace = TestDirectory::new("insert");
    let mut archive = SarcArchive::new(vec![
        SarcEntry::new("blyt/layout.bflyt", b"layout".to_vec()),
        SarcEntry::new("anim/open.bflan", b"animation".to_vec()),
    ]);

    insert_file(
        &mut archive,
        "timg/nested/new.bin",
        b"inserted".to_vec(),
        false,
    )
    .unwrap();

    let bytes = archive.to_bytes().unwrap();
    let parsed = SarcArchive::from_bytes(&bytes).unwrap();
    let entries = entry_map(&parsed);
    assert_eq!(entries["anim/open.bflan"], b"animation");
    assert_eq!(entries["blyt/layout.bflyt"], b"layout");
    assert_eq!(entries["timg/nested/new.bin"], b"inserted");
    assert_eq!(parsed.to_bytes().unwrap(), bytes);

    let unpacked = workspace.path().join("unpacked");
    let repacked = workspace.path().join("repacked.arc");
    unpack_to_directory(&parsed, &unpacked).unwrap();
    pack_from_directory(&unpacked, &repacked).unwrap();
    assert_eq!(fs::read(repacked).unwrap(), bytes);
}

#[test]
fn rejects_duplicates_and_replaces_only_the_matching_entry() {
    let mut archive = SarcArchive::new(vec![
        SarcEntry::new("existing", b"old".to_vec()),
        SarcEntry::new("unrelated", b"keep".to_vec()),
    ]);
    let original = archive.clone();

    let error = insert_file(&mut archive, "existing", b"new".to_vec(), false).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(archive, original);

    insert_file(&mut archive, "existing", b"new".to_vec(), true).unwrap();
    let entries = entry_map(&archive);
    assert_eq!(entries["existing"], b"new");
    assert_eq!(entries["unrelated"], b"keep");
}

#[test]
fn rejects_unsafe_insertions_and_the_entry_limit() {
    for name in ["", "/absolute", "../outside", "a\\b"] {
        let mut archive = SarcArchive::new(Vec::new());
        assert!(insert_file(&mut archive, name, Vec::new(), false).is_err());
        assert!(archive.entries.is_empty());
    }

    let mut full = SarcArchive::new(vec![SarcEntry::hash_only(0, Vec::new()); 0x3fff]);
    assert!(insert_file(&mut full, "new", Vec::new(), false).is_err());
    assert_eq!(full.entries.len(), 0x3fff);
}

#[test]
fn inserting_a_gpu_resource_promotes_data_alignment() {
    let mut archive = SarcArchive::new(vec![SarcEntry::new("ordinary.bin", vec![1])]);
    insert_file(&mut archive, "timg/texture.bntx", vec![2, 3, 4], false).unwrap();

    let bytes = archive.to_bytes().unwrap();
    let data_offset = read_u32(&bytes, 12);
    let gpu_hash = archive
        .entries
        .iter()
        .find(|entry| entry.name.as_deref() == Some("timg/texture.bntx"))
        .unwrap()
        .hash;
    let gpu_offset = sfat_payload_offset(&bytes, gpu_hash);

    assert_eq!(data_offset % 0x1000, 0);
    assert_eq!(gpu_offset % 0x1000, 0);
}

fn entry_map(archive: &SarcArchive) -> BTreeMap<&str, &[u8]> {
    archive
        .entries
        .iter()
        .map(|entry| (entry.name.as_deref().unwrap(), entry.data.as_slice()))
        .collect()
}

fn sfat_payload_offset(bytes: &[u8], hash: u32) -> u32 {
    let data_offset = read_u32(bytes, 12);
    let entry_count = u16::from_le_bytes(bytes[26..28].try_into().unwrap());
    for index in 0..usize::from(entry_count) {
        let record = 32 + index * 16;
        if read_u32(bytes, record) == hash {
            return data_offset + read_u32(bytes, record + 8);
        }
    }
    panic!("missing SFAT record for hash {hash:#010x}");
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn write_file(root: &Path, relative: &str, data: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
