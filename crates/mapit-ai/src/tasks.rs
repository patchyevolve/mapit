//! The four AI task implementations following TRD §5.2.
//! Each task: builds prompt → calls provider → parses response → retries once on failure.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::prompts;
use crate::provider::{AiProvider, AiRequest, AiResponse};

// ---------------------------------------------------------------------------
// Task 1: Summarize
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SummarizeOutput {
    pub summary: String,
}

pub fn summarize(
    provider: &dyn AiProvider,
    model: &str,
    name: &str,
    node_type: &str,
    file_path: &str,
    start_line: u32,
    end_line: u32,
    language: &str,
    source_text: &str,
    signature: &str,
    callers: &[String],
    callees: &[String],
) -> Result<SummarizeOutput> {
    let user_prompt = prompts::SUMMARIZE
        .replace("{{name}}", name)
        .replace("{{type}}", node_type)
        .replace("{{file_path}}", file_path)
        .replace("{{start_line}}", &start_line.to_string())
        .replace("{{end_line}}", &end_line.to_string())
        .replace("{{language}}", language)
        .replace("{{source_text}}", source_text)
        .replace("{{signature}}", signature)
        .replace("{{callers}}", &callers.join(", "))
        .replace("{{callees}}", &callees.join(", "));

    let request = AiRequest {
        model: model.to_owned(),
        system_prompt: Some("You are a code analysis assistant. Always return valid JSON.".into()),
        user_prompt,
        expect_json: true,
    };

    let response = try_complete(provider, &request)?;
    // Try strict JSON parse first; fall back to using raw text as the summary.
    // Some smaller/free models return plain text instead of JSON — that’s still
    // useful as a summary and must not mark the node Unavailable.
    match parse_json::<SummarizeOutput>(&response.content) {
        Ok(out) => Ok(out),
        Err(_) => {
            let raw = response.content.trim().to_string();
            if raw.len() > 5 {
                Ok(SummarizeOutput { summary: raw })
            } else {
                anyhow::bail!("summarize: response too short to use as summary")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Task 2: Batch summarize (all functions in a file in one call)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BatchSummarizeOutput {
    pub summaries: Vec<BatchSummaryEntry>,
}

#[derive(Deserialize)]
pub struct BatchSummaryEntry {
    pub name: String,
    pub summary: String,
}

/// Summarize all functions/symbols in a file in a single AI call.
/// `symbol_descs` is one string per symbol (pre-formatted with name, signature, callers, callees).
pub fn summarize_batch(
    provider: &dyn AiProvider,
    model: &str,
    file_path: &str,
    language: &str,
    project_overview: Option<&str>,
    symbol_descs: &[String],
) -> Result<BatchSummarizeOutput> {
    let pv = project_overview.unwrap_or("(no project overview available)");
    let user_prompt = prompts::SUMMARIZE_BATCH
        .replace("{{project_overview}}", pv)
        .replace("{{file_path}}", file_path)
        .replace("{{language}}", language)
        .replace("{{symbols}}", &symbol_descs.join("\n\n"));

    let request = AiRequest {
        model: model.to_owned(),
        system_prompt: Some("You are a code analysis assistant. Always return valid JSON.".into()),
        user_prompt,
        expect_json: true,
    };

    let response = try_complete(provider, &request)?;
    match parse_json::<BatchSummarizeOutput>(&response.content) {
        Ok(out) => Ok(out),
        Err(_) => {
            let raw = response.content.trim().to_string();
            if raw.len() > 10 {
                // Try line-based fallback: "name: summary" per line
                let mut summaries = Vec::new();
                for line in raw.lines() {
                    if let Some((name, summary)) = line.split_once(':') {
                        summaries.push(BatchSummaryEntry {
                            name: name.trim().to_owned(),
                            summary: summary.trim().to_owned(),
                        });
                    }
                }
                if !summaries.is_empty() {
                    return Ok(BatchSummarizeOutput { summaries });
                }
            }
            anyhow::bail!("summarize_batch: could not parse response")
        }
    }
}

// ---------------------------------------------------------------------------
// Task 3: Project overview (run once per annotation pass)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ProjectOverviewOutput {
    pub overview: String,
}

/// Get a high-level project overview that provides system context for all
/// per-function summaries. Single AI call, reused across all batch prompts.
pub fn summarize_project(
    provider: &dyn AiProvider,
    model: &str,
    file_tree: &str,
    entry_points: &str,
    public_symbols: &str,
) -> Result<ProjectOverviewOutput> {
    let user_prompt = prompts::PROJECT_OVERVIEW
        .replace("{{file_tree}}", file_tree)
        .replace("{{entry_points}}", entry_points)
        .replace("{{public_symbols}}", public_symbols);

    let request = AiRequest {
        model: model.to_owned(),
        system_prompt: Some("You are a code analysis assistant. Always return valid JSON.".into()),
        user_prompt,
        expect_json: true,
    };

    let response = try_complete(provider, &request)?;
    match parse_json::<ProjectOverviewOutput>(&response.content) {
        Ok(out) => Ok(out),
        Err(_) => {
            let raw = response.content.trim().to_string();
            if raw.len() > 10 {
                Ok(ProjectOverviewOutput { overview: raw })
            } else {
                anyhow::bail!("project_overview: response too short")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Task 4: Summarize file
// ---------------------------------------------------------------------------

pub fn summarize_file(
    provider: &dyn AiProvider,
    model: &str,
    file_path: &str,
    language: &str,
    symbol_summaries: &[String],
) -> Result<SummarizeOutput> {
    let user_prompt = prompts::SUMMARIZE_FILE
        .replace("{{file_path}}", file_path)
        .replace("{{language}}", language)
        .replace("{{symbols_summaries}}", &symbol_summaries.join("\n"));

    let request = AiRequest {
        model: model.to_owned(),
        system_prompt: Some("You are a code analysis assistant. Always return valid JSON.".into()),
        user_prompt,
        expect_json: true,
    };

    let response = try_complete(provider, &request)?;
    match parse_json::<SummarizeOutput>(&response.content) {
        Ok(out) => Ok(out),
        Err(_) => {
            let raw = response.content.trim().to_string();
            if raw.len() > 5 {
                Ok(SummarizeOutput { summary: raw })
            } else {
                anyhow::bail!("summarize_file: response too short to use as summary")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Task 3: Classify
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ClassifyOutput {
    pub features: Vec<Feature>,
}

#[derive(Deserialize)]
pub struct Feature {
    pub name: String,
    pub confidence: f64,
    pub member_symbol_ids: Vec<String>,
    pub member_file_ids: Vec<String>,
}

pub fn classify(
    provider: &dyn AiProvider,
    model: &str,
    file_list: &str,
    symbol_list: &str,
    clustering_hints: &str,
) -> Result<ClassifyOutput> {
    let user_prompt = prompts::CLASSIFY
        .replace("{{file_list}}", file_list)
        .replace("{{symbol_list}}", symbol_list)
        .replace("{{clustering_hints}}", clustering_hints);

    let request = AiRequest {
        model: model.to_owned(),
        system_prompt: Some("You are a code analysis assistant. Always return valid JSON.".into()),
        user_prompt,
        expect_json: true,
    };

    let response = try_complete(provider, &request)?;
    parse_json(&response.content)
}

#[derive(Deserialize)]
pub struct Flaw {
    pub kind: String,
    pub severity: String,
    pub description: String,
    pub confidence: f64,
    pub basis: String,
}

// ---------------------------------------------------------------------------
// Task 5: Batch flag flaws (all functions in a file in one call)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BatchFlagFlawsOutput {
    /// Flaws keyed by function name
    pub flaws: std::collections::HashMap<String, Vec<Flaw>>,
}

/// Analyze all functions in a file for flaws in a single AI call.
/// `func_descs` is one string per function (pre-formatted with name, source, structural info).
pub fn flag_flaws_batch(
    provider: &dyn AiProvider,
    model: &str,
    file_path: &str,
    language: &str,
    project_overview: Option<&str>,
    func_descs: &[String],
) -> Result<BatchFlagFlawsOutput> {
    let pv = project_overview.unwrap_or("(no project overview available)");
    let user_prompt = prompts::FLAW_FLAGS_BATCH
        .replace("{{project_overview}}", pv)
        .replace("{{file_path}}", file_path)
        .replace("{{language}}", language)
        .replace("{{functions}}", &func_descs.join("\n\n"));

    let request = AiRequest {
        model: model.to_owned(),
        system_prompt: Some("You are a code analysis assistant. Always return valid JSON.".into()),
        user_prompt,
        expect_json: true,
    };

    let response = try_complete(provider, &request)?;
    match parse_json::<BatchFlagFlawsOutput>(&response.content) {
        Ok(out) => Ok(out),
        Err(e) => {
            error!("flag_flaws_batch parse failed (returning empty): {e:#}");
            Ok(BatchFlagFlawsOutput {
                flaws: std::collections::HashMap::new(),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Task 6: Answer
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AnswerOutput {
    pub answer: String,
    pub referenced_node_ids: Vec<String>,
}

pub fn answer(
    provider: &dyn AiProvider,
    model: &str,
    context: &str,
    question: &str,
) -> Result<AnswerOutput> {
    let user_prompt = prompts::ANSWER
        .replace("{{context}}", context)
        .replace("{{question}}", question);

    let request = AiRequest {
        model: model.to_owned(),
        system_prompt: Some("You are a code analysis assistant. Always return valid JSON.".into()),
        user_prompt,
        expect_json: true,
    };

    let response = try_complete(provider, &request)?;
    parse_json(&response.content)
}

// ---------------------------------------------------------------------------
// Task 7: Simulate (function/file/module/project execution behavior)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize)]
pub struct SimulateOutput {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub entry: String,
    #[serde(default)]
    pub inputs: Vec<SimulateIO>,
    #[serde(default)]
    pub steps: Vec<SimulateStep>,
    #[serde(default)]
    pub outputs: Vec<SimulateIO>,
    #[serde(default)]
    pub exit: String,
    #[serde(default)]
    pub errors: Vec<SimulateError>,
}

#[derive(Deserialize, Serialize)]
pub struct SimulateIO {
    pub name: String,
    #[serde(rename = "type")]
    pub io_type: String,
    #[serde(default)]
    pub from_user: String,
    #[serde(default)]
    pub from_system: String,
    #[serde(default)]
    pub to_user: String,
    #[serde(default)]
    pub to_system: String,
    #[serde(default)]
    pub side_effects: String,
}

#[derive(Deserialize, Serialize)]
pub struct SimulateStep {
    pub order: u32,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Deserialize, Serialize)]
pub struct SimulateError {
    #[serde(default)]
    pub condition: String,
    #[serde(default)]
    pub result: String,
}

/// Simulate runtime behavior of a symbol at any level (function/file/module/project).
/// `context` contains pre-formatted structural data appropriate for the level.
pub fn simulate(
    provider: &dyn AiProvider,
    model: &str,
    level: &str,
    name: &str,
    project_overview: Option<&str>,
    context: &str,
) -> Result<SimulateOutput> {
    let pv = project_overview.unwrap_or("(no project overview)");
    let user_prompt = prompts::SIMULATE
        .replace("{{level}}", level)
        .replace("{{name}}", name)
        .replace("{{project_overview}}", pv)
        .replace("{{context}}", context);

    let request = AiRequest {
        model: model.to_owned(),
        system_prompt: Some("You are a code analysis assistant. Always return valid JSON.".into()),
        user_prompt,
        expect_json: true,
    };

    let response = try_complete(provider, &request)?;
    parse_json(&response.content)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Call provider.complete with one retry on parse-failure.
fn try_complete(provider: &dyn AiProvider, request: &AiRequest) -> Result<AiResponse> {
    let result = provider.complete(request.clone());

    match result {
        Ok(resp) => {
            info!("AI complete ok, {} chars", resp.content.len());
            Ok(resp)
        }
        Err(e) => {
            error!("AI complete failed (will not retry): {e:#}");
            anyhow::bail!("AI provider error: {e:#}")
        }
    }
}

/// Parse a JSON response, with one retry logic if it fails.
// ---------------------------------------------------------------------------
// Tests (mocked provider)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{AiProvider, AiRequest, AiResponse, ModelInfo};

    struct MockProvider {
        response: String,
    }

    impl AiProvider for MockProvider {
        fn id(&self) -> &str {
            "mock"
        }
        fn list_models(&self) -> Result<Vec<ModelInfo>> {
            Ok(vec![])
        }
        fn complete(&self, _request: AiRequest) -> Result<AiResponse> {
            Ok(AiResponse {
                content: self.response.clone(),
                model_used: "mock-model".into(),
                finish_reason: Some("stop".into()),
            })
        }
        fn supports_streaming(&self) -> bool {
            false
        }
    }

    #[test]
    fn summarize_parses_valid_json() {
        let provider = MockProvider {
            response: r#"{"summary": "This function computes the answer."}"#.into(),
        };
        let result = summarize(
            &provider,
            "test",
            "compute",
            "Function",
            "src/lib.rs",
            1,
            10,
            "rust",
            "fn compute() {}",
            "fn compute()",
            &[],
            &["other".to_string()],
        );
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().summary,
            "This function computes the answer."
        );
    }

    #[test]
    fn summarize_handles_malformed_json_with_brace_extraction() {
        let provider = MockProvider {
            response: r#"Some text before {"summary": "extracted from braces"} trailing"#.into(),
        };
        let result = summarize(
            &provider,
            "test",
            "foo",
            "Function",
            "src/lib.rs",
            1,
            1,
            "rust",
            "fn foo() {}",
            "fn foo()",
            &[],
            &[],
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().summary, "extracted from braces");
    }

    #[test]
    fn flag_flaws_parses_flaw_list() {
        let provider = MockProvider {
            response: r#"{"flaws": [{"kind": "dead_code", "severity": "warning", "description": "unused function", "confidence": 0.9, "basis": "structural+ai"}]}"#.into(),
        };
        let result = flag_flaws(
            &provider,
            "test",
            "unused",
            "src/lib.rs",
            1,
            1,
            false,
            false,
            "rust",
            "fn unused() {}",
            "fn unused()",
            &[],
            &[],
        );
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.flaws.len(), 1);
        assert_eq!(output.flaws[0].kind, "dead_code");
    }

    #[test]
    fn flag_flaws_returns_empty_when_no_flaws() {
        let provider = MockProvider {
            response: r#"{"flaws": []}"#.into(),
        };
        let result = flag_flaws(
            &provider,
            "test",
            "clean_fn",
            "src/lib.rs",
            1,
            1,
            true,
            false,
            "rust",
            "fn clean_fn() {}",
            "fn clean_fn()",
            &[],
            &[],
        );
        assert!(result.is_ok());
        assert!(result.unwrap().flaws.is_empty());
    }

    #[test]
    fn answer_parses_referenced_node_ids() {
        let provider = MockProvider {
            response:
                r#"{"answer": "It initializes the system.", "referenced_node_ids": ["id1", "id2"]}"#
                    .into(),
        };
        let result = answer(&provider, "test", "context here", "What does init do?");
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.answer, "It initializes the system.");
        assert_eq!(output.referenced_node_ids, vec!["id1", "id2"]);
    }

    #[test]
    fn parse_json_extracts_from_noisy_output() {
        let result = parse_json::<SummarizeOutput>(
            r#"Here is the result: {"summary": "hello"} Hope this helps."#,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().summary, "hello");
    }

    #[test]
    fn parse_json_fails_on_invalid_content() {
        let result = parse_json::<SummarizeOutput>("not json at all");
        assert!(result.is_err());
    }
}

fn parse_json<T: serde::de::DeserializeOwned>(json_str: &str) -> Result<T> {
    // First attempt: direct parse
    match serde_json::from_str::<T>(json_str) {
        Ok(val) => return Ok(val),
        Err(e) => {
            // Try to find and extract a JSON block from the response
            if let Some(start) = json_str.find('{') {
                if let Some(end) = json_str.rfind('}') {
                    let extracted = &json_str[start..=end];
                    match serde_json::from_str::<T>(extracted) {
                        Ok(val) => return Ok(val),
                        Err(e2) => {
                            anyhow::bail!(
                                "JSON parse failed (first: {e}, extracted: {e2}): {}",
                                &json_str[..200.min(json_str.len())]
                            );
                        }
                    }
                }
            }
            anyhow::bail!(
                "JSON parse failed: {e}: {}",
                &json_str[..200.min(json_str.len())]
            );
        }
    }
}
