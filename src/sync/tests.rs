use super::{check_or_write_manifests, manifest::render_workspace_manifest};
use std::fs;

const EXAMPLE: &str = include_str!("../../examples/openssl/pixi.toml");
const HELPER: &str = include_str!("../../examples/openssl/.pixi-cross/pixi.toml");

fn authored_example() -> &'static str {
    EXAMPLE.split_once("[environments]").unwrap().0
}

#[test]
fn syncing_the_example_reproduces_both_manifests() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("pixi.toml");
    let helper = dir.path().join(".pixi-cross/pixi.toml");
    fs::write(&manifest, authored_example()).unwrap();

    let sync = |check| check_or_write_manifests(&manifest, &helper, check);
    assert!(sync(true).is_err());
    assert!(!helper.exists());
    assert_eq!(fs::read_to_string(&manifest).unwrap(), authored_example());
    sync(false).unwrap();
    assert_eq!(fs::read_to_string(&manifest).unwrap(), EXAMPLE);
    assert_eq!(fs::read_to_string(&helper).unwrap(), HELPER);
    sync(true).unwrap();
}

#[test]
fn sync_refuses_to_overwrite_an_authored_helper() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("pixi.toml");
    let helper = dir.path().join(".pixi-cross/pixi.toml");
    fs::create_dir(helper.parent().unwrap()).unwrap();
    fs::write(&helper, "# mine\n").unwrap();
    fs::write(&manifest, authored_example()).unwrap();

    let error = check_or_write_manifests(&manifest, &helper, false).unwrap_err();
    assert!(error.to_string().contains("refusing to replace authored"));
    assert_eq!(fs::read_to_string(helper).unwrap(), "# mine\n");
    assert_eq!(fs::read_to_string(manifest).unwrap(), authored_example());
}

#[test]
fn renaming_a_profile_preserves_handwritten_entries_and_comments() {
    let edited = EXAMPLE
        .replace(
            "[tool.cross.environments.dev]",
            "[tool.cross.environments.ci]",
        )
        .replace("dev-linux-64 = ", "lint = ['host'] # keep\ndev-linux-64 = ");
    let task = "[tasks.show]\ncmd = '''\n[feature.decoy]  # pixi-cross\n'''\n";
    let input = (edited + task).replace('\n', "\r\n");

    let updated = render_workspace_manifest(&input).unwrap();
    assert!(updated.contains("lint = ['host'] # keep\r\n"));
    assert!(updated.contains(&task.replace('\n', "\r\n")));
    assert!(!updated.replace("\r\n", "").contains('\n'));
    let manifest: toml::Table = toml::from_str(&updated).unwrap();
    let envs = manifest["environments"].as_table().unwrap();
    assert!(envs.contains_key("ci-linux-aarch64"));
    assert!(!envs.contains_key("dev-linux-aarch64"));
}

#[test]
fn sync_accepts_existing_environment_tables() {
    for declaration in [
        "[environments]\nci = { features = ['host'], solve-group = 'a,b' }",
        "[environments.ci]\nfeatures = ['host']\nsolve-group = 'a,b'",
        "environments.ci = { features = ['host'], solve-group = 'a,b' }",
        "environments = { ci = { features = ['host'], solve-group = 'a,b' }, other = [] }",
        "environments = { ci.features = ['host'], other = [],\n ci.solve-group = 'a,b' }",
    ] {
        let input = format!("{declaration} # handwritten\n{}", authored_example());
        let updated = render_workspace_manifest(&input).unwrap();
        assert!(updated.contains("# handwritten"));
        assert_eq!(render_workspace_manifest(&updated).unwrap(), updated);
        let manifest: toml::Table = toml::from_str(&updated).unwrap();
        let envs = &manifest["environments"];
        assert_eq!(envs["ci"]["features"][0].as_str(), Some("host"));
        assert_eq!(envs["ci"]["solve-group"].as_str(), Some("a,b"));
        assert!(envs.get("dev-linux-aarch64").is_some());
    }
}

#[test]
fn an_authored_environment_name_collision_stops_sync() {
    let input = format!("{}[environments]\ndev = []\n", authored_example());
    let error = render_workspace_manifest(&input).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("conflicts with an authored environment")
    );
}

#[test]
fn host_environments_split_only_when_their_dependencies_vary_by_build() {
    let project = authored_example().replace(
        "[feature.host.dependencies]",
        "[tool.cross.feature.host.dependencies]",
    );
    for (package, expected) in [
        (
            "\"sdk-{{ target_platform }}\"",
            vec!["dev-host-dependencies"],
        ),
        (
            "\"sdk-{{ build_platform }}\"",
            vec![
                "dev-host-dependencies-linux-64",
                "dev-host-dependencies-linux-aarch64",
            ],
        ),
    ] {
        let input = project.replace("openssl =", &format!("{package} ="));
        let updated = render_workspace_manifest(&input).unwrap();
        let manifest: toml::Table = toml::from_str(&updated).unwrap();
        let companions: Vec<_> = manifest["environments"]
            .as_table()
            .unwrap()
            .keys()
            .filter(|name| name.starts_with("dev-host-dependencies"))
            .collect();
        assert_eq!(companions, expected);
    }
}
