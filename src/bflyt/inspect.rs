use serde::{Deserialize, Serialize};
use serde_jsonc2::Value;

use super::Bflyt;

/// A semantic change at an RFC 6901 JSON Pointer in the public BFLYT model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BflytChange {
    pub path: String,
    pub kind: BflytChangeKind,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BflytChangeKind {
    Added,
    Removed,
    Changed,
}

/// Stable, recursively generated changes between two public BFLYT JSON models.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BflytDiff {
    pub changes: Vec<BflytChange>,
}

impl BflytDiff {
    pub fn has_differences(&self) -> bool {
        !self.changes.is_empty()
    }
}

/// Serialize two layouts through their public Serde representation and compare them.
pub fn diff_bflyt(left: &Bflyt, right: &Bflyt) -> Result<BflytDiff, serde_jsonc2::Error> {
    let left = serde_jsonc2::to_value(left)?;
    let right = serde_jsonc2::to_value(right)?;
    Ok(diff_bflyt_values(&left, &right))
}

/// Compare serialized BFLYT values recursively.
///
/// Object keys are visited in lexical order. Arrays are compared by index and retain their
/// order semantics, so insertion or reordering can produce changes at affected indices.
pub fn diff_bflyt_values(left: &Value, right: &Value) -> BflytDiff {
    fn push_change(
        changes: &mut Vec<BflytChange>,
        path: &str,
        kind: BflytChangeKind,
        before: Option<&Value>,
        after: Option<&Value>,
    ) {
        changes.push(BflytChange {
            path: path.to_owned(),
            kind,
            before: before.cloned(),
            after: after.cloned(),
        });
    }

    fn visit(left: &Value, right: &Value, path: &str, changes: &mut Vec<BflytChange>) {
        match (left, right) {
            (Value::Object(left), Value::Object(right)) => {
                let mut keys: Vec<_> = left.keys().chain(right.keys()).collect();
                keys.sort_unstable();
                keys.dedup();

                for key in keys {
                    let child_path = format!("{path}/{}", escape_pointer_token(key));
                    match (left.get(key), right.get(key)) {
                        (Some(before), Some(after)) => visit(before, after, &child_path, changes),
                        (Some(before), None) => push_change(
                            changes,
                            &child_path,
                            BflytChangeKind::Removed,
                            Some(before),
                            None,
                        ),
                        (None, Some(after)) => push_change(
                            changes,
                            &child_path,
                            BflytChangeKind::Added,
                            None,
                            Some(after),
                        ),
                        (None, None) => unreachable!("key came from one of the two objects"),
                    }
                }
            }
            (Value::Array(left), Value::Array(right)) => {
                let shared_len = left.len().min(right.len());
                for index in 0..shared_len {
                    visit(
                        &left[index],
                        &right[index],
                        &format!("{path}/{index}"),
                        changes,
                    );
                }
                for (index, before) in left.iter().enumerate().skip(shared_len) {
                    push_change(
                        changes,
                        &format!("{path}/{index}"),
                        BflytChangeKind::Removed,
                        Some(before),
                        None,
                    );
                }
                for (index, after) in right.iter().enumerate().skip(shared_len) {
                    push_change(
                        changes,
                        &format!("{path}/{index}"),
                        BflytChangeKind::Added,
                        None,
                        Some(after),
                    );
                }
            }
            _ if left != right => push_change(
                changes,
                path,
                BflytChangeKind::Changed,
                Some(left),
                Some(right),
            ),
            _ => {}
        }
    }

    let mut changes = Vec::new();
    visit(left, right, "", &mut changes);
    BflytDiff { changes }
}

fn escape_pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}
