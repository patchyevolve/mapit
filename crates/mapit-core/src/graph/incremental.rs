//! Incremental re-mapping — manifest diffing, file classification, and the
//! `manifest.json` sidecar file (docs/05-backend-schema.md §5).
//!
//! Design:
//! - `manifest.json` is a fast-path file: read on every `mapit` invocation to
//!   detect "anything changed?" without opening SQLite at all.
//! - SQLite `files_manifest` table is the authoritative source. If the JSON
//!   file is missing or its data disagrees with SQLite (e.g. crash mid-write),
//!   `manifest.json` is rebuilt from SQLite — the JSON never wins over the DB.
//! - `diff_manifest` returns the minimal set of files to re-process.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Per-file entry stored in both `manifest.json` and the SQLite table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    pub content_hash: String,
    pub language: Option<String>,
    pub last_parsed_at: String,
    pub parse_status: String,
}

/// The full `manifest.json` file shape (doc §5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub schema_version: u32,
    /// Maps relative file path → entry.
    pub files: HashMap<String, ManifestEntry>,
}

impl ManifestFile {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            files: HashMap::new(),
        }
    }
}

impl Default for ManifestFile {
    fn default() -> Self {
        Self::new()
    }
}

/// Classification of a file vs. the stored manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Unchanged,
    Modified,
    Added,
    Deleted,
}

