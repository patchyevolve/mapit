//! Assembly language adapter — x86-64 NASM/GAS-style .asm/.s/.S files.
//!
//! No tree-sitter grammar exists for assembly at the required maturity level,
//! so this adapter uses line-by-line text parsing (TRD §4.2 "reduced scope"):
//!   - Label definitions: `label:` or `GLOBAL label` / `.globl label`
//!   - Call/jmp targets: `call label`, `jmp label`, `jne label`, etc.
//!   - C-compatible symbol detection for cross-language linking (TRD §4.3)
//!
//! This is intentionally limited to structural shape (what labels exist,
//! what jumps to what), not full instruction-level semantics.

use anyhow::Result;

use super::{
    AdapterOutput, LanguageAdapter, SymbolDefinition, SymbolKind, SymbolReference,
};

pub struct AsmAdapter;

impl LanguageAdapter for AsmAdapter {
    fn language_id(&self) -> &'static str {
        "asm"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["s", "S", "asm"]
    }

    fn extract(&self, _relative_path: &str, source: &str) -> Result<AdapterOutput> {
        let mut output = AdapterOutput::default();
        parse_asm(source, &mut output);
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

fn parse_asm(source: &str, output: &mut AdapterOutput) {
    // Track which labels are currently "open" (i.e., we're inside their body).
    // For assembly we treat the region between one label and the next as that
    // label's "body" for the purpose of attributing call/jmp targets.
    let mut current_label: Option<(String, u32)> = None; // (name, start_line)
    let mut call_order: i32 = 0;

    // First pass: collect all label definitions so we know what's local.
    // Second pass (interleaved): emit references.

    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = idx as u32 + 1;
        let line = strip_comment(raw_line).trim();

        if line.is_empty() {
            continue;
        }

        // --- Label definition ---
        // Patterns: `label:`, `.label:`, `LABEL:` (NASM/GAS)
        if let Some(label) = try_parse_label(line) {
            // Close previous label region
            if let Some((prev_name, prev_start)) = current_label.take() {
                // end_line is line before this new label
                let end_line = if line_no > 1 { line_no - 1 } else { line_no };
                push_label_def(output, &prev_name, prev_start, end_line);
            }
            current_label = Some((label, line_no));
            call_order = 0;
            continue;
        }

        // --- .globl / GLOBAL directive ---
        // These export a symbol to the linker — mark as entry point candidate.
        if let Some(sym) = try_parse_global_directive(line) {
            // We may not have seen the label yet; record it as a forward-declared
            // entry point by emitting a minimal definition now (will be merged
            // or shadowed when the real label is encountered).
            output.definitions.push(SymbolDefinition {
                name: sym.clone(),
                qualified_name: sym.clone(),
                kind: SymbolKind::Function,
                start_line: line_no,
                end_line: line_no,
                source_text: raw_line.to_owned(),
                signature: sym.clone(),
                is_entry_point_candidate: true,
            });
            continue;
        }

        // --- call / jmp / jcc instructions ---
        if let Some(target) = try_parse_call_or_jump(line) {
            if let Some((ref caller_name, _)) = current_label {
                output.references.push(SymbolReference {
                    from_qualified_name: caller_name.clone(),
                    called_name: target,
                    call_line: line_no,
                    order_hint: call_order,
                    condition: None,
                });
                call_order += 1;
            }
        }
    }

    // Close the last open label
    if let Some((name, start_line)) = current_label {
        let end_line = source.lines().count() as u32;
        push_label_def(output, &name, start_line, end_line);
    }

    // Deduplicate definitions (a .globl directive + a label line may both
    // emit the same name; keep the one with the real start_line from the label).
    dedup_definitions(output);
}

fn push_label_def(output: &mut AdapterOutput, name: &str, start_line: u32, end_line: u32) {
    // Skip local labels (.L_foo, ..L1, etc.) — they're internal branch targets,
    // not meaningful function-level symbols.
    if is_local_label(name) {
        return;
    }
    // Avoid duplicating if a .globl already added this name
    if output.definitions.iter().any(|d| d.name == name) {
        // Update the existing entry's span
        if let Some(def) = output.definitions.iter_mut().find(|d| d.name == name) {
            def.start_line = start_line;
            def.end_line = end_line;
        }
        return;
    }
    output.definitions.push(SymbolDefinition {
        name: name.to_owned(),
        qualified_name: name.to_owned(),
        kind: SymbolKind::Function,
        start_line,
        end_line,
        source_text: String::new(), // asm bodies are not stored as source text
        signature: name.to_owned(),
        is_entry_point_candidate: is_asm_entry_point(name),
    });
}

fn dedup_definitions(output: &mut AdapterOutput) {
    // Collect which names ever had is_entry_point_candidate = true
    let ep_names: std::collections::HashSet<String> = output
        .definitions
        .iter()
        .filter(|d| d.is_entry_point_candidate)
        .map(|d| d.name.clone())
        .collect();

    // Dedup by name (keep first occurrence)
    let mut seen = std::collections::HashSet::new();
    output.definitions.retain(|d| seen.insert(d.name.clone()));

    // Restore is_entry_point_candidate on the surviving definition
    for def in &mut output.definitions {
        if ep_names.contains(&def.name) {
            def.is_entry_point_candidate = true;
        }
    }
}

