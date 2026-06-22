//! Incremental re-mapping support (Phase 3).
//! Stubs only — will be fleshed out in Phase 3.

use std::collections::HashMap;
use anyhow::Result;

/// Per-file state from the manifest.
#[derive(Debug, Clone)]
pub struct ManifestEntry {
    pub content_hash: String,
    pub language: Option<String>,
    pub parse_status: String,
}

/// Classify a file relative to the stored manifest.
#[derive(Debug, PartialEq, Eq)]
pub enum FileStatus {
    Unchanged,
    Modified,
    Added,
    Deleted,
}

/// Diff the current file hashes against the stored manifest.
/// Returns a map of relative_path -> FileStatus for every file that changed.
pub fn diff_manifest(
    current: &HashMap<String, String>, // path -> content_hash
    stored: &HashMap<String, ManifestEntry>,
) -> Result<HashMap<String, FileStatus>> {
    let mut result = HashMap::new();

    for (path, hash) in current {
        match stored.get(path) {
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

    for path in stored.keys() {
        if !current.contains_key(path) {
            result.insert(path.clone(), FileStatus::Deleted);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_modified_file() {
        let current = HashMap::from([("a.rs".to_owned(), "hash2".to_owned())]);
        let stored = HashMap::from([(
            "a.rs".to_owned(),
            ManifestEntry {
                content_hash: "hash1".to_owned(),
                language: Some("rust".to_owned()),
                parse_status: "ok".to_owned(),
            },
        )]);
        let diff = diff_manifest(&current, &stored).unwrap();
        assert_eq!(diff["a.rs"], FileStatus::Modified);
    }

    #[test]
    fn detects_added_and_deleted() {
        let current = HashMap::from([("b.rs".to_owned(), "h1".to_owned())]);
        let stored = HashMap::from([(
            "a.rs".to_owned(),
            ManifestEntry {
                content_hash: "h0".to_owned(),
                language: None,
                parse_status: "ok".to_owned(),
            },
        )]);
        let diff = diff_manifest(&current, &stored).unwrap();
        assert_eq!(diff["b.rs"], FileStatus::Added);
        assert_eq!(diff["a.rs"], FileStatus::Deleted);
    }
}
