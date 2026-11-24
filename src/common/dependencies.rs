//! Render template dependencies into the binary specs accepted by the Pixi backend.
use anyhow::{Context, Result, bail, ensure};
use pixi_build_types::{BinaryPackageSpec, NamedSpec, PackageSpec};
use rattler_conda_types::{ChannelUrl, NamedChannelOrUrl, PackageName, VersionSpec};
use rattler_digest::{Md5, Sha256, serde::SerializableHash};
use toml::Value;
use url::Url;

use crate::common::{
    plan::{Output, Plan},
    project::Project,
    template::TemplateContext,
};

pub fn validate_output_dependencies(project: &Project, plan: &Plan) -> Result<()> {
    for output in plan.outputs.values().flatten() {
        render_template_dependencies(project, output, None)?;
    }
    Ok(())
}

/// Pixi supplies canonical solver channels only when it requests outputs.
pub fn render_template_dependencies(
    project: &Project,
    output: &Output,
    channels: Option<&[ChannelUrl]>,
) -> Result<Vec<NamedSpec<PackageSpec>>> {
    let context = TemplateContext::for_platforms(output.build, output.target);
    let mut dependencies = Vec::new();
    for feature in &output.features {
        let entries = context
            .render_dependencies(
                &project.template_features()[feature],
                output.package_platform(),
            )
            .with_context(|| {
                format!("feature {feature}, output {}", output.name.as_normalized())
            })?;
        for (name, value) in entries {
            let name = PackageName::try_from(name)?;
            let spec = parse_binary_spec(value, channels)
                .with_context(|| format!("{feature}.dependencies.{}", name.as_source()))?;
            dependencies.push(NamedSpec {
                name: name.into(),
                spec: spec.into(),
            });
        }
    }
    Ok(dependencies)
}

fn parse_binary_spec(value: Value, channels: Option<&[ChannelUrl]>) -> Result<BinaryPackageSpec> {
    let fields = match value {
        Value::String(version) => return Ok(version.parse::<VersionSpec>()?.into()),
        Value::Table(fields) => fields,
        _ => bail!("expected a version or conda match spec table"),
    };
    let mut spec = BinaryPackageSpec::default();
    for (key, value) in fields {
        match key.as_str() {
            "version" => spec.version = Some(value.try_into()?),
            "build" => spec.build = Some(value.try_into()?),
            "build-number" if value.is_integer() => {
                spec.build_number = Some(value.to_string().parse()?)
            }
            "build-number" => spec.build_number = Some(value.try_into()?),
            "file-name" => spec.file_name = Some(value.try_into()?),
            "extras" => spec.extras = Some(value.try_into()?),
            "flags" => spec.flags = Some(value.try_into()?),
            "channel" => spec.channel = resolve_channel(value.try_into()?, channels)?,
            "subdir" => spec.subdir = Some(value.try_into()?),
            "license" => spec.license = Some(value.try_into()?),
            "url" => spec.url = Some(value.try_into()?),
            "md5" => spec.md5 = Some(value.try_into::<SerializableHash<Md5>>()?.0),
            "sha256" => spec.sha256 = Some(value.try_into::<SerializableHash<Sha256>>()?.0),
            _ => bail!("unsupported binary match spec field {key:?}"),
        }
    }
    Ok(spec)
}

/// Resolve a short channel name against Pixi's actual solver URLs, including custom aliases.
fn resolve_channel(
    channel: NamedChannelOrUrl,
    channels: Option<&[ChannelUrl]>,
) -> Result<Option<Url>> {
    if let NamedChannelOrUrl::Url(url) = channel {
        return Ok(Some(url));
    }
    let name = channel.as_str();
    let Some(channels) = channels else {
        return Ok(None);
    };
    let suffix = format!("/{}", name.trim_end_matches('/'));
    let mut matches = channels
        .iter()
        .filter(|url| url.as_ref().path().trim_end_matches('/').ends_with(&suffix));
    let channel = matches.next().with_context(|| {
        format!("channel {name:?} does not match a workspace channel; supply its full URL")
    })?;
    ensure!(
        matches.next().is_none(),
        "ambiguous channel {name:?}; supply its full URL"
    );
    Ok(Some(channel.clone().into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rattler_conda_types::Subdir;

    #[test]
    fn build_selectors_and_target_templates_produce_the_compiler_spec() {
        let project = Project::from_manifest(
            r#"
            [workspace]
            platforms = ["linux-loongarch64", "win-64"]
            preview = ["pixi-build"]

            [tool.cross.environments.dev]
            build-features = ["tools"]

            [tool.cross.feature.tools.target.linux.dependencies."{{ compiler('cxx') }}"]
            version = "{{ '>=19' if win and is_linux(build_platform) else '*' }}"
            channel = "conda-forge"
        "#,
        )
        .unwrap();

        let plan = Plan::from_project(&project).unwrap();
        let cross = plan.outputs[&Subdir::LinuxLoongArch64]
            .iter()
            .find(|out| out.name.as_normalized() == "cross-dev-win-64")
            .unwrap();
        let mirror = Url::parse("https://mirror.example/conda-forge/").unwrap();
        let deps =
            render_template_dependencies(&project, cross, Some(&[mirror.clone().into()])).unwrap();
        let [compiler] = deps.as_slice() else {
            panic!("expected one compiler, got {deps:?}")
        };
        assert_eq!(compiler.name.as_str(), "vs2022_win-64");
        assert_eq!(
            compiler.spec,
            BinaryPackageSpec {
                version: Some(">=19".parse().unwrap()),
                channel: Some(mirror),
                ..Default::default()
            }
            .into()
        );
    }
}
