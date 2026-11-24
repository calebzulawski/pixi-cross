//! Save only the exports that must be restored on deactivation or a nested target switch.
use std::collections::BTreeMap;
use std::env;
use std::io::{self, BufRead};

use anyhow::{Context, Result, ensure};

fn tracked_name(key: &str) -> bool {
    !key.is_empty()
        && !key.as_bytes()[0].is_ascii_digit()
        && key.bytes().all(|c| c == b'_' || c.is_ascii_alphanumeric())
        && !matches!(key, "_" | "PATH" | "PWD" | "OLDPWD")
        && !key.starts_with("PIXI_")
        && !key.starts_with("_pixi_cross_")
}

fn tracked_exports() -> impl Iterator<Item = (String, Vec<u8>)> {
    env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_encoded_bytes())))
        .filter(|(name, _)| tracked_name(name))
}

pub(super) fn print_export(key: &str, value: &[u8]) {
    println!("export {key}=$'{}'", value.escape_ascii());
}

/// Keep the pre-activation values in a shell array until the compiler hooks finish.
pub(super) fn save_initial_exports() {
    print!("_pixi_cross_before=(");
    for (key, _) in tracked_exports() {
        print!(" \"{key}=${{{key}}}\"");
    }
    println!(" )");
}

fn parse_snapshot_entry(entry: &[u8]) -> Result<(&str, &[u8])> {
    let mut fields = entry.splitn(2, |&c| c == b'=');
    let name = std::str::from_utf8(fields.next().unwrap())?;
    ensure!(tracked_name(name), "invalid activation variable {name:?}");
    let value = fields
        .next()
        .context("expected name=value in the activation snapshot")?;
    Ok((name, value))
}

/// Compare the shell's NUL-separated snapshot with the exports after compiler activation.
pub(super) fn record_activation_changes() -> Result<()> {
    let mut current: BTreeMap<_, _> = tracked_exports().collect();
    // Native GCC activation can clear this flag.
    let cross = u8::from(env::var("build_platform")? != env::var("target_platform")?).to_string();
    print_export("CONDA_BUILD_CROSS_COMPILATION", cross.as_bytes());
    current.insert("CONDA_BUILD_CROSS_COMPILATION".into(), cross.into_bytes());

    let mut changed = Vec::new();
    for entry in io::stdin().lock().split(0) {
        let entry = entry?;
        let (name, value) = parse_snapshot_entry(&entry)?;
        if current.remove(name).as_deref() != Some(value) {
            print_export(&format!("PIXI_CROSS_SAVED_{name}"), value);
            changed.push(name.to_owned());
        }
    }
    // Variables first exported during activation have no previous value.
    changed.extend(current.into_keys());
    print_export("PIXI_CROSS_CHANGED", changed.join(" ").as_bytes());
    Ok(())
}

pub(super) fn restore_previous_exports() {
    let changed = env::var("PIXI_CROSS_CHANGED").unwrap_or_default();
    for name in changed.split_ascii_whitespace().filter(|k| tracked_name(k)) {
        let saved = format!("PIXI_CROSS_SAVED_{name}");
        if env::var_os(&saved).is_some() {
            println!("export {name}=\"${saved}\"\nunset {saved}");
        } else {
            println!("unset {name}");
        }
    }
    println!("unset PIXI_CROSS_CHANGED");
}
