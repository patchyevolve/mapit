//! Directory walker — respects `.gitignore` and default excludes.
//! Returns a list of candidate source files with detected language.
//! Uses the `ignore` crate (same engine as ripgrep).

use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;
use tracing::debug;

/// A source file discovered during a walk.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Absolute path on disk.
    pub path: PathBuf,
    /// Path relative to the project root (used as the stable key everywhere).
    pub relative_path: String,
    /// Detected language id, e.g. "rust", "c", "python".
    pub language: String,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Default directory/file patterns to ignore in addition to `.gitignore`.
/// User can supplement these via `.mapitignore` or project-local config.
pub const DEFAULT_IGNORE_PATTERNS: &[&str] = &[
    ".git",
    ".mapit",
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
    "venv",
    ".venv",
    "*.min.js",
    "*.lock",
    "*.pb.go",
    "*.generated.*",
];

/// Walk `root`, apply `.gitignore` + default excludes, and return all
/// recognizable source files with their detected language.
///
/// Files whose extension maps to no supported language are skipped silently
/// (they are not surfaced as errors — unknown file types degrade to "not
/// analyzed" per TRD §4.4).
pub fn walk(root: &Path, extra_ignores: &[String]) -> Result<Vec<SourceFile>> {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false) // don't skip hidden files by default (kernel headers etc.)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false);

    // Add default ignore patterns
    let mut overrides = ignore::overrides::OverrideBuilder::new(root);
    for pat in DEFAULT_IGNORE_PATTERNS {
        overrides.add(&format!("!{pat}"))?;
    }
    for pat in extra_ignores {
        overrides.add(&format!("!{pat}"))?;
    }
    builder.overrides(overrides.build()?);

    let mut files = Vec::new();

    for entry in builder.build() {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                // A single walk error never aborts the whole run (TRD §9).
                debug!("Walk error (skipping): {err}");
                continue;
            }
        };

        let path = entry.path().to_path_buf();
        if !path.is_file() {
            continue;
        }

        let language = match detect_language(&path) {
            Some(l) => l,
            None => continue, // not a recognized source file — skip silently
        };

        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();

        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);

        files.push(SourceFile {
            path,
            relative_path,
            language,
            size_bytes,
        });
    }

    debug!("Walker found {} source files under {}", files.len(), root.display());
    Ok(files)
}

/// Detect the language of a file from its extension.
/// Returns `None` for unrecognized extensions.
fn detect_language(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    let lang = match ext.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "c" => "c",
        "h" => "c",   // treated as C by default; C++ headers use .hpp/.hxx
        "cpp" | "cc" | "cxx" | "c++" => "cpp",
        "hpp" | "hh" | "hxx" => "cpp",
        "s" | "asm" => "asm",
        // .S (capital) is the preprocessed assembly convention
        _ if ext == "S" => "asm",
        "py" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "jsx" => "jsx",
        _ => return None,
    };
    Some(lang.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn walk_finds_rust_files() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.rs"), "fn main() {}").unwrap();
        fs::write(src.join("lib.rs"), "pub fn foo() {}").unwrap();
        fs::write(dir.path().join("README.md"), "# readme").unwrap();

        let files = walk(dir.path(), &[]).unwrap();
        let langs: Vec<_> = files.iter().map(|f| f.language.as_str()).collect();
        assert!(langs.iter().all(|&l| l == "rust"), "unexpected languages: {langs:?}");
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn walk_skips_git_dir() {
        let dir = TempDir::new().unwrap();
        let git = dir.path().join(".git");
        fs::create_dir(&git).unwrap();
        fs::write(git.join("config"), "[core]").unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files = walk(dir.path(), &[]).unwrap();
        assert!(!files.iter().any(|f| f.relative_path.contains(".git")));
    }

    #[test]
    fn detect_language_cases() {
        assert_eq!(detect_language(Path::new("foo.rs")), Some("rust".into()));
        assert_eq!(detect_language(Path::new("foo.c")), Some("c".into()));
        assert_eq!(detect_language(Path::new("foo.cpp")), Some("cpp".into()));
        assert_eq!(detect_language(Path::new("foo.py")), Some("python".into()));
        assert_eq!(detect_language(Path::new("foo.ts")), Some("typescript".into()));
        assert_eq!(detect_language(Path::new("foo.md")), None);
        assert_eq!(detect_language(Path::new("foo.json")), None);
    }
}
