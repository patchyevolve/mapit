//! Default `mapit` (no subcommand) — map then open.
use std::path::Path;
use anyhow::Result;

pub async fn run(target: &Path) -> Result<()> {
    super::map::run(target, false).await?;
    println!("Run `mapit open` to view the interactive graph (Phase 7).");
    Ok(())
}
