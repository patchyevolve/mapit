//! C++ language adapter (Phase 2). Stub.
use anyhow::Result;
use super::{AdapterOutput, LanguageAdapter};

pub struct CppAdapter;

impl LanguageAdapter for CppAdapter {
    fn language_id(&self) -> &'static str { "cpp" }
    fn file_extensions(&self) -> &'static [&'static str] { &["cpp", "cc", "cxx", "hpp", "hh", "hxx"] }
    fn extract(&self, _relative_path: &str, _source: &str) -> Result<AdapterOutput> {
        // TODO Phase 2
        Ok(AdapterOutput::default())
    }
}
