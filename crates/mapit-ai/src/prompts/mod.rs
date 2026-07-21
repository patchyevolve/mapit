//! Versioned prompt templates — stored as separate text files, embedded at compile time.
//! Each template uses `{{placeholder}}` syntax for substitution.

pub static SUMMARIZE: &str = include_str!("summarize.txt");
pub static SUMMARIZE_BATCH: &str = include_str!("summarize_batch.txt");
pub static SUMMARIZE_FILE: &str = include_str!("summarize_file.txt");
pub static CLASSIFY: &str = include_str!("classify.txt");
pub static FLAW_FLAGS: &str = include_str!("flag_flaws.txt");
pub static ANSWER: &str = include_str!("answer.txt");
pub static FLAW_FLAGS_BATCH: &str = include_str!("flag_flaws_batch.txt");
pub static SIMULATE: &str = include_str!("simulate.txt");
pub static PROJECT_OVERVIEW: &str = include_str!("project_overview.txt");
