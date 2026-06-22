//! C language adapter (Phase 2). Stub — returns empty output until implemented.
use anyhow::Result;
use super::{AdapterOutput, LanguageAdapter};

pub struct CAdapter;

impl LanguageAdapter for CAdapter {
    fn language_id(&self) -> &'static str { "c" }
    fn file_extensions(&self) -> &'static [&'static str] { &["c", "h"] }
    fn extract(&self, _relative_path: &str, _source: &str) -> Result<AdapterOutput> {
        // TODO Phase 2
        Ok(AdapterOutput::default())
    }
}