/// Load `manifest.json` from the `.mapit/` directory.
/// Returns an empty manifest if the file doesn't exist.
/// Never fails on a missing file — only fails on a corrupt/unreadable file.
pub fn load_manifest(mapit_dir: &Path) -> Result<ManifestFile> {
    let path = mapit_dir.join("manifest.json");
    if !path.exists() {
        return Ok(ManifestFile::new());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading manifest at {}", path.display()))?;
    let manifest: ManifestFile = serde_json::from_str(&text)
        .with_context(|| format!("parsing manifest at {}", path.display()))?;

    if manifest.schema_version != 1 {
        warn!(
            "manifest.json schema_version={} is unsupported, treating as empty",
            manifest.schema_version
        );
        return Ok(ManifestFile::new());
    }
    Ok(manifest)
}

/// Write `manifest.json` to the `.mapit/` directory atomically
/// (write to a temp file, then rename — prevents a partial write from
/// corrupting the manifest on crash).
pub fn save_manifest(mapit_dir: &Path, manifest: &ManifestFile) -> Result<()> {
    std::fs::create_dir_all(mapit_dir)?;
    let path = mapit_dir.join("manifest.json");
    let tmp_path = mapit_dir.join("manifest.json.tmp");

    let text = serde_json::to_string_pretty(manifest)?;
    std::fs::write(&tmp_path, &text)
        .with_context(|| format!("writing temp manifest to {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &path)
        .with_context(|| format!("renaming manifest to {}", path.display()))?;

    debug!("manifest.json saved ({} entries)", manifest.files.len());
    Ok(())
}

/// Rebuild `manifest.json` from the SQLite `files_manifest` table.
/// Called when the JSON is missing, stale, or disagrees with the DB (doc §5).
pub fn rebuild_manifest_from_store(
    mapit_dir: &Path,
    store: &crate::graph::store::GraphStore,
) -> Result<ManifestFile> {
    let entries = store.all_manifest_entries()?;
    let mut manifest = ManifestFile::new();
    for (path, entry) in entries {
        manifest.files.insert(path, entry);
    }
    save_manifest(mapit_dir, &manifest)?;
    debug!(
        "manifest.json rebuilt from SQLite ({} entries)",
        manifest.files.len()
    );
    Ok(manifest)
}

/// Diff current file hashes against the stored manifest.
/// Returns a map of relative_path → FileStatus for every file.
/// `Unchanged` files are included so callers can skip re-processing them.
pub fn diff_manifest(
    current: &HashMap<String, String>, // path -> content_hash
    stored: &ManifestFile,
) -> HashMap<String, FileStatus> {
    let mut result = HashMap::new();

    for (path, hash) in current {
        match stored.files.get(path) {
            None => {
                result.insert(path.clone(), FileStatus::Added);
            }
            Some(entry) if entry.content_hash != *hash => {
                result.insert(path.clone(), FileStatus::Modified);
            }
            _ => {
                result.insert(path.clone(), FileStatus::Unchanged);
            }
        }
    }

    // Files in the manifest that are no longer on disk
    for path in stored.files.keys() {
        if !current.contains_key(path) {
            result.insert(path.clone(), FileStatus::Deleted);
        }
    }

    result
}

pub fn changed_count(diff: &HashMap<String, FileStatus>) -> usize {
    diff.values()
        .filter(|s| **s != FileStatus::Unchanged)
        .count()
}

pub fn changed_paths(diff: &HashMap<String, FileStatus>) -> Vec<&str> {
    diff.iter()
        .filter(|(_, s)| **s != FileStatus::Unchanged)
        .map(|(p, _)| p.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_manifest(entries: &[(&str, &str)]) -> ManifestFile {
        let mut m = ManifestFile::new();
        for (path, hash) in entries {
            m.files.insert(
                path.to_string(),
                ManifestEntry {
                    content_hash: hash.to_string(),
                    language: Some("rust".to_owned()),
                    last_parsed_at: "2026-01-01T00:00:00Z".to_owned(),
                    parse_status: "ok".to_owned(),
                },
            );
        }
        m
    }

    #[test]
    fn detects_unchanged() {
        let current = HashMap::from([("a.rs".to_owned(), "h1".to_owned())]);
        let stored = make_manifest(&[("a.rs", "h1")]);
        let diff = diff_manifest(&current, &stored);
        assert_eq!(diff["a.rs"], FileStatus::Unchanged);
    }

    #[test]
    fn detects_modified_file() {
        let current = HashMap::from([("a.rs".to_owned(), "hash2".to_owned())]);
        let stored = make_manifest(&[("a.rs", "hash1")]);
        let diff = diff_manifest(&current, &stored);
        assert_eq!(diff["a.rs"], FileStatus::Modified);
    }

    #[test]
    fn detects_added_file() {
        let current = HashMap::from([("b.rs".to_owned(), "h1".to_owned())]);
        let stored = make_manifest(&[]);
        let diff = diff_manifest(&current, &stored);
        assert_eq!(diff["b.rs"], FileStatus::Added);
    }

    #[test]
    fn detects_deleted_file() {
        let current = HashMap::new();
        let stored = make_manifest(&[("a.rs", "h1")]);
        let diff = diff_manifest(&current, &stored);
        assert_eq!(diff["a.rs"], FileStatus::Deleted);
    }

    #[test]
    fn changed_count_only_counts_changes() {
        let current = HashMap::from([
            ("a.rs".to_owned(), "h1".to_owned()), // unchanged
            ("b.rs".to_owned(), "h2".to_owned()), // added
        ]);
        let stored = make_manifest(&[("a.rs", "h1"), ("c.rs", "h3")]);
        let diff = diff_manifest(&current, &stored);
        // b.rs added, c.rs deleted = 2 changes
        assert_eq!(changed_count(&diff), 2);
    }

    #[test]
    fn manifest_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mapit_dir = dir.path().join(".mapit");
        std::fs::create_dir_all(&mapit_dir).unwrap();

        let mut m = ManifestFile::new();
        m.files.insert(
            "src/main.rs".to_owned(),
            ManifestEntry {
                content_hash: "abc123".to_owned(),
                language: Some("rust".to_owned()),
                last_parsed_at: "2026-01-01T00:00:00Z".to_owned(),
                parse_status: "ok".to_owned(),
            },
        );
        save_manifest(&mapit_dir, &m).unwrap();

        let loaded = load_manifest(&mapit_dir).unwrap();
        assert_eq!(loaded.files["src/main.rs"].content_hash, "abc123");
    }

    #[test]
    fn load_manifest_returns_empty_if_missing() {
        let dir = TempDir::new().unwrap();
        let m = load_manifest(dir.path()).unwrap();
        assert!(m.files.is_empty());
    }
}
