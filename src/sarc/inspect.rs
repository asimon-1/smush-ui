use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};

use binrw::BinReaderExt;
use serde::Serialize;

use crate::{
    bflan::{Bflan, inspect::BflanDiff},
    bflyt::{Bflyt, inspect::BflytDiff},
    bntx::{Bntx, BntxDiff},
    hash::sha256_hex,
};

use super::{SarcArchive, SarcEntry};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SarcEntryIdentity {
    Path { path: String },
    Hash { hash: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedFileKind {
    Sarc,
    Bflyt,
    Bflan,
    Bntx,
    Opaque,
}

impl DetectedFileKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sarc => "sarc",
            Self::Bflyt => "bflyt",
            Self::Bflan => "bflan",
            Self::Bntx => "bntx",
            Self::Opaque => "opaque",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SarcEntryInspection {
    pub identity: SarcEntryIdentity,
    pub name_hash: String,
    pub stored_index: usize,
    pub payload_size: u64,
    pub payload_sha256: String,
    pub detected_kind: DetectedFileKind,
    pub extension: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SarcInspection {
    pub entry_count: usize,
    pub entries: Vec<SarcEntryInspection>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SarcEntryChange {
    pub identity: SarcEntryIdentity,
    pub before_stored_index: usize,
    pub after_stored_index: usize,
    pub before_payload_size: u64,
    pub after_payload_size: u64,
    pub before_payload_sha256: String,
    pub after_payload_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bflyt: Option<BflytDiff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bflan: Option<BflanDiff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bntx: Option<BntxDiff>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SarcDiff {
    pub before_entry_count: usize,
    pub after_entry_count: usize,
    pub added: Vec<SarcEntryInspection>,
    pub removed: Vec<SarcEntryInspection>,
    pub changed: Vec<SarcEntryChange>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SarcDiffOptions {
    pub expand_bflyt: bool,
    pub expand_bflan: bool,
    pub expand_bntx: bool,
}

impl SarcDiff {
    pub fn has_differences(&self) -> bool {
        !(self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty())
    }
}

pub fn inspect(archive: &SarcArchive) -> SarcInspection {
    let mut entries = archive
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| inspect_entry(entry, index))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.identity
            .cmp(&right.identity)
            .then(left.stored_index.cmp(&right.stored_index))
    });
    SarcInspection {
        entry_count: archive.entries.len(),
        entries,
    }
}

pub fn diff(before: &SarcArchive, after: &SarcArchive) -> SarcDiff {
    diff_with_options(before, after, SarcDiffOptions::default())
}

pub fn diff_with_options(
    before: &SarcArchive,
    after: &SarcArchive,
    options: SarcDiffOptions,
) -> SarcDiff {
    let before_entries = entries_by_identity(before);
    let after_entries = entries_by_identity(after);
    let identities: BTreeSet<_> = before_entries
        .keys()
        .chain(after_entries.keys())
        .cloned()
        .collect();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for identity in identities {
        let before_matches = before_entries
            .get(&identity)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let after_matches = after_entries
            .get(&identity)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let common_count = before_matches.len().min(after_matches.len());

        for ((before_index, before_entry), (after_index, after_entry)) in before_matches
            .iter()
            .take(common_count)
            .zip(after_matches.iter().take(common_count))
        {
            if before_entry.data != after_entry.data {
                let before_kind = detect_magic(&before_entry.data);
                let after_kind = detect_magic(&after_entry.data);
                changed.push(SarcEntryChange {
                    identity: identity.clone(),
                    before_stored_index: *before_index,
                    after_stored_index: *after_index,
                    before_payload_size: before_entry.data.len() as u64,
                    after_payload_size: after_entry.data.len() as u64,
                    before_payload_sha256: sha256_hex(&before_entry.data),
                    after_payload_sha256: sha256_hex(&after_entry.data),
                    bflyt: if options.expand_bflyt
                        && is_candidate(
                            &identity,
                            "bflyt",
                            before_kind,
                            after_kind,
                            DetectedFileKind::Bflyt,
                        ) {
                        diff_bflyt_payloads(&before_entry.data, &after_entry.data)
                    } else {
                        None
                    },
                    bflan: if options.expand_bflan
                        && is_candidate(
                            &identity,
                            "bflan",
                            before_kind,
                            after_kind,
                            DetectedFileKind::Bflan,
                        ) {
                        diff_bflan_payloads(&before_entry.data, &after_entry.data)
                    } else {
                        None
                    },
                    bntx: if options.expand_bntx
                        && is_candidate(
                            &identity,
                            "bntx",
                            before_kind,
                            after_kind,
                            DetectedFileKind::Bntx,
                        ) {
                        diff_bntx_payloads(&before_entry.data, &after_entry.data)
                    } else {
                        None
                    },
                });
            }
        }
        removed.extend(
            before_matches[common_count..]
                .iter()
                .map(|(index, entry)| inspect_entry(entry, *index)),
        );
        added.extend(
            after_matches[common_count..]
                .iter()
                .map(|(index, entry)| inspect_entry(entry, *index)),
        );
    }

    SarcDiff {
        before_entry_count: before.entries.len(),
        after_entry_count: after.entries.len(),
        added,
        removed,
        changed,
    }
}

fn diff_bflyt_payloads(before: &[u8], after: &[u8]) -> Option<BflytDiff> {
    let before: Bflyt = Cursor::new(before).read_le().ok()?;
    let after: Bflyt = Cursor::new(after).read_le().ok()?;
    crate::bflyt::inspect::diff_bflyt(&before, &after).ok()
}

fn diff_bflan_payloads(before: &[u8], after: &[u8]) -> Option<BflanDiff> {
    let before = Bflan::from_bytes(before).ok()?;
    let after = Bflan::from_bytes(after).ok()?;
    crate::bflan::inspect::diff_bflan(&before, &after).ok()
}

fn diff_bntx_payloads(before: &[u8], after: &[u8]) -> Option<BntxDiff> {
    let before = Bntx::from_bytes(before).ok()?;
    let after = Bntx::from_bytes(after).ok()?;
    Some(crate::bntx::diff_bntx(&before, &after))
}

fn is_candidate(
    identity: &SarcEntryIdentity,
    expected_extension: &str,
    before_kind: DetectedFileKind,
    after_kind: DetectedFileKind,
    expected_kind: DetectedFileKind,
) -> bool {
    before_kind == expected_kind
        || after_kind == expected_kind
        || matches!(identity, SarcEntryIdentity::Path { path } if extension(path).as_deref() == Some(expected_extension))
}

pub fn inspect_entry(entry: &SarcEntry, stored_index: usize) -> SarcEntryInspection {
    SarcEntryInspection {
        identity: identity(entry),
        name_hash: format!("{:08x}", entry.hash),
        stored_index,
        payload_size: entry.data.len() as u64,
        payload_sha256: sha256_hex(&entry.data),
        detected_kind: detect_magic(&entry.data),
        extension: entry.name.as_deref().and_then(extension),
    }
}

pub fn detect_magic(data: &[u8]) -> DetectedFileKind {
    if data.starts_with(b"SARC\x14\x00\xff\xfe") {
        DetectedFileKind::Sarc
    } else if data.starts_with(b"FLYT\xff\xfe\x14\x00") {
        DetectedFileKind::Bflyt
    } else if data.starts_with(b"FLAN\xff\xfe\x14\x00") {
        DetectedFileKind::Bflan
    } else if data.starts_with(b"BNTX\0\0\0\0") {
        DetectedFileKind::Bntx
    } else {
        DetectedFileKind::Opaque
    }
}

fn entries_by_identity(
    archive: &SarcArchive,
) -> BTreeMap<SarcEntryIdentity, Vec<(usize, &SarcEntry)>> {
    let mut entries = BTreeMap::<_, Vec<_>>::new();
    for (index, entry) in archive.entries.iter().enumerate() {
        entries
            .entry(identity(entry))
            .or_default()
            .push((index, entry));
    }
    entries
}

fn identity(entry: &SarcEntry) -> SarcEntryIdentity {
    match &entry.name {
        Some(path) => SarcEntryIdentity::Path { path: path.clone() },
        None => SarcEntryIdentity::Hash {
            hash: format!("{:08x}", entry.hash),
        },
    }
}

fn extension(path: &str) -> Option<String> {
    let file_name = path.rsplit(['/', '\\']).next()?;
    let (_, extension) = file_name.rsplit_once('.')?;
    (!extension.is_empty()).then(|| extension.to_ascii_lowercase())
}
