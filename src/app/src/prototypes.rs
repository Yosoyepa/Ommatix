//! The operator's curated icon prototypes (`[cache] prototypes`): one JSON object per
//! line, `{"label": "...", "context": "...", "grid": [72 grey levels]}`, where `grid` is
//! the 9×8 greyscale crop of the icon, row-major, as the engine normalises it.
//!
//! The file is validated strictly and refused as a whole on any problem: an unknown or
//! missing key, a blank line, a label or context out of bounds, a grid that is not 72
//! values in 0..=255, or more prototypes than `[cache] max_entries`. Every prototype is
//! `Curated` (the operator reviewed it) and is valid in its own context only.

use std::path::Path;

use anyhow::{Context, Result, bail};
use ommatix_application::contract::limits;
use ommatix_domain::appearance::{
    DESCRIPTOR_VERSION, GRID_LEN, IconCache, Prototype, PrototypeSource, gradient_descriptor,
};
use serde::Deserialize;

/// Bytes of one line, generously: 72 values of up to three digits and two strings.
const MAX_LINE_BYTES: u64 = 2048;

/// Longest context name, as for `[safety] allowed_applications`.
pub const MAX_CONTEXT_CHARS: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Line {
    label: String,
    context: String,
    grid: Vec<u8>,
}

/// A label or context: not blank, no control characters, at most `max` characters.
fn valid_name(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

/// Parses the text of a prototypes file into a cache, computing each descriptor with
/// `tau`.
pub fn parse(text: &str, max_entries: u32, tau: u8) -> Result<IconCache> {
    let mut cache = IconCache::new();
    for (index, line) in text.lines().enumerate() {
        let n = index + 1;
        if cache.prototypes().len() >= max_entries as usize {
            bail!("more than [cache] max_entries = {max_entries} prototypes");
        }
        let entry: Line =
            serde_json::from_str(line).with_context(|| format!("line {n}: invalid prototype"))?;
        if !valid_name(&entry.label, limits::MAX_LABEL_CHARS) {
            bail!(
                "line {n}: the label must be 1 to {} visible characters",
                limits::MAX_LABEL_CHARS
            );
        }
        if !valid_name(&entry.context, MAX_CONTEXT_CHARS) {
            bail!("line {n}: the context must be 1 to {MAX_CONTEXT_CHARS} visible characters");
        }
        if entry.grid.len() != GRID_LEN {
            bail!(
                "line {n}: the grid must have {GRID_LEN} grey levels (9x8, row-major), not {}",
                entry.grid.len()
            );
        }
        let descriptor = gradient_descriptor(&entry.grid, tau)
            .with_context(|| format!("line {n}: invalid grid"))?;
        cache.insert(Prototype {
            label: entry.label,
            context: entry.context,
            descriptor_version: DESCRIPTOR_VERSION,
            descriptor,
            source: PrototypeSource::Curated,
        });
    }
    if cache.prototypes().is_empty() {
        bail!("the file has no prototypes");
    }
    Ok(cache)
}

/// Reads and parses `path`, refusing a file larger than `max_entries` lines can be.
pub fn load(path: &Path, max_entries: u32, tau: u8) -> Result<IconCache> {
    let read = || -> Result<String> {
        let limit = u64::from(max_entries) * MAX_LINE_BYTES;
        let size = std::fs::metadata(path)?.len();
        if size > limit {
            bail!(
                "the file has {size} bytes; the limit for max_entries = {max_entries} is {limit}"
            );
        }
        Ok(std::fs::read_to_string(path)?)
    };
    let text = read().with_context(|| format!("cannot read {}", path.display()))?;
    parse(&text, max_entries, tau).with_context(|| format!("in {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(label: &str, context: &str, grid: &[u8]) -> String {
        serde_json::json!({ "label": label, "context": context, "grid": grid }).to_string()
    }

    fn stripes() -> Vec<u8> {
        [0, 90, 90, 0, 0, 90, 90, 0, 0].repeat(8)
    }

    fn error(text: &str) -> String {
        format!("{:#}", parse(text, 4, 3).unwrap_err())
    }

    #[test]
    fn curated_prototypes_are_loaded_in_their_context() {
        let text = format!(
            "{}\n{}\n",
            line("Save", "notepad.exe", &stripes()),
            line("Save", "paint.exe", &stripes())
        );
        let cache = parse(&text, 2, 3).unwrap();
        assert_eq!(cache.prototypes().len(), 2);
        let p = &cache.prototypes()[0];
        assert_eq!(
            (p.label.as_str(), p.context.as_str()),
            ("Save", "notepad.exe")
        );
        assert_eq!(p.source, PrototypeSource::Curated);
        assert_eq!(p.descriptor, gradient_descriptor(&stripes(), 3).unwrap());
    }

    #[test]
    fn any_invalid_line_refuses_the_whole_file() {
        let good = line("Save", "app", &stripes());
        let mut wide = stripes();
        wide.push(0);
        let long_label = "x".repeat(limits::MAX_LABEL_CHARS + 1);
        for (text, expected) in [
            (String::new(), "no prototypes"),
            (format!("{good}\n\n{good}"), "line 2"),
            (format!("{good}\nnot json"), "line 2"),
            (line("Save", "app", &stripes()[..71]), "72 grey levels"),
            (line("Save", "app", &wide), "72 grey levels"),
            (line(" ", "app", &stripes()), "label"),
            (line(&long_label, "app", &stripes()), "label"),
            (line("Sa\u{7}ve", "app", &stripes()), "label"),
            (line("Save", "", &stripes()), "context"),
            (
                r#"{"label":"Save","context":"app","grid":[300]}"#.to_owned(),
                "invalid prototype",
            ),
            (
                r#"{"label":"Save","context":"app","grid":[],"source":"model"}"#.to_owned(),
                "invalid prototype",
            ),
            (
                r#"{"label":"Save","context":"app"}"#.to_owned(),
                "invalid prototype",
            ),
            (
                format!("{good}\n{good}\n{good}\n{good}\n{good}"),
                "max_entries",
            ),
        ] {
            let message = error(&text);
            assert!(message.contains(expected), "{text}: {message}");
        }
    }

    #[test]
    fn a_missing_file_is_refused() {
        let message = format!(
            "{:#}",
            load(Path::new("no/such/file.jsonl"), 4, 3).unwrap_err()
        );
        assert!(message.contains("cannot read"), "{message}");
    }
}
