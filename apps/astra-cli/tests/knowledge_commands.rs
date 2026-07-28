use assert_cmd::Command;
use astra_knowledge::{
    Confidence, Evidence, EvidenceKind, KnowledgeCategory, KnowledgeClaim, KnowledgeNamespace,
    KnowledgeStore, Validity, KNOWLEDGE_SCHEMA_VERSION,
};
use predicates::prelude::*;
use serde_json::Value;
use std::path::Path;
use tempfile::tempdir;

fn astra(home: &Path, knowledge: &Path) -> Command {
    let mut command = Command::cargo_bin("astra").expect("binary");
    command
        .env("HOME", home)
        .env("ASTRA_CONFIG_DIR", home.join("astra-config"))
        .env("ASTRA_KNOWLEDGE_DIR", knowledge)
        .env("PATH", "")
        .current_dir(home);
    command
}

fn seed_claims(root: &Path) {
    let store = KnowledgeStore::open(root);
    let namespace = KnowledgeNamespace::project("demo");
    let fact = KnowledgeClaim::new(
        KnowledgeCategory::Fact,
        "project:demo",
        "uses_language",
        serde_json::json!("rust"),
        vec![Evidence::new(EvidenceKind::ContextFact, "language:rust")],
        Confidence::High,
        Validity::Current,
    )
    .and_then(|claim| claim.with_created_at("2026-01-01T00:00:00Z"))
    .expect("fact claim");
    let decision = KnowledgeClaim::new(
        KnowledgeCategory::Decision,
        "project:demo",
        "execution_boundary",
        serde_json::json!("state_bound"),
        vec![Evidence::new(EvidenceKind::AdrDecision, "0008")],
        Confidence::High,
        Validity::Current,
    )
    .and_then(|claim| claim.with_created_at("2026-01-01T00:00:00Z"))
    .expect("decision claim");
    store.add_claim(&namespace, &fact).expect("store fact");
    store
        .add_claim(&namespace, &decision)
        .expect("store decision");
}

#[test]
fn knowledge_help_lists_read_only_queries() {
    let home = tempdir().expect("home");
    let knowledge = tempdir().expect("knowledge");
    astra(home.path(), knowledge.path())
        .args(["knowledge", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("facts"))
        .stdout(predicate::str::contains("verifications"))
        .stdout(predicate::str::contains("decisions"))
        .stdout(predicate::str::contains("edit").not());
}

#[test]
fn knowledge_queries_are_versioned_and_deterministic() {
    let home = tempdir().expect("home");
    let knowledge = tempdir().expect("knowledge");
    seed_claims(knowledge.path());

    let first = astra(home.path(), knowledge.path())
        .args(["knowledge", "show", "demo", "--json"])
        .output()
        .expect("first output");
    let second = astra(home.path(), knowledge.path())
        .args(["knowledge", "show", "demo", "--json"])
        .output()
        .expect("second output");
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    let json: Value = serde_json::from_slice(&first.stdout).expect("knowledge JSON");
    assert_eq!(json["schema_version"], KNOWLEDGE_SCHEMA_VERSION);
    assert_eq!(json["project"], "demo");
    assert_eq!(json["claims"].as_array().expect("claims").len(), 2);

    astra(home.path(), knowledge.path())
        .args(["knowledge", "facts", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("uses_language"))
        .stdout(predicate::str::contains("execution_boundary").not());
}

#[test]
fn knowledge_list_is_empty_without_persisted_projects() {
    let home = tempdir().expect("home");
    let knowledge = tempdir().expect("knowledge");
    astra(home.path(), knowledge.path())
        .args(["knowledge", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No persisted project knowledge."));
}
