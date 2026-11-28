//! The operator's grants file (`[safety] grants_file`; ADR-0040, decisions 3 to 5, and
//! ADR-0043, decision 9; both proposed): which grant each ticket selects, for the clients of the
//! broker served over the pipe (`[mcp] transport = "pipe"`).
//!
//! The file is the operator's, in TOML, like the configuration:
//!
//! ```toml
//! [[grant]]
//! ticket_sha256 = "<the SHA-256 of the ticket, 64 hexadecimal characters>"
//! applications = ["notepad.exe"]
//! deliver_input = true
//! read_values = false
//! # optional:
//! window_pin = 1234567
//! delivery_budget = 20      # results that report input, from admission; 0 = observe only
//! term_ms = 600000          # the broker checks this term at the entry of the tool call
//! [grant.window_classes]
//! "notepad.exe" = ["Notepad"]
//! ```
//!
//! The integer ranges are 0 through 4294967295 for `delivery_budget` and 0 through
//! 9223372036854775807 for `term_ms`. A negative value, a float, a string or a value above the
//! range makes the whole file invalid. An absent key sets no limit in the file. A zero budget
//! bars live input. A zero term ends at admission and leaves the client with the default grant.
//! The broker samples the term for a new live call at entry, measured from admission. The term
//! does not interrupt a call that has passed that check. A connected client's ceilings take the
//! meet of each grant read at a tool call and can only go down.
//!
//! For each grant lookup from the broker, [`GrantsFile`] opens the file once, reads at most
//! [`MAX_GRANTS_FILE_BYTES`] plus one byte, parses it and returns the grant whose hash matches,
//! or none. The broker performs this lookup for the ticket at each tool call. A withdrawal
//! supplies no grant and leaves the ledger ceilings in place. It fails closed: the **whole**
//! file is invalid, and answers none for every ticket, when it
//! is missing, unreadable, over the bound, not valid TOML of this form (an unknown, missing or
//! mistyped key included), has a malformed hash, two grants with the same hash, more than
//! [`MAX_GRANTS`] grants, or an entry whose names or pin could not be what the operator meant.
//! It never writes, creates or removes a file, and it never sees a ticket: the broker hands it
//! the ticket's SHA-256. A path that opens as anything but a regular file on a disk (a
//! directory, a device, a named pipe) is refused before anything is read, and the whole file is
//! then invalid too.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ommatix_application::{Grant, GrantStore, WindowPin};
use serde::Deserialize;

use crate::operator_file::{self, OpenError};

/// The largest grants file read, in bytes: the configuration file's own bound.
pub const MAX_GRANTS_FILE_BYTES: u64 = 64 * 1024;

/// The most grants one file may list. A grant is an agent's task; eight clients at most are
/// connected at once (the engine's journal shares), so 64 leaves room for tickets issued ahead.
pub const MAX_GRANTS: usize = 64;

/// The longest application or window-class name, in characters, as in `[safety]`.
const MAX_NAME_CHARS: usize = 256;

/// The largest `term_ms`: TOML's largest integer. The `toml` crate reads larger integers into a
/// `u64` field, so the bound is checked here.
const MAX_TERM_MS: u64 = 9_223_372_036_854_775_807;

/// Why a grants file was found invalid: a closed list of causes, which carries nothing of the
/// file's text (no key, value or hash), so that it can be reported on stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantsProblem {
    /// No file at the path.
    Missing,
    /// The path opened, but not as a regular file on a disk (a directory, a device or a named
    /// pipe), or its kind could not be read: refused before anything is read
    /// ([`crate::operator_file`]).
    NotARegularFile,
    /// The file could not be opened or read, or is not UTF-8.
    Unreadable,
    /// The file is larger than [`MAX_GRANTS_FILE_BYTES`].
    TooLarge,
    /// Not valid TOML of the grants file's form: a syntax error, an unknown key, a missing key or
    /// a value of the wrong type.
    NotTheForm,
    /// A `ticket_sha256` that is not 64 hexadecimal characters.
    MalformedHash,
    /// Two grants with the same `ticket_sha256`.
    DuplicateHash,
    /// More than [`MAX_GRANTS`] grants.
    TooManyGrants,
    /// An empty or overlong application or window-class name, one with a control character, a
    /// `window_classes` key that is not among the grant's `applications`, or a `window_pin` of 0.
    InvalidEntry,
}

