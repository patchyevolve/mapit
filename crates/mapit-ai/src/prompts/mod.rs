//! Versioned prompt templates — stored as separate text files, embedded at compile time.
//! Each template uses `{{placeholder}}` syntax for substitution.

pub static SUMMARIZE: &str = include_str!("summarize.txt");
pub static CLASSIFY: &str = include_str!("classify.txt");
pub static FLAW_FLAGS: &str = include_str!("flag_flaws.txt");
pub static ANSWER: &str = include_str!("answer.txt");
