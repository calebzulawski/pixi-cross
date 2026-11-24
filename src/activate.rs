//! Install the selected host environment from the workspace lock and emit shell assignments.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};
use rattler_conda_types::Subdir;

use crate::common::{acquire_directory_lock, manifest::load_authored_project};

mod exports;

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Phase {
    Start,
    Finish,
    Restore,
}

impl Phase {
    pub fn print_shell_changes(self) -> Result<()> {
        match self {
            Self::Start => install_host_and_start_activation(),
            Self::Finish => exports::record_activation_changes(),
            Self::Restore => {
                exports::restore_previous_exports();
                Ok(())
            }
        }
    }
}

struct ActivationContext {
    workspace_root: PathBuf,
    build_prefix: PathBuf,
    host_environment: String,
    build_platform: Subdir,
    target_platform: Subdir,
    package_name: String,
    package_version: String,
}

impl ActivationContext {
    fn from_pixi_environment() -> Result<Self> {
        let manifest = PathBuf::from(env::var("PIXI_PROJECT_MANIFEST")?);
        let workspace_root = manifest
            .parent()
            .context("missing workspace directory")?
            .to_owned();
        let project = load_authored_project(&manifest)?;
        let build_prefix = env::var("CONDA_PREFIX")?.into();
        let name = env::var("PIXI_ENVIRONMENT_NAME")?;
        let build_platform = Subdir::current().context("no conda subdir for the running CLI")?;
        let (host_environment, target_platform) =
            project.host_environment_for_activation(&name, build_platform)?;
        Ok(Self {
            workspace_root,
            build_prefix,
            host_environment,
            build_platform,
            target_platform,
            package_name: env::var("PIXI_PROJECT_NAME")?,
            package_version: env::var("PIXI_PROJECT_VERSION").unwrap_or_default(),
        })
    }

    fn install_locked_host_environment(&self) -> Result<PathBuf> {
        let dir = self
            .workspace_root
            .join(".pixi/host")
            .join(&self.host_environment);
        fs::create_dir_all(&dir)?;
        let _host_install_lock = acquire_directory_lock(&dir)?;
        let prefix = dir.join(self.target_platform.as_str());
        let status = Command::new(self.build_prefix.join("bin/pixi-install-to-prefix"))
            .arg("--lockfile")
            .arg(self.workspace_root.join("pixi.lock"))
            .arg(format!("--environment={}", self.host_environment))
            .arg("--platform")
            .arg(self.target_platform.as_str())
            .arg("--no-activation-scripts")
            .arg(&prefix)
            .stdout(Stdio::from(std::io::stderr()))
            .status()
            .context("starting pixi-install-to-prefix")?;
        ensure!(
            status.success(),
            "installing {} for {} failed ({status})",
            self.host_environment,
            self.target_platform
        );
        Ok(prefix)
    }

    fn activation_variables(self, prefix: &Path) -> Result<Vec<(&'static str, String)>> {
        let path = |p: &Path| {
            p.to_str()
                .map(str::to_owned)
                .context("prefix must be UTF-8")
        };
        let host = path(prefix)?;
        let mut pkg_config = format!("{host}/lib/pkgconfig:{host}/share/pkgconfig");
        if self.target_platform.is_windows() {
            pkg_config =
                format!("{host}/Library/lib/pkgconfig:{host}/Library/share/pkgconfig:{pkg_config}");
        }
        let cross = u8::from(self.build_platform != self.target_platform).to_string();
        Ok(vec![
            ("BUILD_PREFIX", path(&self.build_prefix)?),
            ("PREFIX", host),
            ("build_platform", self.build_platform.to_string()),
            ("host_platform", self.target_platform.to_string()),
            ("target_platform", self.target_platform.to_string()),
            ("SUBDIR", self.target_platform.to_string()),
            ("CONDA_BUILD", "1".into()),
            ("CONDA_BUILD_STATE", "BUILD".into()),
            ("CONDA_BUILD_CROSS_COMPILATION", cross),
            ("SRC_DIR", path(&self.workspace_root)?),
            ("PKG_NAME", self.package_name),
            ("PKG_VERSION", self.package_version),
            ("PKG_CONFIG_LIBDIR", pkg_config),
        ])
    }
}

fn install_host_and_start_activation() -> Result<()> {
    let context = ActivationContext::from_pixi_environment()?;
    let prefix = context.install_locked_host_environment()?;
    exports::save_initial_exports();
    for (key, value) in context.activation_variables(&prefix)? {
        exports::print_export(key, value.as_bytes());
    }
    Ok(())
}
