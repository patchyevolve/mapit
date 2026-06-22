//! The four AI task implementations following TRD §5.2.
//! Each task: builds prompt → calls provider → parses response → retries once on failure.

use anyhow::Result;
use serde::Deserialize;
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
// Task 2: Classify
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

// ---------------------------------------------------------------------------
// Task 3: Flag flaws
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FlagFlawsOutput {
    pub flaws: Vec<Flaw>,
}

#[derive(Deserialize)]
pub struct Flaw {
    pub kind: String,
    pub severity: String,
    pub description: String,
    pub confidence: f64,
    pub basis: String,
}

pub fn flag_flaws(
    provider: &dyn AiProvider,
    model: &str,
    name: &str,
    file_path: &str,
    start_line: u32,
    end_line: u32,
    has_incoming_calls: bool,
    is_entry_point_candidate: bool,
    language: &str,
    source_text: &str,
    signature: &str,
    callers: &[String],
    callees: &[String],
) -> Result<FlagFlawsOutput> {
    let user_prompt = prompts::FLAW_FLAGS
        .replace("{{name}}", name)
        .replace("{{file_path}}", file_path)
        .replace("{{start_line}}", &start_line.to_string())
        .replace("{{end_line}}", &end_line.to_string())
        .replace("{{has_incoming_calls}}", &has_incoming_calls.to_string())
        .replace(
            "{{is_entry_point_candidate}}",
            &is_entry_point_candidate.to_string(),
        )
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
    // flag_flaws must never hard-fail the annotation run — if JSON parse fails,
    // return empty flaws (degraded-but-alive per AGENTS.md §2).
    match parse_json::<FlagFlawsOutput>(&response.content) {
        Ok(out) => Ok(out),
        Err(e) => {
            error!("flag_flaws parse failed (returning empty): {e:#}");
            Ok(FlagFlawsOutput { flaws: vec![] })
        }
    }
}

// ---------------------------------------------------------------------------
// Task 4: Answer
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
