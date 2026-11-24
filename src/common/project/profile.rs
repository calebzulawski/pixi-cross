//! Resolve the build and host roles of one configured cross environment.
use anyhow::{Context, Result, ensure};
use rattler_conda_types::Subdir;

use super::{Project, Roles};
use crate::common::template::TemplateContext;

impl Project {
    pub fn environment_profiles(&self) -> impl Iterator<Item = Result<Profile<'_>>> {
        self.tool
            .cross
            .environments
            .iter()
            .map(|(name, roles)| Profile::from_feature_roles(self, name, roles))
    }

    /// Resolve only the profile named by the active target-specific environment.
    pub fn host_environment_for_activation(
        &self,
        environment: &str,
        build: Subdir,
    ) -> Result<(String, Subdir)> {
        for (name, roles) in &self.tool.cross.environments {
            let Some(target) = environment
                .strip_prefix(name)
                .and_then(|suffix| suffix.strip_prefix('-'))
                .and_then(|suffix| suffix.parse::<Subdir>().ok())
            else {
                continue;
            };
            let profile = Profile::from_feature_roles(self, name, roles)?;
            ensure!(
                profile.targets.contains(&target),
                "{name} does not target {target}"
            );
            ensure!(
                profile.builds.contains(&build),
                "{name} does not support build platform {build}"
            );
            return Ok((profile.host_environment_name(build), target));
        }
        anyhow::bail!("no [tool.cross.environments] entry matches Pixi environment {environment:?}")
    }
}

pub struct Profile<'a> {
    pub name: &'a str,
    pub roles: &'a Roles,
    pub builds: Vec<Subdir>,
    pub targets: Vec<Subdir>,
    split_host: bool,
}

impl<'a> Profile<'a> {
    fn from_feature_roles(project: &Project, name: &'a str, roles: &'a Roles) -> Result<Self> {
        ensure!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "invalid environment name {name:?}"
        );
        let builds = project
            .common_feature_platforms(&roles.build_features)
            .with_context(|| format!("{name}.build-features"))?;
        let targets = project
            .common_feature_platforms(&roles.host_features)
            .with_context(|| format!("{name}.host-features"))?;
        let split_host =
            host_dependencies_vary_by_build(project, &roles.host_features, &builds, &targets)?;
        Ok(Self {
            name,
            roles,
            builds,
            targets,
            split_host,
        })
    }

    pub fn cross_environment_name(&self, target: Subdir) -> String {
        format!("{}-{target}", self.name)
    }

    /// One build supplies the template context when the host environment is shared.
    pub fn build_platforms_for_host_environments(&self) -> &[Subdir] {
        if self.split_host {
            &self.builds
        } else {
            &self.builds[..1]
        }
    }

    pub fn host_environment_name(&self, build: Subdir) -> String {
        if self.split_host {
            format!("{}-host-dependencies-{build}", self.name)
        } else {
            format!("{}-host-dependencies", self.name)
        }
    }

    pub fn native_environment_features(&self) -> Vec<String> {
        let mut features = self.roles.build_features.clone();
        for name in &self.roles.host_features {
            if !features.contains(name) {
                features.push(name.clone());
            }
        }
        features
    }
}

fn host_dependencies_vary_by_build(
    project: &Project,
    names: &[String],
    builds: &[Subdir],
    targets: &[Subdir],
) -> Result<bool> {
    if builds.len() < 2 {
        return Ok(false);
    }
    for feature in names
        .iter()
        .filter_map(|n| project.template_features().get(n))
    {
        for &target in targets {
            let render = |build: Subdir| {
                TemplateContext::for_platforms(build, target).render_dependencies(feature, target)
            };
            let first = render(builds[0])?;
            for &build in &builds[1..] {
                if render(build)? != first {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
