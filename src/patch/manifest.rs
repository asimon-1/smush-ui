use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PatchManifest {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upsert: Vec<UpsertEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patch: Vec<PatchEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<RemoveEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpsertEntry {
    pub archive_path: String,
    pub kind: UpsertKind,
    pub source: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpsertKind {
    File,
    BflytJson,
    BntxBundle,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PatchEntry {
    pub archive_path: String,
    pub kind: PatchKind,
    pub source: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PatchKind {
    Bntx,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveEntry {
    pub archive_path: String,
}
