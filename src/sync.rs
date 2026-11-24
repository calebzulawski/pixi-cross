//! Synchronize the workspace registrations, helper manifest and Pixi lockfile.
mod manifest;
#[cfg(test)]
mod tests;

use crate::common::{acquire_directory_lock, manifest::TAG};
use anyhow::{Context, Result, ensure};
use clap::Args;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::Builder;

#[derive(Args)]
pub struct Options {
    #[arg(short, long)]
    manifest_path: Option<PathBuf>,
    /// Verify registrations and lockfile without changing them.
    #[arg(long)]
    check: bool,
    /// Skip the pixi lock step.
    #[arg(long)]
    no_lock: bool,
}

/// Sync both manifests before asking Pixi to check or update the lockfile.
pub fn run_sync_command(options: Options) -> Result<()> {
    let path = find_workspace_manifest(options.manifest_path)?;
    let root = path.parent().context("manifest must have a parent")?;
    let helper = root.join(manifest::HELPER_DIR).join("pixi.toml");
    {
        let _workspace_lock = acquire_directory_lock(root)?;
        check_or_write_manifests(&path, &helper, options.check)?;
    }
    if !options.no_lock {
        run_pixi_lock(&path, options.check)?;
    }
    Ok(())
}

/// Use the supplied file or directory, or search upward for pixi.toml.
fn find_workspace_manifest(path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(mut path) = path {
        if path.is_dir() {
            path.push("pixi.toml");
        }
        return Ok(path.canonicalize()?);
    }
    std::env::current_dir()?
        .ancestors()
        .map(|p| p.join("pixi.toml"))
        .find(|p| p.is_file())
        .context("could not find pixi.toml in this directory or its parents")
}

/// Generate both manifests and verify the helper belongs to pixi-cross before writing either file.
fn check_or_write_manifests(workspace: &Path, helper: &Path, check_only: bool) -> Result<()> {
    let original = fs::read_to_string(workspace)?;
    let workspace_toml = manifest::render_workspace_manifest(&original)?;
    let existing = read_generated_helper(helper)?;
    let helper_toml = manifest::render_helper_manifest();
    check_or_replace_file(helper, existing.as_deref(), &helper_toml, check_only)?;
    check_or_replace_file(workspace, Some(&original), &workspace_toml, check_only)
}

/// Read the previous helper if it exists and check that pixi-cross generated it.
fn read_generated_helper(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let header = text.lines().next().unwrap_or_default();
            let marker = header.strip_prefix("[package]").map(str::trim);
            ensure!(
                marker == Some(TAG),
                "refusing to replace authored {}",
                path.display()
            );
            Ok(Some(text))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Report a stale file in check mode or replace it while preserving its permissions.
fn check_or_replace_file(
    path: &Path,
    previous: Option<&str>,
    expected: &str,
    check_only: bool,
) -> Result<()> {
    if previous == Some(expected) {
        return Ok(());
    }
    ensure!(
        !check_only,
        "generated files are stale; run pixi cross sync"
    );

    let parent = path.parent().unwrap();
    if previous.is_none() {
        fs::create_dir_all(parent)?;
    }
    let mut builder = Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o666));
    }
    let mut file = builder.tempfile_in(parent)?;
    file.write_all(expected.as_bytes())?;
    if previous.is_some() {
        file.as_file()
            .set_permissions(fs::metadata(path)?.permissions())?;
    }
    file.persist(path)?;
    Ok(())
}

/// Run `pixi lock` for the workspace, passing `--check` when only verifying it.
fn run_pixi_lock(workspace: &Path, check_only: bool) -> Result<()> {
    let exe = std::env::var_os("PIXI_EXE").unwrap_or_else(|| "pixi".into());
    let mut cmd = Command::new(exe);
    cmd.arg("lock").arg("--manifest-path").arg(workspace);
    if check_only {
        cmd.arg("--check");
    }
    ensure!(
        cmd.status().context("starting pixi lock")?.success(),
        "pixi lock failed"
    );
    Ok(())
}
