//! Language adapter trait and registry.
//! Each supported language implements `LanguageAdapter` — adding a new language
//! means implementing this trait only, never modifying core engine logic (TRD §4.1).

pub mod asm;
pub mod c;
pub mod cpp;
pub mod javascript;
pub mod python;
pub mod rust;

use anyhow::Result;

// ---------------------------------------------------------------------------
// Output types produced by adapters
// ---------------------------------------------------------------------------

/// A symbol definition extracted from a source file.
#[derive(Debug, Clone)]
pub struct SymbolDefinition {
    /// Simple name (e.g. "parse_header").
    pub name: String,
    /// Fully-qualified name including enclosing scope (e.g. "Parser::parse_header").
    /// Used for node ID computation and scoped resolution.
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub start_line: u32,
    pub end_line: u32,
    /// The raw source text of this definition (used to compute structural_hash).
    pub source_text: String,
    /// Best-effort textual signature as written in source.
    pub signature: String,
    /// Heuristic: is this a likely entry point? (main, pub fn, exported symbol, etc.)
    pub is_entry_point_candidate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Type,
    Macro,
    Global,
    Module,
}

/// A call or use reference from one symbol to another.
#[derive(Debug, Clone)]
pub struct SymbolReference {
    /// Qualified name of the *caller* (the enclosing function/method).
    pub from_qualified_name: String,
    /// Name as written at the call site (unresolved).
    pub called_name: String,
    /// Source position of the call site.
    pub call_line: u32,
    /// Sequential order within the caller body (for execution-order reconstruction).
    pub order_hint: i32,
    /// Condition string if this call is inside a branch, e.g. "if (status == ERROR)".
    pub condition: Option<String>,
}

/// An import/include/use statement in a file.
#[derive(Debug, Clone)]
pub struct ImportStatement {
    pub kind: ImportKind,
    /// The path or module name as written (e.g. "stdio.h", "crate::foo::bar").
    pub target: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportKind {
    Include,  // C/C++ #include
    Use,      // Rust `use`
    Import,   // Python import / JS import
    Require,  // CommonJS require()
    LinksInto, // from build-file parsing
}

/// Everything a language adapter extracts from one source file.
#[derive(Debug, Default)]
pub struct AdapterOutput {
    pub definitions: Vec<SymbolDefinition>,
    pub references: Vec<SymbolReference>,
    pub imports: Vec<ImportStatement>,
}

// ---------------------------------------------------------------------------
// The adapter trait (TRD §4.1)
// ---------------------------------------------------------------------------

pub trait LanguageAdapter: Send + Sync {
    /// Short language identifier, e.g. "rust", "c".
    fn language_id(&self) -> &'static str;

    /// File extensions this adapter handles (lowercase, without dot).
    fn file_extensions(&self) -> &'static [&'static str];

    /// Extract all definitions, references, and imports from source.
    ///
    /// Must never panic on malformed input — return an error instead.
    /// A parse failure here degrades to `ParseStatus::ParseFailed` for
    /// the file; it must never abort the whole mapping run (TRD §9).
    fn extract(&self, relative_path: &str, source: &str) -> Result<AdapterOutput>;

    /// Whether this adapter supports control-flow extraction (Phase 3).
    /// Default: false. Override in Rust and C adapters.
    fn supports_cfg(&self) -> bool {
        false
    }

    /// The CfgLanguage variant for this adapter (only meaningful if
    /// `supports_cfg()` returns true).
    fn cfg_language(&self) -> Option<crate::control_flow::CfgLanguage> {
        None
    }
}

// ---------------------------------------------------------------------------
// Registry — returns the right adapter for a language id
// ---------------------------------------------------------------------------

pub fn adapter_for_language(language: &str) -> Option<Box<dyn LanguageAdapter>> {
    match language {
        "rust" => Some(Box::new(rust::RustAdapter)),
        "c" => Some(Box::new(c::CAdapter)),
        "cpp" => Some(Box::new(cpp::CppAdapter)),
        "asm" => Some(Box::new(asm::AsmAdapter)),
        "python" => Some(Box::new(python::PythonAdapter)),
        "javascript" | "jsx" => Some(Box::new(javascript::JavaScriptAdapter)),
        "typescript" | "tsx" => Some(Box::new(javascript::TypeScriptAdapter)),
        _ => None,
    }
}