impl fmt::Display for GrantsProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Missing => "it does not exist",
            Self::NotARegularFile => {
                "it is not a regular file on a disk (a directory, a device or a named pipe is \
                 refused before anything is read)"
            }
            Self::Unreadable => "it cannot be read as UTF-8 text",
            Self::TooLarge => "it is larger than 65536 bytes",
            Self::NotTheForm => {
                "it is not a list of [[grant]] tables with exactly the keys ticket_sha256, \
                 applications, deliver_input, read_values and, optionally, window_classes, \
                 window_pin, delivery_budget (an integer from 0 to 4294967295) and term_ms (an \
                 integer from 0 to 9223372036854775807)"
            }
            Self::MalformedHash => "a ticket_sha256 is not 64 hexadecimal characters",
            Self::DuplicateHash => "two grants have the same ticket_sha256",
            Self::TooManyGrants => "it lists more than 64 grants",
            Self::InvalidEntry => {
                "an application or window class is empty, longer than 256 characters or holds a \
                 control character, a window_classes key is not among the grant's applications, \
                 or a window_pin is 0"
            }
        })
    }
}

impl std::error::Error for GrantsProblem {}

/// The file's shape. Unknown keys are refused at every level.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantsText {
    #[serde(default)]
    grant: Vec<Entry>,
}

/// One `[[grant]]`: the four keys every grant states, and the four optional limits.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    ticket_sha256: String,
    applications: Vec<String>,
    deliver_input: bool,
    read_values: bool,
    #[serde(default)]
    window_classes: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    window_pin: Option<u64>,
    #[serde(default)]
    delivery_budget: Option<u32>,
    #[serde(default)]
    term_ms: Option<u64>,
}

/// A name the gate could match: non-empty, at most 256 characters, no control character.
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= MAX_NAME_CHARS
        && !name.chars().any(char::is_control)
}

/// 64 hexadecimal characters, either case, as 32 bytes.
fn parse_sha256(hex: &str) -> Option<[u8; 32]> {
    let bytes = hex.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = [0_u8; 32];
    for (slot, &[high, low]) in out.iter_mut().zip(bytes.as_chunks::<2>().0) {
        let high = char::from(high).to_digit(16)?;
        let low = char::from(low).to_digit(16)?;
        *slot = u8::try_from(high * 16 + low).ok()?;
    }
    Some(out)
}

impl Entry {
    fn into_grant(self) -> Result<([u8; 32], Grant), GrantsProblem> {
        if self.term_ms.is_some_and(|term| term > MAX_TERM_MS) {
            return Err(GrantsProblem::NotTheForm);
        }
        let hash = parse_sha256(&self.ticket_sha256).ok_or(GrantsProblem::MalformedHash)?;
        let applications: BTreeSet<String> = self.applications.into_iter().collect();
        let names_valid = applications.iter().all(|a| valid_name(a))
            && self.window_classes.iter().all(|(application, classes)| {
                applications.contains(application) && classes.iter().all(|c| valid_name(c))
            });
        if !names_valid || self.window_pin == Some(0) {
            return Err(GrantsProblem::InvalidEntry);
        }
        let grant = Grant {
            applications,
            window_classes: self
                .window_classes
                .into_iter()
                .map(|(application, classes)| (application, classes.into_iter().collect()))
                .collect(),
            window_pin: self.window_pin.map(WindowPin::Window),
            deliver_input: self.deliver_input,
            read_values: self.read_values,
            delivery_budget: self.delivery_budget,
            term: self.term_ms.map(Duration::from_millis),
        };
        Ok((hash, grant))
    }
}

/// Parses a grants file's text: every grant by its ticket's hash, or why the whole file is
/// invalid.
///
/// # Errors
///
/// A [`GrantsProblem`] other than the three of reading the file.
pub fn parse(text: &str) -> Result<BTreeMap<[u8; 32], Grant>, GrantsProblem> {
    let file: GrantsText = toml::from_str(text).map_err(|_| GrantsProblem::NotTheForm)?;
    if file.grant.len() > MAX_GRANTS {
        return Err(GrantsProblem::TooManyGrants);
    }
    let mut grants = BTreeMap::new();
    for entry in file.grant {
        let (hash, grant) = entry.into_grant()?;
        if grants.insert(hash, grant).is_some() {
            return Err(GrantsProblem::DuplicateHash);
        }
    }
    Ok(grants)
}

