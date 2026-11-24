//! Validate platform lists used to construct the compilation matrix.
use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use rattler_conda_types::Subdir;

pub fn validate_platforms(platforms: &[Subdir]) -> Result<()> {
    ensure!(!platforms.is_empty(), "expected at least one platform");
    let mut seen = BTreeSet::new();
    for &platform in platforms {
        ensure!(seen.insert(platform), "duplicate platform {platform}");
    }
    Ok(())
}
