use std::path::Path;
use anyhow::Result;
pub async fn run(_target: &Path, question: &str) -> Result<()> {
    println!("mapit ask \"{question}\" — Phase 5");
    Ok(())
}
