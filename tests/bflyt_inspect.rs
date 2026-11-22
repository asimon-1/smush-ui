use serde_jsonc2::Value;
use smush_ui::bflyt::inspect::{BflytChangeKind, diff_bflyt_values};

fn value(source: &str) -> Value {
    serde_jsonc2::from_str(source).expect("fixture should be valid JSON")
}

#[test]
fn recursively_reports_added_removed_and_changed_values_with_escaped_pointers() {
    let before = value(
        r#"{
            "a/b": {"~name": 1, "removed": false},
            "sections": [{"name": "first"}]
        }"#,
    );
    let after = value(
        r#"{
            "a/b": {"~name": 2, "added": "new"},
            "sections": [{"name": "first"}, {"name": "second"}]
        }"#,
    );

    let diff = diff_bflyt_values(&before, &after);

    assert_eq!(
        diff.changes
            .iter()
            .map(|change| (change.path.as_str(), change.kind))
            .collect::<Vec<_>>(),
        [
            ("/a~1b/added", BflytChangeKind::Added),
            ("/a~1b/removed", BflytChangeKind::Removed),
            ("/a~1b/~0name", BflytChangeKind::Changed),
            ("/sections/1", BflytChangeKind::Added),
        ]
    );
    assert_eq!(diff.changes[0].before, None);
    assert_eq!(diff.changes[0].after, Some(value("\"new\"")));
    assert_eq!(diff.changes[1].before, Some(value("false")));
    assert_eq!(diff.changes[1].after, None);
    assert_eq!(diff.changes[2].before, Some(value("1")));
    assert_eq!(diff.changes[2].after, Some(value("2")));
}

#[test]
fn arrays_are_order_sensitive_and_compare_by_index() {
    let before = value(r#"{"sections":["pane","group"]}"#);
    let after = value(r#"{"sections":["group","pane"]}"#);

    let diff = diff_bflyt_values(&before, &after);

    assert_eq!(diff.changes.len(), 2);
    assert_eq!(diff.changes[0].path, "/sections/0");
    assert_eq!(diff.changes[0].kind, BflytChangeKind::Changed);
    assert_eq!(diff.changes[1].path, "/sections/1");
    assert_eq!(diff.changes[1].kind, BflytChangeKind::Changed);
}

#[test]
fn change_records_serialize_deterministically() {
    let before = value(r#"{"z":0,"a":0}"#);
    let after = value(r#"{"z":1,"a":1}"#);
    let first = diff_bflyt_values(&before, &after);
    let second = diff_bflyt_values(&before, &after);

    assert_eq!(first, second);
    assert_eq!(
        serde_jsonc2::to_string(&first).expect("diff should serialize"),
        serde_jsonc2::to_string(&second).expect("diff should serialize")
    );
    assert_eq!(first.changes[0].path, "/a");
    assert_eq!(first.changes[1].path, "/z");
}
