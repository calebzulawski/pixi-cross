//! Directory locking, workspace configuration, and dependency metadata shared by the commands.
use std::{fs::File, io, path::Path};

pub mod dependencies;
pub mod manifest;
pub mod plan;
pub mod platform;
pub mod project;
pub mod template;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Hold an exclusive lock on the directory until the caller drops the returned file.
pub(crate) fn acquire_directory_lock(directory: &Path) -> io::Result<File> {
    let lock = File::open(directory)?;
    lock.lock()?;
    Ok(lock)
}
