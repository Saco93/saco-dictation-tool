#![allow(unused_crate_dependencies)]

use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repository root")
}

fn read_repo_file(path: &str) -> String {
    let full = repo_root().join(path);
    fs::read_to_string(&full).unwrap_or_else(|err| {
        panic!("failed to read {}: {}", full.display(), err);
    })
}

#[test]
fn traceability_and_checklist_cover_ac1_to_ac15() {
    let traceability = read_repo_file("docs/AC_TRACEABILITY.md");
    let checklist = read_repo_file("docs/release-go-no-go-checklist.md");

    for ac in 1..=15 {
        let token = format!("| AC{ac} |");
        assert!(
            traceability.contains(&token),
            "traceability doc missing token {token}",
        );
        assert!(
            checklist.contains(&token),
            "release checklist missing token {token}",
        );
    }
}

#[test]
fn release_checklist_keeps_blocking_gate_statuses_explicit() {
    let checklist = read_repo_file("docs/release-go-no-go-checklist.md");
    for ac in [8, 14, 15] {
        let pass_row = format!("| AC{ac} | PASS | Blocking |");
        assert!(
            checklist.contains(&pass_row),
            "blocking gate row missing or not PASS for AC{ac}",
        );
    }

    assert!(
        checklist.contains("Current Decision: CONDITIONAL GO")
            || checklist.contains("Current Decision: GO"),
        "release checklist must contain explicit GO or CONDITIONAL GO decision"
    );
}
