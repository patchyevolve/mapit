//! Python language adapter (Phase 2). Stub.
use anyhow::Result;
use super::{AdapterOutput, LanguageAdapter};

pub struct PythonAdapter;

impl LanguageAdapter for PythonAdapter {
    fn language_id(&self) -> &'static str { "python" }
    fn file_extensions(&self) -> &'static [&'static str] { &["py"] }
    fn extract(&self, _relative_path: &str, _source: &str) -> Result<AdapterOutput> {
        // TODO Phase 2
        Ok(AdapterOutput::default())
    }
}
