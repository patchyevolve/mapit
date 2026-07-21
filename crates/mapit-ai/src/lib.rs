//! AI enrichment layer for the mapit codebase mapper.
//!
//! Provides:
//! - **`provider`** — `AiProvider` trait for abstracting over LLM backends
//! - **`ollama`** — provider for local Ollama instances
//! - **`openai_compatible`** — provider for any OpenAI-compatible API (OpenRouter, etc.)
//! - **`tasks`** — high-level AI tasks: batch summarization, flaw detection, simulation, project overview
//! - **`prompts`** — embedded prompt templates with `{{placeholder}}` substitution

pub mod ollama;
pub mod openai_compatible;
pub mod prompts;
pub mod provider;
pub mod tasks;
