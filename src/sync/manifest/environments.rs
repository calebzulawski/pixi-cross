//! Add generated environments alongside handwritten ones.
use std::ops::Range;

use anyhow::Result;
use toml_edit::{Document, InlineTable, Table};

use super::append_table;
use crate::common::manifest::{line_end, line_start};

/// Add entries to the existing environments table or append a new section.
pub(super) fn insert_environment_entries(
    source: &mut String,
    span: Option<Range<usize>>,
    mut entries: Table,
    newline: &str,
) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    // The range covers the braces, the section header, or the name in a dotted key.
    let offset = match span {
        Some(s) if source[s.clone()].starts_with('{') => {
            convert_inline_environments_to_section(source, s, newline)?
        }
        Some(s) if source[s.clone()].starts_with('[') => line_end(source, s.end),
        Some(s) if !source[line_start(source, s.start)..s.start].contains('[') => {
            entries.set_dotted(true);
            entries = Table::from_iter([("environments", entries)]);
            line_start(source, s.start)
        }
        _ => {
            // A child section such as [environments.ci] can also exist without a parent header.
            append_table(source, "environments", entries, newline);
            return Ok(());
        }
    };
    let mut block = entries.to_string().replace('\n', newline);
    if offset > 0 && !source[..offset].ends_with('\n') {
        block.insert_str(0, newline);
    }
    source.insert_str(offset, &block);
    Ok(())
}

/// Move an inline environments table below the root settings and return the start of its entries.
fn convert_inline_environments_to_section(
    source: &mut String,
    span: Range<usize>,
    newline: &str,
) -> Result<usize> {
    let root = Document::parse(&source[..span.end])?;
    let (key, item) = root.get_key_value("environments").unwrap();
    let key_span = key.span().unwrap();
    let start = line_start(source, key_span.start);
    let prefix = key
        .leaf_decor()
        .prefix()
        .and_then(|s| s.span())
        .map_or(start, |s| s.start);
    let comments_start = source[prefix..start]
        .split_inclusive('\n')
        .rev()
        .take_while(|s| s.trim_start().starts_with('#'))
        .fold(start, |i, s| i - s.len());
    let end = line_end(source, span.end);
    let leading = &source[comments_start..start];
    let name = &source[key_span];
    let trailing = source[span.end..end].trim_end_matches(['\r', '\n']);
    let header = format!("{leading}[{name}]{trailing}{newline}");
    let body = format_inline_assignments(source, item.as_inline_table().unwrap(), newline);
    source.replace_range(comments_start..end, "");
    if !source.is_empty() && !source.ends_with('\n') {
        source.push_str(newline);
    }
    source.push_str(&header);
    let offset = source.len();
    source.push_str(&body);
    Ok(offset)
}

/// Put each inline assignment on its own line, keeping comments and the original key order.
fn format_inline_assignments(source: &str, table: &InlineTable, newline: &str) -> String {
    // Each value's trailing whitespace and comments end at the next outer comma or closing brace.
    let span = table.span().unwrap();
    let closing_brace = span.end - 1;
    let mut entry_ends: Vec<_> = table
        .get_values()
        .into_iter()
        .map(|(_, value)| value.decor().suffix().unwrap().span().unwrap().end)
        .filter(|&end| end < closing_brace)
        .collect();
    entry_ends.sort_unstable();
    let mut body = String::new();
    let mut start = span.start + 1;
    for end in entry_ends.into_iter().chain([closing_brace]) {
        let entry = source[start..end].trim_matches([' ', '\t']);
        body.push_str(entry);
        if !entry.is_empty() && !entry.ends_with('\n') {
            body.push_str(newline);
        }
        start = end + 1;
    }
    body
}
