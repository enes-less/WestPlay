//! WestPlay has one version, the workspace's (Cargo.toml), set for a release by tools/release/version.ps1. The
//! places that must repeat it (Tauri's config, which writes it into the app's file details; the UI's package)
//! say the same, and the changelog has its release.

use std::path::Path;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

fn json_version(path: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(&read(path)).unwrap();
    value["version"].as_str().unwrap_or_else(|| panic!("{path} has no version")).to_owned()
}

#[test]
fn every_place_says_the_workspaces_version() {
    assert_eq!(json_version("tauri.conf.json"), VERSION, "app/tauri.conf.json");
    assert_eq!(json_version("../ui/package.json"), VERSION, "ui/package.json");
}

#[test]
fn the_changelog_has_this_version_or_whats_to_come() {
    let changelog = read("../CHANGELOG.md");
    let release = format!("\n## {VERSION} ");
    assert!(
        changelog.contains(&release) || changelog.contains("\n## Unreleased"),
        "CHANGELOG.md has neither `## {VERSION} (date)` nor `## Unreleased`"
    );
}
