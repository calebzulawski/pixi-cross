//! Authored Pixi workspace and cross-compilation configuration.
use anyhow::{Context, Result, ensure};
use rattler_conda_types::Subdir;
use serde::Deserialize;
use serde::de::IgnoredAny;
use std::collections::{BTreeMap, BTreeSet};
use toml::Spanned;

use crate::common::{platform::validate_platforms, template::TemplateFeature};

mod profile;
pub use profile::Profile;

#[derive(Deserialize)]
pub struct Project {
    pub workspace: Workspace,
    #[serde(default, rename = "feature")]
    ordinary: BTreeMap<String, OrdinaryFeature>,
    // Sync edits this span and checks name collisions. Pixi interprets the entries.
    #[serde(default)]
    pub environments: Option<Spanned<BTreeMap<String, IgnoredAny>>>,
    tool: Tools,
}

#[derive(Deserialize)]
pub struct Workspace {
    pub platforms: Vec<Subdir>,
    #[serde(default)]
    preview: Option<toml::Value>,
}

#[derive(Deserialize)]
struct OrdinaryFeature {
    platforms: Option<Vec<Subdir>>,
}

#[derive(Deserialize)]
struct Tools {
    cross: Cross,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cross {
    #[serde(default, rename = "feature")]
    templates: BTreeMap<String, TemplateFeature>,
    environments: BTreeMap<String, Roles>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Roles {
    #[serde(default)]
    pub build_features: Vec<String>,
    #[serde(default)]
    pub host_features: Vec<String>,
}

impl Workspace {
    fn validate_platforms_and_preview(&self) -> Result<()> {
        validate_platforms(&self.platforms)?;
        let pixi_build = match &self.preview {
            Some(toml::Value::Boolean(enabled)) => *enabled,
            Some(toml::Value::Array(entries)) => {
                entries.iter().any(|v| v.as_str() == Some("pixi-build"))
            }
            _ => false,
        };
        ensure!(
            pixi_build,
            "enable pixi-build in workspace.preview for [dev] packages"
        );
        Ok(())
    }
}

impl Project {
    pub fn from_manifest(authored: &str) -> Result<Self> {
        let project: Self = toml::from_str(authored)
            .context("reading [workspace] and [tool.cross] in pixi.toml")?;
        project.workspace.validate_platforms_and_preview()?;
        for (name, template) in project.template_features() {
            project.validate_template_feature(name, template)?;
        }
        Ok(project)
    }

    fn validate_template_feature(&self, name: &str, template: &TemplateFeature) -> Result<()> {
        ensure!(
            name != "default",
            "default is reserved for Pixi's default feature"
        );
        ensure!(
            !self.ordinary.contains_key(name),
            "template feature {name:?} conflicts with an authored Pixi feature"
        );
        template
            .validate_platforms_and_selectors()
            .with_context(|| format!("tool.cross.feature.{name}"))?;
        if let Some(platforms) = &template.platforms {
            ensure!(
                platforms
                    .iter()
                    .all(|p| self.workspace.platforms.contains(p)),
                "tool.cross.feature.{name} names a platform outside workspace.platforms"
            );
        }
        Ok(())
    }

    pub fn template_features(&self) -> &BTreeMap<String, TemplateFeature> {
        &self.tool.cross.templates
    }

    pub fn common_feature_platforms(&self, names: &[String]) -> Result<Vec<Subdir>> {
        let mut seen = BTreeSet::new();
        let mut result = self.workspace.platforms.clone();
        for name in names {
            ensure!(seen.insert(name), "duplicate feature {name:?}");
            let restriction = if let Some(f) = self.template_features().get(name) {
                &f.platforms
            } else {
                &self
                    .ordinary
                    .get(name)
                    .with_context(|| format!("unknown feature {name:?}"))?
                    .platforms
            };
            if let Some(list) = restriction {
                result.retain(|p| list.contains(p));
            }
        }
        ensure!(
            !result.is_empty(),
            "selected features have no platform in common"
        );
        Ok(result)
    }
}
