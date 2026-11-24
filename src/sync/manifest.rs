//! Generate the Pixi environments, features, and helper manifest for a workspace.
use anyhow::{Context, Result};
use rattler_conda_types::{PackageName, Subdir};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value};

use crate::common::{
    VERSION,
    dependencies::validate_output_dependencies,
    manifest::{TAG, strip_generated_entries},
    plan::{Environment, Plan, native_template_output_name},
    project::Project,
};
mod environments;

pub(super) const HELPER_DIR: &str = ".pixi-cross";

/// Rebuild the marked parts of pixi.toml from the workspace's cross features and environments.
pub(super) fn render_workspace_manifest(original: &str) -> Result<String> {
    let mut source = strip_generated_entries(original)?.into_owned();
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let project = Project::from_manifest(&source)?;
    let plan = Plan::from_project(&project)?;
    validate_output_dependencies(&project, &plan)?;
    let marker = format!("  {TAG}");
    let mut entries = Table::new();
    for (name, env) in &plan.environments {
        let mut spec = encode_environment_entry(&project, env)?;
        spec.decor_mut().set_suffix(marker.as_str());
        entries.insert(name, Item::Value(spec));
    }
    let span = project.environments.as_ref().map(|envs| envs.span());
    environments::insert_environment_entries(&mut source, span, entries, newline)?;
    let features = encode_native_features(&project, &marker)?;
    append_table(&mut source, "feature", features, newline);
    if source != original {
        toml_edit::Document::parse(source.as_str())
            .context("adding Pixi registrations to the authored manifest")?;
    }
    Ok(source)
}

/// Write just the feature names when the environment can inherit all its other settings.
fn encode_environment_entry(project: &Project, env: &Environment) -> Result<Value> {
    let features: Array = env.features.iter().collect();
    let inherited_platforms = env.platforms == project.common_feature_platforms(&env.features)?;
    if inherited_platforms && !env.no_default_feature && env.output.is_none() {
        return Ok(features.into());
    }
    let mut spec = InlineTable::new();
    if !features.is_empty() {
        spec.insert("features", features.into());
    }
    if !inherited_platforms {
        spec.insert(
            "platforms",
            env.platforms
                .iter()
                .copied()
                .map(Subdir::as_str)
                .collect::<Array>()
                .into(),
        );
    }
    if env.no_default_feature {
        spec.insert("no-default-feature", true.into());
    }
    if let Some(output) = &env.output {
        spec.insert("dev", helper_path_dependency(output));
    }
    Ok(spec.into())
}

/// Give each template a Pixi feature that selects build tools for the current machine.
fn encode_native_features(project: &Project, marker: &str) -> Result<Table> {
    let mut features = Table::new();
    features.set_implicit(true);
    for (name, template) in project.template_features() {
        let mut feature = Table::new();
        feature.decor_mut().set_suffix(marker);
        if let Some(platforms) = &template.platforms {
            feature["platforms"] = toml_edit::value(
                platforms
                    .iter()
                    .copied()
                    .map(Subdir::as_str)
                    .collect::<Array>(),
            );
        }
        feature["dev"] = Item::Value(helper_path_dependency(&native_template_output_name(name)?));
        features[name] = Item::Table(feature);
    }
    Ok(features)
}

/// Point `dev.<output>.path` at the helper manifest's directory.
fn helper_path_dependency(output: &PackageName) -> Value {
    let mut path = InlineTable::from_iter([("path", HELPER_DIR)]);
    path.set_dotted(true);
    let mut dev = InlineTable::from_iter([(output.as_normalized(), path)]);
    dev.set_dotted(true);
    dev.into()
}

/// Pin the helper's build backend to the running CLI version.
pub(super) fn render_helper_manifest() -> String {
    include_str!("manifest/helper.toml.in").replace("@VERSION@", VERSION)
}

/// Append a nonempty TOML table after a blank line using the workspace's line endings.
fn append_table(source: &mut String, name: &str, table: Table, newline: &str) {
    if table.is_empty() {
        return;
    }
    if !source.ends_with('\n') {
        source.push_str(newline);
    }
    if !source.ends_with(&newline.repeat(2)) {
        source.push_str(newline);
    }
    let section = DocumentMut::from(Table::from_iter([(name, table)]));
    source.push_str(&section.to_string().replace('\n', newline));
}
