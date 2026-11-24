//! Generated Pixi environments and helper package outputs for a project.
use crate::common::project::{Profile, Project};
use anyhow::{Context, Result, ensure};
use rattler_conda_types::{PackageName, Subdir};
use std::collections::BTreeMap;

pub fn native_template_output_name(feature: &str) -> Result<PackageName> {
    PackageName::try_from(format!("native-{feature}"))
        .with_context(|| format!("naming the native package for template feature {feature:?}"))
}

#[derive(PartialEq, Eq)]
pub enum OutputKind {
    Native,
    Cross,
    Host,
}

pub struct Output {
    pub name: PackageName,
    pub features: Vec<String>,
    pub build: Subdir,
    pub target: Subdir,
    pub kind: OutputKind,
}

impl Output {
    pub fn package_platform(&self) -> Subdir {
        match self.kind {
            OutputKind::Native | OutputKind::Cross => self.build,
            OutputKind::Host => self.target,
        }
    }
}

#[derive(Default)]
pub struct Environment {
    pub platforms: Vec<Subdir>,
    pub features: Vec<String>,
    pub output: Option<PackageName>,
    pub no_default_feature: bool,
}

#[derive(Default)]
pub struct Plan {
    pub environments: BTreeMap<String, Environment>,
    pub outputs: BTreeMap<Subdir, Vec<Output>>,
}

impl Plan {
    pub fn from_project(project: &Project) -> Result<Self> {
        let mut plan = Self::default();
        plan.add_native_template_outputs(project)?;
        for profile in project.environment_profiles() {
            let profile = profile?;
            plan.add_native_environment(&profile)?;
            plan.add_cross_environments(project, &profile)?;
            for &build in profile.build_platforms_for_host_environments() {
                plan.add_host_environment(project, &profile, build)?;
            }
        }
        if let Some(authored) = &project.environments
            && let Some(name) = plan
                .environments
                .keys()
                .find(|n| authored.get_ref().contains_key(*n))
        {
            anyhow::bail!("generated environment {name:?} conflicts with an authored environment");
        }
        Ok(plan)
    }

    fn add_native_template_outputs(&mut self, project: &Project) -> Result<()> {
        for (name, feature) in project.template_features() {
            let output = native_template_output_name(name)?;
            for &platform in feature
                .platforms
                .as_ref()
                .unwrap_or(&project.workspace.platforms)
            {
                self.index_output_by_platform(Output {
                    name: output.clone(),
                    features: vec![name.clone()],
                    build: platform,
                    target: platform,
                    kind: OutputKind::Native,
                })?;
            }
        }
        Ok(())
    }

    fn add_native_environment(&mut self, profile: &Profile<'_>) -> Result<()> {
        let native: Vec<_> = profile
            .builds
            .iter()
            .filter(|p| profile.targets.contains(p))
            .copied()
            .collect();
        if !native.is_empty() {
            self.insert_environment(
                profile.name.into(),
                Environment {
                    features: profile.native_environment_features(),
                    platforms: native,
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    }

    fn add_cross_environments(&mut self, project: &Project, profile: &Profile<'_>) -> Result<()> {
        let (templates, ordinary) =
            split_template_and_pixi_features(project, &profile.roles.build_features);
        for &target in &profile.targets {
            let environment = profile.cross_environment_name(target);
            let output = PackageName::try_from(format!("cross-{environment}"))?;
            self.insert_environment(
                environment,
                Environment {
                    platforms: profile.builds.clone(),
                    features: ordinary.clone(),
                    output: Some(output.clone()),
                    no_default_feature: false,
                },
            )?;
            for &build in &profile.builds {
                self.index_output_by_platform(Output {
                    name: output.clone(),
                    features: templates.clone(),
                    build,
                    target,
                    kind: OutputKind::Cross,
                })?;
            }
        }
        Ok(())
    }

    fn add_host_environment(
        &mut self,
        project: &Project,
        profile: &Profile<'_>,
        build: Subdir,
    ) -> Result<()> {
        let (templates, ordinary) =
            split_template_and_pixi_features(project, &profile.roles.host_features);
        let name = profile.host_environment_name(build);
        let output = (!templates.is_empty())
            .then(|| PackageName::try_from(format!("host-{name}")))
            .transpose()?;
        if let Some(output) = &output {
            for &target in &profile.targets {
                self.index_output_by_platform(Output {
                    name: output.clone(),
                    features: templates.clone(),
                    build,
                    target,
                    kind: OutputKind::Host,
                })?;
            }
        }
        self.insert_environment(
            name,
            Environment {
                platforms: profile.targets.clone(),
                features: ordinary,
                output,
                no_default_feature: true,
            },
        )
    }

    fn insert_environment(&mut self, name: String, env: Environment) -> Result<()> {
        ensure!(
            self.environments.insert(name.clone(), env).is_none(),
            "generated environment name collision: {name}"
        );
        Ok(())
    }

    fn index_output_by_platform(&mut self, output: Output) -> Result<()> {
        let platform = output.package_platform();
        let list = self.outputs.entry(platform).or_default();
        ensure!(
            !list.iter().any(|item| item.name == output.name),
            "duplicate helper output {} on {platform}",
            output.name.as_normalized()
        );
        list.push(output);
        Ok(())
    }
}

fn split_template_and_pixi_features(
    project: &Project,
    features: &[String],
) -> (Vec<String>, Vec<String>) {
    features
        .iter()
        .cloned()
        .partition(|name| project.template_features().contains_key(name))
}
