//! The operator's curated icon prototypes (`[cache] prototypes`): one JSON object per
//! line, `{"label": "...", "context": "...", "grid": [72 grey levels]}`, where `grid` is
//! the 9×8 greyscale crop of the icon, row-major, as the engine normalises it.
//!
//! The file is validated strictly and refused as a whole on any problem: an unknown or
//! missing key, a blank line, a label or context out of bounds, a grid that is not 72
//! values in 0..=255, or more prototypes than `[cache] max_entries`. Every prototype is
//! `Curated` (the operator reviewed it) and is valid in its own context only.
//!
//! The file is pinned by SHA-256 in the operator's configuration (`[cache]
//! prototypes_sha256`, thesis chapter 08 §8.2, SEC-06), like the model manifest: it is
//! read once, bounded, hashed, and the bytes that were hashed are the bytes parsed, so a
//! prototype added or changed after the operator reviewed the file (cache poisoning) is
//! refused before any icon is labelled with it. Nothing at run time adds a prototype: the
//! cache is built here, once, and only from this file.

use std::path::Path;

use anyhow::{Context, Result, bail};
use ommatix_adapter_vision::model::integrity::{Sha256Digest, read_verified};
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

/// Reads `path` (at most as many bytes as `max_entries` lines can hold), checks it against
/// the SHA-256 the operator pinned and parses those same bytes.
pub fn load(path: &Path, max_entries: u32, tau: u8, sha256: &Sha256Digest) -> Result<IconCache> {
    let read = || -> Result<String> {
        let limit = u64::from(max_entries) * MAX_LINE_BYTES;
        let bytes = read_verified("curated icon prototypes", path, sha256, limit)?;
        String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("the file is not UTF-8"))
    };
    let text = read().with_context(|| format!("cannot load {}", path.display()))?;
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

    fn temp_file(tag: &str, bytes: &[u8]) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("ommatix-prototypes-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("icons.jsonl");
        std::fs::write(&file, bytes).unwrap();
        (dir, file)
    }

    #[test]
    fn a_missing_file_is_refused() {
        let pin = Sha256Digest::of(b"x");
        let message = format!(
            "{:#}",
            load(Path::new("no/such/file.jsonl"), 4, 3, &pin).unwrap_err()
        );
        assert!(message.contains("cannot load"), "{message}");
    }

    /// SEC-06, cache poisoning: the file the operator reviewed is the file the engine
    /// loads. A prototype appended, a label changed or a line dropped afterwards changes
    /// the digest, and the whole file is refused before any icon is labelled with it.
    #[test]
    fn sec_06_a_prototypes_file_changed_after_it_was_pinned_is_refused() {
        let reviewed = format!(
            "{}
",
            line("Save", "notepad.exe", &stripes())
        );
        let pin = Sha256Digest::of(reviewed.as_bytes());
        let (dir, file) = temp_file("pinned", reviewed.as_bytes());
        let cache = load(&file, 4, 3, &pin).unwrap();
        assert_eq!(cache.prototypes().len(), 1);

        let poisoned = [
            // A second, attacker-chosen label for the same appearance.
            format!(
                "{reviewed}{}
",
                line("Delete", "notepad.exe", &stripes())
            ),
            // The reviewed label swapped for another.
            reviewed.replace("Save", "Send"),
            // The context moved to another application.
            reviewed.replace("notepad.exe", "calc.exe"),
            // Trailing whitespace is a change too.
            format!(
                "{reviewed}
"
            ),
            String::new(),
        ];
        for text in poisoned {
            std::fs::write(&file, &text).unwrap();
            let message = format!("{:#}", load(&file, 4, 3, &pin).unwrap_err());
            assert!(message.contains("SHA-256 mismatch"), "{text:?}: {message}");
        }
        // Restored, it loads again.
        std::fs::write(&file, &reviewed).unwrap();
        assert!(load(&file, 4, 3, &pin).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_pinned_file_is_still_bounded_by_max_entries() {
        let good = line("Save", "app", &stripes());
        let text = format!(
            "{good}
{good}
{good}
"
        );
        let pin = Sha256Digest::of(text.as_bytes());
        let (dir, file) = temp_file("bounded", text.as_bytes());
        assert!(load(&file, 3, 3, &pin).is_ok());
        let message = format!("{:#}", load(&file, 2, 3, &pin).unwrap_err());
        assert!(message.contains("max_entries"), "{message}");
        // A file larger than max_entries lines can be is refused without reading it all.
        let huge = vec![b' '; 4 * MAX_LINE_BYTES as usize];
        std::fs::write(&file, &huge).unwrap();
        let message = format!(
            "{:#}",
            load(&file, 2, 3, &Sha256Digest::of(&huge)).unwrap_err()
        );
        assert!(message.contains("larger than"), "{message}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// SEC-06, Algorithm 3A: two different labels for one appearance in the operator's
    /// file are a conflict, not an overwrite; the icon stays `unknown` (UT-31 at file
    /// level), and a prototype for another context never labels this one.
    #[test]
    fn sec_06_conflicting_or_foreign_prototypes_never_label_an_icon() {
        use ommatix_domain::appearance::{
            CachePolicy, IconMatch, UnknownReason, gradient_descriptor,
        };
        let text = [
            line("Save", "notepad.exe", &stripes()),
            line("Delete", "notepad.exe", &stripes()),
            line(
                "Format",
                "paint.exe",
                &[0, 80, 0, 80, 0, 80, 0, 80, 0].repeat(8),
            ),
        ]
        .join(
            "
",
        );
        let cache = parse(&text, 8, 3).unwrap();
        assert_eq!(cache.prototypes().len(), 3, "no entry overwrote another");
        let policy = CachePolicy::new(0.1, 0.1).unwrap();
        let query = gradient_descriptor(&stripes(), 3).unwrap();
        assert_eq!(
            cache.lookup(&query, "notepad.exe", policy),
            Ok(IconMatch::Unknown(UnknownReason::Ambiguous))
        );
        // The label curated for another application is not meaning here.
        let other = gradient_descriptor(&[0, 80, 0, 80, 0, 80, 0, 80, 0].repeat(8), 3).unwrap();
        assert!(matches!(
            cache.lookup(&other, "notepad.exe", policy),
            Ok(IconMatch::Unknown(_))
        ));
        assert!(matches!(
            cache.lookup(&other, "paint.exe", policy),
            Ok(IconMatch::Known { ref label, .. }) if label == "Format"
        ));
    }
}
