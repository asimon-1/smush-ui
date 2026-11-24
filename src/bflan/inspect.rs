use serde::Serialize;

use super::Bflan;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BflanDiff {
    pub changes: Vec<crate::bflyt::inspect::BflytChange>,
}

pub fn diff_bflan(before: &Bflan, after: &Bflan) -> Result<BflanDiff, serde_jsonc2::Error> {
    let before = serde_jsonc2::to_value(before)?;
    let after = serde_jsonc2::to_value(after)?;
    Ok(BflanDiff {
        changes: crate::bflyt::inspect::diff_bflyt_values(&before, &after).changes,
    })
}