/// Reads and parses the grants file at `path` now: one open, the handle's kind checked before
/// any read ([`operator_file::open_regular`]), at most [`MAX_GRANTS_FILE_BYTES`] plus one byte
/// read, nothing written.
///
/// It is synchronous: the broker calls it on every question under its own lock, so that a grant
/// applies atomically with the call (ADR-0040, decision 5). A stated limit: the open, the kind and
/// metadata queries and the read can each block for as long as the operator's filesystem does,
/// and while they block every client waits; the file must be on a local fixed disk.
///
/// # Errors
///
/// Why the whole file is invalid.
pub fn read(path: &Path) -> Result<BTreeMap<[u8; 32], Grant>, GrantsProblem> {
    let file = operator_file::open_regular(path).map_err(|e| match e {
        OpenError::NotFound => GrantsProblem::Missing,
        OpenError::Unopenable(_) => GrantsProblem::Unreadable,
        OpenError::NotARegularFile => GrantsProblem::NotARegularFile,
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_GRANTS_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| GrantsProblem::Unreadable)?;
    if bytes.len() as u64 > MAX_GRANTS_FILE_BYTES {
        return Err(GrantsProblem::TooLarge);
    }
    let text = String::from_utf8(bytes).map_err(|_| GrantsProblem::Unreadable)?;
    parse(&text)
}

/// The operator's grants file as the broker's [`GrantStore`]: read on every question, fail
/// closed, never written.
#[derive(Debug, Clone)]
pub struct GrantsFile {
    path: PathBuf,
}

impl GrantsFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// How many grants the file lists now, or why it is invalid: for the start-up note.
    ///
    /// # Errors
    ///
    /// Why the whole file is invalid now.
    pub fn count(&self) -> Result<usize, GrantsProblem> {
        read(&self.path).map(|grants| grants.len())
    }
}

