use std::path::Path;
use anyhow::Result;
pub async fn run(_target: &Path, name: &str) -> Result<()> {
    println!("mapit find {name} — Phase 4 (query layer not yet implemented)");
    Ok(())
}
