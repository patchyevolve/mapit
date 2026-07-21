//! Core data model and graph construction for the mapit codebase mapper.
//!
//! This crate provides:
//! - **`walker`** — filesystem walker that discovers source files by extension
//! - **`languages`** — tree-sitter-based language adapters (Rust, C, C++, Python, JS/TS, ASM)
//! - **`graph`** — graph builder, incremental manifest, SQLite-backed node/edge store
//! - **`config`** — global and project-local configuration file I/O
//! - **`control_flow`** — block/branch/loop extraction from tree-sitter CSTs

pub mod config;
pub mod control_flow;
pub mod graph;
pub mod languages;
pub mod walker;
