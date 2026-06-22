use std::path::Path;
use anyhow::Result;
pub async fn run(_target: &Path, name: &str, _depth: usize) -> Result<()> {
    println!("mapit trace {name} — Phase 4");
    Ok(())
}
