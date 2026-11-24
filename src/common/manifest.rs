//! Read the authored workspace, excluding registrations marked by pixi-cross.
use std::{borrow::Cow, fs, ops::Range, path::Path};

use anyhow::{Context, Result};
use toml_edit::{Document, Item, Table};

use super::project::Project;

pub(crate) const TAG: &str = "# pixi-cross";

/// Load the project after removing previously generated entries from pixi.toml.
pub fn load_authored_project(path: &Path) -> Result<Project> {
    let input = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Project::from_manifest(&strip_generated_entries(&input)?)
}

/// Remove the environment entries and feature sections marked with `# pixi-cross`.
pub fn strip_generated_entries(input: &str) -> Result<Cow<'_, str>> {
    if !input.contains(TAG) {
        return Ok(Cow::Borrowed(input));
    }

    let doc = Document::parse(input)?;
    let mut ranges = Vec::new();
    if let Some(envs) = doc.get("environments").and_then(Item::as_table) {
        for value in envs.iter().filter_map(|(_, item)| item.as_value()) {
            let span = value.span().unwrap();
            if has_generation_marker(input, span.end) {
                ranges.push(line_start(input, span.start)..line_end(input, span.end));
            }
        }
    }
    if let Some(features) = doc.get("feature").and_then(Item::as_table) {
        for table in features.iter().filter_map(|(_, item)| item.as_table()) {
            let header = table.span().unwrap();
            if !table.is_implicit() && has_generation_marker(input, header.end) {
                ranges.push(generated_feature_range(input, table, header));
            }
        }
    }
    if ranges.is_empty() {
        return Ok(Cow::Borrowed(input));
    }

    ranges.sort_unstable_by_key(|r| r.start);
    let mut authored = String::with_capacity(input.len());
    let mut start = 0;
    for range in ranges {
        authored.push_str(&input[start..range.start]);
        start = range.end;
    }
    authored.push_str(&input[start..]);
    Ok(Cow::Owned(authored))
}

/// Check whether the marker is the only text left on this line.
fn has_generation_marker(input: &str, end: usize) -> bool {
    input[end..line_end(input, end)].trim() == TAG
}

/// Include the blank line before a generated feature and stop after its last assignment.
fn generated_feature_range(input: &str, table: &Table, header: Range<usize>) -> Range<usize> {
    let mut start = line_start(input, header.start);
    if input[..start].ends_with("\r\n\r\n") {
        start -= 2;
    } else if input[..start].ends_with("\n\n") {
        start -= 1;
    }
    let end = table
        .get_values()
        .iter()
        .map(|(_, v)| v.span().unwrap().end)
        .max()
        .unwrap_or(header.end);
    start..line_end(input, end)
}

/// Find the start of the line, skipping a byte-order mark on the first line.
pub(crate) fn line_start(source: &str, offset: usize) -> usize {
    source[..offset].rfind('\n').map_or_else(
        || source.len() - source.trim_start_matches('\u{feff}').len(),
        |i| i + 1,
    )
}

/// Find the position after the line's newline or at the end of the file.
pub(crate) fn line_end(source: &str, offset: usize) -> usize {
    source[offset..]
        .find('\n')
        .map_or(source.len(), |i| offset + i + 1)
}
