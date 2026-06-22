//! Assembly language adapter (Phase 2). Stub.
use anyhow::Result;
use super::{AdapterOutput, LanguageAdapter};

pub struct AsmAdapter;

impl LanguageAdapter for AsmAdapter {
    fn language_id(&self) -> &'static str { "asm" }
    fn file_extensions(&self) -> &'static [&'static str] { &["s", "S", "asm"] }
    fn extract(&self, _relative_path: &str, _source: &str) -> Result<AdapterOutput> {
        // TODO Phase 2
        Ok(AdapterOutput::default())
    }
}
