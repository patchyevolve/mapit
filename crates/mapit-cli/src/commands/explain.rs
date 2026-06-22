use std::path::Path;
use anyhow::Result;
pub async fn run(_target: &Path, name: &str) -> Result<()> {
    println!("mapit explain {name} — Phase 4");
    Ok(())
}
