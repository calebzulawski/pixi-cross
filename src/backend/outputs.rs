//! Describe the helper packages available on the requested platform.
use std::path::Path;

use anyhow::Result;
use pixi_build_types::{
    BinaryPackageSpec, NamedSpec,
    procedures::conda_outputs::{
        CondaOutput, CondaOutputDependencies, CondaOutputMetadata, CondaOutputsParams,
        CondaOutputsResult,
    },
};
use rattler_conda_types::{ChannelUrl, PackageName, VersionSpec};

use crate::common::{
    VERSION,
    dependencies::render_template_dependencies,
    manifest::load_authored_project,
    plan::{Output, OutputKind, Plan},
    project::Project,
};

pub(super) fn describe_conda_outputs(
    manifest: &Path,
    params: CondaOutputsParams,
) -> Result<CondaOutputsResult> {
    let project = load_authored_project(manifest)?;
    let plan = Plan::from_project(&project)?;
    let outputs = plan
        .outputs
        .get(&params.host_platform)
        .into_iter()
        .flatten()
        .map(|output| conda_output(&project, output, &params.channels))
        .collect::<Result<_>>()?;
    Ok(CondaOutputsResult {
        outputs,
        input_globs: vec!["pixi.toml".into()],
        input_glob_sets: None,
    })
}

fn conda_output(
    project: &Project,
    output: &Output,
    channels: &[ChannelUrl],
) -> Result<CondaOutput> {
    let mut depends = render_template_dependencies(project, output, Some(channels))?;
    if output.kind == OutputKind::Cross {
        depends.push(NamedSpec {
            name: PackageName::try_from("pixi-cross-activation")?.into(),
            spec: BinaryPackageSpec::from(format!("=={VERSION}").parse::<VersionSpec>()?).into(),
        });
    }
    Ok(CondaOutput {
        metadata: CondaOutputMetadata {
            name: output.name.clone(),
            version: VERSION.parse()?,
            build: "0".into(),
            build_number: 0,
            subdir: output.package_platform(),
            noarch: Default::default(),
            variant: Default::default(),
            flags: vec![],
            track_features: vec![],
            license: None,
            license_family: None,
            purls: None,
            python_site_packages_path: None,
        },
        run_dependencies: CondaOutputDependencies {
            depends,
            ..Default::default()
        },
        build_dependencies: None,
        host_dependencies: None,
        extra_dependencies: Default::default(),
        ignore_run_exports: Default::default(),
        run_exports: Default::default(),
        input_globs: None,
        input_glob_sets: None,
    })
}
