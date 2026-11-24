//! Platform-specific dependency layers rendered with rattler-build’s Jinja context.
use anyhow::{Context, Result, ensure};
use pixi_build_types::TargetSelector;
use rattler_build_jinja::{Jinja, JinjaConfig};
use rattler_conda_types::Subdir;
use serde::Deserialize;
use serde_with::{Map, serde_as};
use toml::{Table, Value};

use crate::common::platform::validate_platforms;

#[serde_as]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateFeature {
    pub platforms: Option<Vec<Subdir>>,
    #[serde(default)]
    dependencies: Table,
    #[serde(default)]
    #[serde_as(as = "Map<_, _>")]
    target: Vec<(TargetSelector, TargetDependencies)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetDependencies {
    #[serde(default)]
    dependencies: Table,
}

impl TemplateFeature {
    pub fn validate_platforms_and_selectors(&self) -> Result<()> {
        if let Some(platforms) = &self.platforms {
            validate_platforms(platforms)?;
        }
        for (selector, _) in &self.target {
            ensure!(
                !matches!(selector, TargetSelector::Platform(_)),
                "unknown conda platform or OS selector {selector}"
            );
        }
        Ok(())
    }

    fn matching_dependency_tables(&self, platform: Subdir) -> impl Iterator<Item = &Table> {
        // Pixi applies matching selectors in manifest order. Later entries win.
        let overrides = self.target.iter().filter_map(move |(selector, layer)| {
            let matches = match selector {
                TargetSelector::Unix => platform.is_unix(),
                TargetSelector::Linux => platform.is_linux(),
                TargetSelector::Win => platform.is_windows(),
                TargetSelector::MacOs => platform.is_osx(),
                TargetSelector::Subdir(name) | TargetSelector::Platform(name) => {
                    name == platform.as_str()
                }
            };
            matches.then_some(&layer.dependencies)
        });
        std::iter::once(&self.dependencies).chain(overrides)
    }
}

pub struct TemplateContext {
    build: Subdir,
    target: Subdir,
    jinja: Jinja,
}

impl TemplateContext {
    pub fn for_platforms(build: Subdir, target: Subdir) -> Self {
        let mut jinja = Jinja::new(JinjaConfig {
            build_platform: build,
            host_platform: target,
            target_platform: target,
            ..Default::default()
        });
        // Dependency entries in pixi.toml use ordinary {{ ... }} delimiters.
        jinja.env_mut().set_syntax(Default::default());
        // Pixi tracks the manifest and platform inputs when locking these outputs.
        jinja.env_mut().remove_global("env");
        Self {
            build,
            target,
            jinja,
        }
    }

    fn render_text(&self, input: &str) -> Result<String> {
        if ["{{", "{%", "{#"]
            .iter()
            .any(|marker| input.contains(marker))
        {
            Ok(self.jinja.render_str(input)?)
        } else {
            Ok(input.to_owned())
        }
    }

    fn render_value(&self, value: &Value) -> Result<Value> {
        Ok(match value {
            Value::String(s) => Value::String(self.render_text(s)?),
            Value::Array(list) => Value::Array(
                list.iter()
                    .map(|v| self.render_value(v))
                    .collect::<Result<_>>()?,
            ),
            Value::Table(table) => Value::Table(self.render_table(table)?),
            other => other.clone(),
        })
    }

    fn render_table(&self, table: &Table) -> Result<Table> {
        let mut result = Table::new();
        for (name, spec) in table {
            let key = self.render_text(name)?;
            ensure!(
                !key.is_empty() && !result.contains_key(&key),
                "empty or duplicate rendered key {key:?}"
            );
            result.insert(key, self.render_value(spec)?);
        }
        Ok(result)
    }

    pub fn render_dependencies(
        &self,
        feature: &TemplateFeature,
        install_platform: Subdir,
    ) -> Result<Table> {
        let mut result = Table::new();
        for entries in feature.matching_dependency_tables(install_platform) {
            let rendered = self
                .render_table(entries)
                .with_context(|| format!("dependencies ({} -> {})", self.build, self.target))?;
            result.extend(rendered);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_templates_cannot_read_the_process_environment() {
        let context = TemplateContext::for_platforms(Subdir::Linux64, Subdir::LinuxAarch64);
        assert!(context.render_text("{{ env.exists('PATH') }}").is_err());
    }
}
