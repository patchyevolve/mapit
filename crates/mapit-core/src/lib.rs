//! Types, graph building, and filesystem walking for mapit.
//!
//! The walker discovers source files, language adapters parse them with
//! tree-sitter, the graph module builds and stores the dependency graph,
//! config handles I/O for settings, and control_flow extracts block and
//! branch structure from CSTs.

pub mod config;
pub mod control_flow;
pub mod graph;
pub mod languages;
pub mod walker;
