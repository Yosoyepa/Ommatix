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
//! [grant.window_classes]
//! "notepad.exe" = ["Notepad"]
//! ```
//!
//! [`GrantsFile`] reads it on **every** question (ADR-0040, decision 5), the way the stop, pin
//! and purge files of [`crate::policy`] are read on every action: it opens the file once, reads
//! at most [`MAX_GRANTS_FILE_BYTES`] plus one byte, parses it and answers the grant whose hash
//! matches, or none. So a grant the operator removes or changes takes effect at the client's next
//! call. It fails closed: the **whole** file is invalid, and answers none for every ticket, when it
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
                "it is not a regular file on a disk (a directory, a device or a named pipe is                  refused before anything is read)"
            }
            Self::Unreadable => "it cannot be read as UTF-8 text",
            Self::TooLarge => "it is larger than 65536 bytes",
            Self::NotTheForm => {
                "it is not a list of [[grant]] tables with exactly the keys ticket_sha256, \
                 applications, deliver_input, read_values and, optionally, window_classes and \
                 window_pin"
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

/// One `[[grant]]`: the four keys every grant states, and the two optional limits.
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
    fn ut_76_every_way_a_file_can_be_invalid_answers_none_for_every_ticket() {
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

        // Not UTF-8, missing, a directory, over the bound.
        let store = GrantsFile::new(dir.write(b"\xff\xfe[[grant]]\n"));
        assert_eq!(store.count(), Err(GrantsProblem::Unreadable));
        assert_eq!(store.grant(&hash(1)), None);
        let store = GrantsFile::new(dir.0.join("absent.toml"));
        assert_eq!(store.count(), Err(GrantsProblem::Missing));
        assert_eq!(store.grant(&hash(1)), None);
        let store = GrantsFile::new(dir.0.clone());
        assert!(matches!(
            store.count(),
            Err(GrantsProblem::Unreadable | GrantsProblem::Missing)
        ));
        assert_eq!(store.grant(&hash(1)), None);
    }

    /// A device that opens is not a grants file: refused before any read (without the check it
    /// would read as empty, a valid file without grants).
    #[cfg(windows)]
    #[test]
    fn ut_76_a_device_that_opens_is_refused_before_any_read() {
        let store = GrantsFile::new("NUL");
        assert_eq!(store.count(), Err(GrantsProblem::NotARegularFile));
        assert_eq!(store.grant(&[1; 32]), None);
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
    fn ut_76_the_file_is_read_again_on_every_question_and_never_written() {
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

        // A read-only file is enough, and the questions leave it and its directory unchanged.
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
    fn ut_76_a_problem_names_its_cause_and_nothing_of_the_file() {
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