impl GrantStore for GrantsFile {
    fn grant(&self, ticket_sha256: &[u8; 32]) -> Option<Grant> {
        read(&self.path).ok()?.remove(ticket_sha256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // UT-76: the reader of the grants file.

    fn hash(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn hex(hash: &[u8; 32]) -> String {
        use std::fmt::Write as _;
        hash.iter().fold(String::new(), |mut text, b| {
            let _ = write!(text, "{b:02x}");
            text
        })
    }

    fn entry(hash: &[u8; 32], extra: &str) -> String {
        format!(
            "[[grant]]\nticket_sha256 = \"{}\"\napplications = [\"notepad.exe\"]\n\
             deliver_input = true\nread_values = false\n{extra}",
            hex(hash)
        )
    }

    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("ommatix-grants-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, text: &[u8]) -> PathBuf {
            let path = self.0.join("grants.toml");
            std::fs::write(&path, text).unwrap();
            path
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn ut_76_a_valid_file_answers_exactly_its_grants() {
        let text = format!(
            "{}\n{}",
            entry(&hash(1), ""),
            entry(
                &hash(2),
                "window_pin = 42\n[grant.window_classes]\n\"notepad.exe\" = [\"Notepad\"]\n"
            )
        )
        .replace(
            "deliver_input = true\nread_values = false\nwindow_pin",
            "deliver_input = false\nread_values = true\nwindow_pin",
        );
        let grants = parse(&text).unwrap();
        assert_eq!(grants.len(), 2);
        assert_eq!(
            grants[&hash(1)],
            Grant {
                applications: BTreeSet::from(["notepad.exe".to_owned()]),
                deliver_input: true,
                ..Grant::default()
            }
        );
        assert_eq!(
            grants[&hash(2)],
            Grant {
                applications: BTreeSet::from(["notepad.exe".to_owned()]),
                window_classes: BTreeMap::from([(
                    "notepad.exe".to_owned(),
                    BTreeSet::from(["Notepad".to_owned()])
                )]),
                window_pin: Some(WindowPin::Window(42)),
                deliver_input: false,
                read_values: true,
                delivery_budget: None,
                term: None,
            }
        );
        // Upper-case hexadecimal names the same hash; an empty file lists nothing.
        let upper =
            entry(&hash(0xab), "").replace(&hex(&hash(0xab)), &hex(&hash(0xab)).to_uppercase());
        assert!(parse(&upper).unwrap().contains_key(&hash(0xab)));
        assert!(parse("").unwrap().is_empty());

        let dir = Dir::new("valid");
        let store = GrantsFile::new(dir.write(text.as_bytes()));
        assert_eq!(store.grant(&hash(1)), Some(grants[&hash(1)].clone()));
        assert_eq!(store.grant(&hash(2)), Some(grants[&hash(2)].clone()));
        assert_eq!(
            store.grant(&hash(3)),
            None,
            "an unlisted ticket has no grant"
        );
        assert_eq!(store.count(), Ok(2));
    }

    #[test]
    fn ut_76_the_listed_invalid_file_cases_answer_no_grant_for_the_test_tickets() {
        let valid = entry(&hash(1), "");
        let cases: Vec<(String, GrantsProblem)> = vec![
            ("[[grant]\n".to_owned(), GrantsProblem::NotTheForm),
            (
                format!("{valid}color = \"red\"\n"),
                GrantsProblem::NotTheForm,
            ),
            (format!("version = 1\n{valid}"), GrantsProblem::NotTheForm),
            (
                valid.replace("read_values = false\n", ""),
                GrantsProblem::NotTheForm,
            ),
            (
                valid.replace("deliver_input = true", "deliver_input = \"yes\""),
                GrantsProblem::NotTheForm,
            ),
            (
                format!("{valid}[grant.window_classes]\n\"notepad.exe\" = \"Notepad\"\n"),
                GrantsProblem::NotTheForm,
            ),
            (
                format!("{valid}window_pin = -1\n"),
                GrantsProblem::NotTheForm,
            ),
            (
                valid.replace(&hex(&hash(1)), &hex(&hash(1))[..63]),
                GrantsProblem::MalformedHash,
            ),
            (
                valid.replace(&hex(&hash(1)), &format!("{}0", hex(&hash(1)))),
                GrantsProblem::MalformedHash,
            ),
            (
                valid.replace(&hex(&hash(1)), &hex(&hash(1)).replacen('1', "g", 1)),
                GrantsProblem::MalformedHash,
            ),
            (format!("{valid}{valid}"), GrantsProblem::DuplicateHash),
            (
                format!(
                    "{valid}{}",
                    valid.replace(&hex(&hash(1)), &hex(&hash(1)).to_uppercase())
                ),
                GrantsProblem::DuplicateHash,
            ),
            (
                (0..=MAX_GRANTS)
                    .map(|i| entry(&hash(u8::try_from(i).unwrap()), ""))
                    .collect(),
                GrantsProblem::TooManyGrants,
            ),
            (
                valid.replace("[\"notepad.exe\"]", "[\" \"]"),
                GrantsProblem::InvalidEntry,
            ),
            (
                valid.replace("[\"notepad.exe\"]", &format!("[\"{}\"]", "a".repeat(257))),
                GrantsProblem::InvalidEntry,
            ),
            (
                valid.replace("[\"notepad.exe\"]", "[\"note\\npad.exe\"]"),
                GrantsProblem::InvalidEntry,
            ),
            (
                format!("{valid}[grant.window_classes]\n\"calc.exe\" = [\"Calc\"]\n"),
                GrantsProblem::InvalidEntry,
            ),
            (
                format!("{valid}[grant.window_classes]\n\"notepad.exe\" = [\"\"]\n"),
                GrantsProblem::InvalidEntry,
            ),
            (
                format!("{valid}window_pin = 0\n"),
                GrantsProblem::InvalidEntry,
            ),
        ];
        let dir = Dir::new("invalid");
        for (text, expected) in &cases {
            assert_eq!(parse(text).err(), Some(*expected), "{text}");
            let store = GrantsFile::new(dir.write(text.as_bytes()));
            // The listed ticket included: one bad entry voids the whole file.
            for ticket in [hash(1), hash(2), hash(0)] {
                assert_eq!(store.grant(&ticket), None, "{text}");
            }
            assert_eq!(store.count(), Err(*expected));
        }

        // Non-UTF-8 data, a missing path and a directory.
        let store = GrantsFile::new(dir.write(b"\xff\xfe[[grant]]\n"));
        assert_eq!(store.count(), Err(GrantsProblem::Unreadable));
        assert_eq!(store.grant(&hash(1)), None);
        let store = GrantsFile::new(dir.0.join("absent.toml"));
        assert_eq!(store.count(), Err(GrantsProblem::Missing));
        assert_eq!(store.grant(&hash(1)), None);
        let store = GrantsFile::new(dir.0.clone());
        // Windows refuses to open a directory as a file; elsewhere the open succeeds and the
        // handle's kind refuses it.
        #[cfg(windows)]
        assert!(matches!(
            store.count(),
            Err(GrantsProblem::Unreadable | GrantsProblem::Missing)
        ));
        #[cfg(not(windows))]
        assert_eq!(store.count(), Err(GrantsProblem::NotARegularFile));
        assert_eq!(store.grant(&hash(1)), None);
    }

    /// The Windows NUL case reports NotARegularFile and returns no grant.
    #[cfg(windows)]
    #[test]
    fn ut_76_a_nul_grants_source_reports_not_a_regular_file_and_no_grant() {
        let store = GrantsFile::new("NUL");
        assert_eq!(store.count(), Err(GrantsProblem::NotARegularFile));
        assert_eq!(store.grant(&[1; 32]), None);
        let shown = GrantsProblem::NotARegularFile.to_string();
        assert!(!shown.contains("  "), "{shown:?}");
    }

    #[test]
    fn ut_76_the_bound_holds_at_one_byte() {
        let dir = Dir::new("bound");
        let valid = entry(&hash(1), "");
        let bound = usize::try_from(MAX_GRANTS_FILE_BYTES).unwrap();
        // Padded with a TOML comment to exactly the bound: read and valid.
        let mut at_bound = valid.clone().into_bytes();
        at_bound.extend(b"#");
        at_bound.resize(bound - 1, b'x');
        at_bound.push(b'\n');
        assert_eq!(at_bound.len(), bound);
        let store = GrantsFile::new(dir.write(&at_bound));
        assert!(store.grant(&hash(1)).is_some());
        // One byte more: the whole file is refused.
        at_bound.insert(0, b'\n');
        let store = GrantsFile::new(dir.write(&at_bound));
        assert_eq!(store.count(), Err(GrantsProblem::TooLarge));
        assert_eq!(store.grant(&hash(1)), None);
    }

    #[test]
    fn ut_76_rereads_the_edits_and_leaves_the_observed_file_state_unchanged() {
        let dir = Dir::new("reread");
        let path = dir.write(entry(&hash(1), "").as_bytes());
        let store = GrantsFile::new(&path);
        assert!(store.grant(&hash(1)).is_some());
        assert_eq!(store.grant(&hash(2)), None);

        // Withdrawn, then listed under another ticket: each question sees the file as it is.
        std::fs::write(&path, entry(&hash(2), "")).unwrap();
        assert_eq!(store.grant(&hash(1)), None);
        assert!(store.grant(&hash(2)).is_some());
        // Made invalid: nothing, not even the grant it had a moment ago.
        std::fs::write(&path, format!("{}junk", entry(&hash(2), ""))).unwrap();
        assert_eq!(store.grant(&hash(2)), None);
        // Removed, then put back.
        std::fs::remove_file(&path).unwrap();
        assert_eq!(store.grant(&hash(2)), None);
        std::fs::write(&path, entry(&hash(2), "")).unwrap();
        assert!(store.grant(&hash(2)).is_some());

        // Compare the bytes, modification time and directory-entry count around the read-only lookups.
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions.clone()).unwrap();
        let before = (
            std::fs::read(&path).unwrap(),
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            std::fs::read_dir(&dir.0).unwrap().count(),
        );
        for _ in 0..3 {
            assert!(store.grant(&hash(2)).is_some());
            assert_eq!(store.grant(&hash(9)), None);
        }
        let after = (
            std::fs::read(&path).unwrap(),
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            std::fs::read_dir(&dir.0).unwrap().count(),
        );
        assert_eq!(before, after);
        #[allow(clippy::permissions_set_readonly_false)] // to let the test directory go
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
    }

    #[test]
    fn ut_76_a_valid_file_with_a_budget_and_a_term_answers_them_in_the_grant() {
        let text = entry(&hash(1), "delivery_budget = 3\nterm_ms = 60000\n");
        let grant = parse(&text).unwrap().remove(&hash(1)).unwrap();
        assert_eq!(grant.delivery_budget, Some(3));
        assert_eq!(grant.term, Some(Duration::from_secs(60)));
        // Both optional keys accept zero and return Some(0) and Some(Duration::ZERO).
        let zero = parse(&entry(&hash(1), "delivery_budget = 0\nterm_ms = 0\n")).unwrap();
        assert_eq!(zero[&hash(1)].delivery_budget, Some(0));
        assert_eq!(zero[&hash(1)].term, Some(Duration::ZERO));
        // Absent is no limit.
        let absent = parse(&entry(&hash(1), "")).unwrap();
        assert_eq!(absent[&hash(1)].delivery_budget, None);
        assert_eq!(absent[&hash(1)].term, None);
        // Through the file, as the broker asks it.
        let dir = Dir::new("limits");
        let store = GrantsFile::new(dir.write(text.as_bytes()));
        assert_eq!(store.grant(&hash(1)), Some(grant));
    }

    #[test]
    fn ut_76_the_largest_budget_and_term_parse_and_the_next_values_do_not() {
        let largest = entry(
            &hash(1),
            "delivery_budget = 4294967295\nterm_ms = 9223372036854775807\n",
        );
        let grant = parse(&largest).unwrap().remove(&hash(1)).unwrap();
        assert_eq!(grant.delivery_budget, Some(u32::MAX));
        assert_eq!(
            grant.term,
            Some(Duration::from_millis(9_223_372_036_854_775_807))
        );
        let refused = [
            "delivery_budget = 4294967296\n",
            "delivery_budget = -1\n",
            "delivery_budget = 1.0\n",
            "delivery_budget = \"1\"\n",
            "delivery_budget = true\n",
            "term_ms = 9223372036854775808\n",
            "term_ms = 18446744073709551615\n",
            "term_ms = 18446744073709551616\n",
            "term_ms = -1\n",
            "term_ms = 1.5\n",
            "term_ms = \"60000\"\n",
            "term_ms = 1e3\n",
        ];
        let dir = Dir::new("limit-bounds");
        for extra in refused {
            let text = entry(&hash(1), extra);
            assert_eq!(
                parse(&text).err(),
                Some(GrantsProblem::NotTheForm),
                "{extra}"
            );
            let store = GrantsFile::new(dir.write(text.as_bytes()));
            assert_eq!(store.grant(&hash(1)), None, "{extra}");
        }
    }

    /// A key accepted by mistake fails the named test of its family: each file is valid but for
    /// one key, inside a `[[grant]]` or at the top level, and must void the whole file.
    fn assert_every_key_voids_the_file(family: &str, keys: &[&str]) {
        let dir = Dir::new(family);
        for key in keys {
            for value in ["1", "[\"click\"]", "\"x\""] {
                let inside = entry(&hash(1), &format!("{key} = {value}\n"));
                let top = format!("{key} = {value}\n{}", entry(&hash(1), ""));
                for text in [inside, top] {
                    assert_eq!(
                        parse(&text).err(),
                        Some(GrantsProblem::NotTheForm),
                        "{text}"
                    );
                    let store = GrantsFile::new(dir.write(text.as_bytes()));
                    for ticket in [hash(1), hash(2)] {
                        assert_eq!(store.grant(&ticket), None, "{text}");
                    }
                }
            }
        }
    }

    #[test]
    fn ut_76_keys_for_action_kinds_are_refused_with_the_whole_file() {
        assert_every_key_voids_the_file(
            "kinds",
            &[
                "action_kinds",
                "kinds",
                "action_kind",
                "actions",
                "allowed_actions",
            ],
        );
    }

    #[test]
    fn ut_76_keys_for_a_region_are_refused_with_the_whole_file() {
        assert_every_key_voids_the_file("region", &["region", "regions", "frame_size", "bbox"]);
    }

    #[test]
    fn ut_76_other_names_for_a_budget_or_a_term_are_refused_with_the_whole_file() {
        assert_every_key_voids_the_file(
            "aliases",
            &["budget", "term", "term_s", "term_secs", "max_deliveries"],
        );
    }

    #[test]
    fn ut_76_invalid_form_is_reported_without_the_fixture_key_or_hash() {
        let secret = "Zephyrquartz";
        let text = entry(&hash(1), &format!("{secret} = 1\n"));
        let problem = parse(&text).unwrap_err();
        assert_eq!(problem, GrantsProblem::NotTheForm);
        let shown = format!("{problem} {problem:?}");
        assert!(
            !shown.contains(secret) && !shown.contains(&hex(&hash(1))),
            "{shown}"
        );
    }
}
