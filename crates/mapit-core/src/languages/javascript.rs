//! JavaScript / TypeScript language adapters (Phase 2). Stubs.
use anyhow::Result;
use super::{AdapterOutput, LanguageAdapter};

pub struct JavaScriptAdapter;

impl LanguageAdapter for JavaScriptAdapter {
    fn language_id(&self) -> &'static str { "javascript" }
    fn file_extensions(&self) -> &'static [&'static str] { &["js", "mjs", "cjs", "jsx"] }
    fn extract(&self, _relative_path: &str, _source: &str) -> Result<AdapterOutput> {
        // TODO Phase 2
        Ok(AdapterOutput::default())
    }
}

pub struct TypeScriptAdapter;

impl LanguageAdapter for TypeScriptAdapter {
    fn language_id(&self) -> &'static str { "typescript" }
    fn file_extensions(&self) -> &'static [&'static str] { &["ts", "mts", "cts", "tsx"] }
    fn extract(&self, _relative_path: &str, _source: &str) -> Result<AdapterOutput> {
        // TODO Phase 2
        Ok(AdapterOutput::default())
    }
}
