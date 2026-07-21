//! LLM integration for enriching codebase analysis.
//!
//! Defines the `AiProvider` trait, providers for Ollama and
//! OpenAI-compatible APIs, high-level tasks (summarization, flaw
//! detection, simulation), and embedded prompt templates.

pub mod ollama;
pub mod openai_compatible;
pub mod prompts;
pub mod provider;
pub mod tasks;