// ---------------------------------------------------------------------------
// Line parsers
// ---------------------------------------------------------------------------

/// Strip inline comment (`;` for NASM, `#` for GAS, `//` for some assemblers).
fn strip_comment(line: &str) -> &str {
    // Try `;` first (NASM style), then `#` (GAS style)
    for marker in &[";", "#", "//"] {
        if let Some(pos) = line.find(marker) {
            return &line[..pos];
        }
    }
    line
}

/// If the line is a label definition, return the label name.
/// Handles: `label:`, `.label:`, `label: ; comment`
fn try_parse_label(line: &str) -> Option<String> {
    // Must end with ':' (after stripping whitespace)
    let trimmed = line.trim_end();
    if !trimmed.ends_with(':') {
        return None;
    }
    let name = trimmed.trim_end_matches(':').trim();
    // Must be a valid identifier (not just punctuation)
    if name.is_empty() || name.contains(' ') {
        return None;
    }
    // Skip section directives like `.text:` — they start with `.` but are not
    // function labels; actual function labels starting with `.` are local.
    Some(name.to_owned())
}

/// Detects `.globl sym` or `GLOBAL sym` or `global sym` directives.
fn try_parse_global_directive(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let rest = if lower.starts_with(".globl ") {
        line[".globl ".len()..].trim()
    } else if lower.starts_with("global ") {
        line["global ".len()..].trim()
    } else {
        return None;
    };
    let sym = rest.split(',').next()?.trim().to_owned();
    if sym.is_empty() { None } else { Some(sym) }
}

/// Detects `call target`, `jmp target`, `jne target`, etc.
/// Returns the target label/symbol name.
fn try_parse_call_or_jump(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    // All x86 unconditional + conditional branches + calls
    let prefixes = [
        "call ", "jmp ", "je ", "jne ", "jz ", "jnz ", "jg ", "jge ",
        "jl ", "jle ", "ja ", "jae ", "jb ", "jbe ", "js ", "jns ",
        "jo ", "jno ", "jp ", "jnp ", "jcxz ", "jecxz ", "jrcxz ",
        // Also handle near/far qualifiers: `call near ptr foo`
        "call near ", "call far ",
    ];
    for prefix in &prefixes {
        if lower.starts_with(prefix) {
            let target = line[prefix.len()..].trim();
            // Strip addressing modifiers like `[rax]`, `QWORD PTR [...]`
            let target = target
                .trim_start_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                .split(|c: char| c == ',' || c == ' ' || c == '\t')
                .next()
                .unwrap_or("")
                .trim();
            if !target.is_empty() && !target.starts_with('[') {
                return Some(target.to_owned());
            }
        }
    }
    None
}

/// Local labels are internal branch targets, not function-level symbols.
/// GAS: `.L_foo`, `1:` / `2:` (numeric local labels)
/// NASM: `..@foo`, `.foo`
fn is_local_label(name: &str) -> bool {
    if name.starts_with(".L") || name.starts_with("..") {
        return true;
    }
    // Numeric labels (GAS local labels like `1`, `2`)
    if name.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}

/// Common assembly entry point names.
fn is_asm_entry_point(name: &str) -> bool {
    matches!(
        name,
        "_start"
            | "start"
            | "_main"
            | "main"
            | "kmain"
            | "kernel_entry"
            | "reset_handler"
            | "interrupt_handler"
            | "isr_handler"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str) -> AdapterOutput {
        AsmAdapter.extract("test.s", src).unwrap()
    }

    #[test]
    fn finds_label() {
        let src = "my_func:\n  xor eax, eax\n  ret\n";
        let out = extract(src);
        assert!(out.definitions.iter().any(|d| d.name == "my_func"));
    }

    #[test]
    fn finds_call_target() {
        let src = "caller:\n  call callee\n  ret\ncallee:\n  ret\n";
        let out = extract(src);
        assert!(out.references.iter().any(|r| r.called_name == "callee"));
    }

    #[test]
    fn globl_marks_entry_point() {
        let src = ".globl my_export\nmy_export:\n  ret\n";
        let out = extract(src);
        let def = out.definitions.iter().find(|d| d.name == "my_export").unwrap();
        assert!(def.is_entry_point_candidate);
    }

    #[test]
    fn skips_local_labels() {
        let src = ".Llocal:\n  nop\n  ret\n";
        let out = extract(src);
        assert!(!out.definitions.iter().any(|d| d.name == ".Llocal"));
    }

    #[test]
    fn jmp_is_a_reference() {
        let src = "foo:\n  jmp bar\nbar:\n  ret\n";
        let out = extract(src);
        assert!(out.references.iter().any(|r| r.called_name == "bar"));
    }

    #[test]
    fn start_is_entry_point() {
        let src = "_start:\n  call main\n  ret\n";
        let out = extract(src);
        let s = out.definitions.iter().find(|d| d.name == "_start").unwrap();
        assert!(s.is_entry_point_candidate);
    }

    #[test]
    fn strips_semicolon_comments() {
        let src = "my_fn:  ; this is a function\n  ret\n";
        let out = extract(src);
        assert!(out.definitions.iter().any(|d| d.name == "my_fn"));
    }
}
