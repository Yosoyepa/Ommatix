//! The local policy as the operator configured it (`[safety]`), and the operator's stop
//! (thesis chapter 08 §8.2, SEC-02).
//!
//! The stop is the operator's alone: it is engaged by creating the configured
//! `[safety] stop_file` (or through an in-process [`StopSwitch`]) and cleared by removing
//! it. No MCP tool and nothing on the screen can set or clear it, because the agent has no
//! port that writes the policy. The engine reads [`PolicyStore::current`] at the gate and
//! again right before each delivery, so the stop takes effect on the next action, even one
//! already past the gate; it never undoes input that was already delivered.
//!
//! The window pin (ADR-0020) works the same way: `[safety] window_pin_file` names a file
//! that holds the handle of the only window live input may go to. The operator (or the
//! harness it runs) writes it; no tool and nothing on the screen can. It is read on every
//! action, and a missing, empty or malformed file pins nothing, which allows no input.
//!
//! The purge file (`[safety] purge_file`, SEC-07, chapter 08 §8.4) asks the engine to delete
//! its state and cache. The operator creates it; the engine reads it at each request boundary
//! and honours each instance of it once, an instance being told apart by its modification time
//! and size. The engine never deletes the file: it stays where the operator put it, and creating
//! it again is a new request.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use ommatix_application::{LocalPolicy, PolicyStore, WindowPin};

/// An in-process stop the composition root can hold: engaged, it stops new input exactly
/// like the stop file. Cloning shares the same switch.
#[derive(Debug, Clone, Default)]
pub struct StopSwitch(Arc<AtomicBool>);

impl StopSwitch {
    pub fn engage(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn clear(&self) {
        self.0.store(false, Ordering::SeqCst);
    }

    pub fn is_engaged(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Whether an operator's file (the stop or the purge file) is there. Fails closed: a file
/// whose presence cannot be established (a permission error, an unreachable volume) counts as
/// present.
fn operator_file_present(path: &Path) -> bool {
    path.try_exists().unwrap_or(true)
}

/// The stderr line once the operator's purge request has been honoured (SEC-07).
const PURGE_NOTICE: &str =
    "ommatix: the engine's state and cache were deleted on the operator's request";

/// One instance of the purge file, told apart by its modification time and size. A file whose
/// metadata cannot be read has no stamp of its own: its one instance is honoured once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: Option<u64>,
}

impl FileStamp {
    fn of(path: &Path) -> Self {
        let metadata = std::fs::metadata(path).ok();
        Self {
            modified: metadata.as_ref().and_then(|m| m.modified().ok()),
            len: metadata.as_ref().map(std::fs::Metadata::len),
        }
    }
}

/// The purge file's state as the engine has seen it.
#[derive(Debug, Default)]
struct PurgeState {
    /// The instance honoured last. Cleared when the file is gone, so that the next file the
    /// operator creates is a new request.
    honoured: Option<FileStamp>,
    /// The instance that `purge_requested` reported and `purge_done` has yet to honour.
    pending: Option<FileStamp>,
}

/// Longest pin file read, in bytes: a decimal window handle is at most 20 digits.
const MAX_PIN_BYTES: u64 = 32;

/// The window the pin file names now. Fails closed: a file that is missing, unreadable,
/// longer than a handle, not valid text or not a positive decimal number pins no window.
fn read_window_pin(path: &Path) -> WindowPin {
    let mut text = String::new();
    let read = std::fs::File::open(path)
        .and_then(|file| file.take(MAX_PIN_BYTES + 1).read_to_string(&mut text));
    if read.is_err() || text.len() as u64 > MAX_PIN_BYTES {
        return WindowPin::Unset;
    }
    match text.trim().parse::<u64>() {
        Ok(handle) if handle > 0 => WindowPin::Window(handle),
        _ => WindowPin::Unset,
    }
}

/// A [`PolicyStore`] fixed at startup from the configuration file, plus the operator's
/// stop, read on every call. Nothing in the process loosens it, so the agent can never
/// change it (thesis chapter 08).
#[derive(Debug)]
pub struct ConfigPolicy {
    policy: LocalPolicy,
    stop_file: Option<PathBuf>,
    window_pin_file: Option<PathBuf>,
    purge_file: Option<PathBuf>,
    switch: StopSwitch,
    /// Whether to report the stop being engaged or cleared on stderr.
    notices: bool,
    /// The stop state last read, to report changes once.
    was_stopped: AtomicBool,
    /// Which instance of the purge file has been honoured.
    purge: Mutex<PurgeState>,
}

impl ConfigPolicy {
    pub fn new(policy: LocalPolicy) -> Self {
        Self {
            policy,
            stop_file: None,
            window_pin_file: None,
            purge_file: None,
            switch: StopSwitch::default(),
            notices: false,
            was_stopped: AtomicBool::new(false),
            purge: Mutex::new(PurgeState::default()),
        }
    }

    /// Engages the stop while `path` exists.
    pub fn with_stop_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.stop_file = Some(path.into());
        self
    }

    /// Asks the engine to delete its state and cache whenever the operator creates a new
    /// instance of the file at `path` (SEC-07, chapter 08 §8.4). The engine never deletes the
    /// file, and an instance it has honoured is not honoured again.
    pub fn with_purge_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.purge_file = Some(path.into());
        self
    }

    /// Pins live input to the window whose handle `path` holds (ADR-0020). From now on the
    /// policy carries a pin on every read, and it names no window, which allows no input,
    /// whenever the file does not hold a handle.
    pub fn with_window_pin_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.window_pin_file = Some(path.into());
        self
    }

    /// Prints a line on stderr whenever a read finds the stop engaged or cleared.
    pub fn with_stderr_notices(mut self) -> Self {
        self.notices = true;
        self
    }

    /// The configured stop file, if any.
    pub fn stop_file(&self) -> Option<&Path> {
        self.stop_file.as_deref()
    }

    /// A handle on this policy's in-process stop.
    pub fn stop_switch(&self) -> StopSwitch {
        self.switch.clone()
    }

    /// Why the stop is engaged now, or `None` when it is not.
    pub fn stop_reason(&self) -> Option<String> {
        if self.policy.stopped {
            return Some("the configuration says so".to_owned());
        }
        if self.switch.is_engaged() {
            return Some("the in-process stop switch".to_owned());
        }
        self.stop_file
            .as_deref()
            .filter(|path| operator_file_present(path))
            .map(|path| format!("the stop file {} exists", path.display()))
    }

    /// Reports on stderr, once, each time the stop is found engaged or cleared.
    fn notice(&self, reason: Option<&str>) {
        let stopped = reason.is_some();
        let was = self.was_stopped.swap(stopped, Ordering::SeqCst);
        if !self.notices || was == stopped {
            return;
        }
        eprintln!("{}", stop_notice(reason));
    }
}

/// The stderr line for the stop being engaged (`Some(reason)`) or cleared (`None`).
fn stop_notice(reason: Option<&str>) -> String {
    match reason {
        Some(reason) => format!(
            "ommatix: STOP engaged by the operator ({reason}): no new input will be sent; \
             actions already delivered keep their reported status"
        ),
        None => {
            "ommatix: stop cleared by the operator: [safety] governs live input again".to_owned()
        }
    }
}

impl PolicyStore for ConfigPolicy {
    fn current(&self) -> LocalPolicy {
        let reason = self.stop_reason();
        self.notice(reason.as_deref());
        LocalPolicy {
            stopped: reason.is_some(),
            window_pin: self.window_pin_file.as_deref().map(read_window_pin),
            ..self.policy.clone()
        }
    }

    /// A purge is requested by an instance of the purge file that has not been honoured yet.
    /// While the file is gone, nothing is requested and the honoured instance is forgotten, so
    /// the next file the operator creates is a new request.
    fn purge_requested(&self) -> bool {
        let Some(path) = self.purge_file.as_deref() else {
            return false;
        };
        let mut state = self.purge.lock().unwrap_or_else(PoisonError::into_inner);
        if !operator_file_present(path) {
            *state = PurgeState::default();
            return false;
        }
        let stamp = FileStamp::of(path);
        if state.honoured == Some(stamp) {
            return false;
        }
        state.pending = Some(stamp);
        true
    }

    /// The engine purged for the request reported last: that instance is honoured, and the
    /// file stays where the operator put it.
    fn purge_done(&self) {
        let mut state = self.purge.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(stamp) = state.pending.take() else {
            return;
        };
        state.honoured = Some(stamp);
        drop(state);
        if self.notices {
            eprintln!("{PURGE_NOTICE}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live() -> LocalPolicy {
        LocalPolicy {
            actuation_enabled: true,
            allowed_applications: vec!["notepad.exe".to_owned()],
            ..LocalPolicy::default()
        }
    }

    /// A fresh directory under the system's temporary directory.
    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ommatix-policy-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reports_what_it_was_built_with() {
        let policy = LocalPolicy {
            include_values: true,
            ..LocalPolicy::default()
        };
        assert_eq!(ConfigPolicy::new(policy.clone()).current(), policy);
    }

    #[test]
    fn the_stop_file_stops_while_it_exists_and_changes_nothing_else() {
        let dir = temp_dir("file");
        let file = dir.join("STOP");
        let store = ConfigPolicy::new(live()).with_stop_file(&file);
        assert_eq!(store.current(), live());
        assert_eq!(store.stop_reason(), None);

        std::fs::write(&file, b"").unwrap();
        let stopped = store.current();
        assert!(stopped.stopped);
        assert_eq!(
            LocalPolicy {
                stopped: false,
                ..stopped
            },
            live()
        );
        assert!(store.stop_reason().unwrap().contains("STOP"));

        std::fs::remove_file(&file).unwrap();
        assert_eq!(store.current(), live(), "removing the file clears the stop");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_switch_stops_until_it_is_cleared() {
        let store = ConfigPolicy::new(live());
        let switch = store.stop_switch();
        switch.engage();
        assert!(store.current().stopped);
        assert_eq!(
            store.stop_reason().as_deref(),
            Some("the in-process stop switch")
        );
        switch.clear();
        assert!(!store.current().stopped);
    }

    #[test]
    fn the_stop_notice_reads_as_one_sentence() {
        // Found in the S3 supervised runs: a lost line continuation put a run of spaces in
        // the middle of the STOP line on stderr.
        let engaged = stop_notice(Some("the stop file STOP exists"));
        assert_eq!(
            engaged,
            "ommatix: STOP engaged by the operator (the stop file STOP exists): no new input \
             will be sent; actions already delivered keep their reported status"
        );
        for line in [engaged, stop_notice(None)] {
            assert!(!line.contains("  "), "{line:?}");
        }
    }

    #[test]
    fn without_a_pin_file_no_window_is_pinned() {
        let store = ConfigPolicy::new(live());
        assert_eq!(store.current().window_pin, None);
    }

    #[test]
    fn the_pin_follows_the_file_and_a_file_that_names_no_window_pins_nothing() {
        let dir = temp_dir("pin");
        let file = dir.join("window.pin");
        let store = ConfigPolicy::new(live()).with_window_pin_file(&file);
        // Configured but missing: a pin that names no window, which allows no input.
        assert_eq!(store.current().window_pin, Some(WindowPin::Unset));

        std::fs::write(
            &file, b"265482
",
        )
        .unwrap();
        assert_eq!(
            store.current().window_pin,
            Some(WindowPin::Window(265_482)),
            "read again on every call"
        );
        std::fs::write(&file, b"  18446744073709551615 ").unwrap();
        assert_eq!(
            store.current().window_pin,
            Some(WindowPin::Window(u64::MAX))
        );
        for bad in [
            &b""[..],
            b"0",
            b"-5",
            b"0x1F",
            b"12 34",
            b"twelve",
            b"18446744073709551616",
            &[0xFF, 0xFE, 0x31][..],
            &[b'1'; 33][..],
        ] {
            std::fs::write(&file, bad).unwrap();
            assert_eq!(
                store.current().window_pin,
                Some(WindowPin::Unset),
                "{bad:?}"
            );
        }
        std::fs::remove_file(&file).unwrap();
        assert_eq!(store.current().window_pin, Some(WindowPin::Unset));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_directory_where_the_pin_file_should_be_pins_nothing() {
        let dir = temp_dir("pin-dir");
        let store = ConfigPolicy::new(live()).with_window_pin_file(&dir);
        assert_eq!(store.current().window_pin, Some(WindowPin::Unset));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_pin_changes_nothing_else_in_the_policy() {
        let dir = temp_dir("pin-else");
        let file = dir.join("window.pin");
        std::fs::write(&file, b"7").unwrap();
        let current = ConfigPolicy::new(live())
            .with_window_pin_file(&file)
            .current();
        assert_eq!(
            LocalPolicy {
                window_pin: None,
                ..current
            },
            live()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stop_in_the_configuration_cannot_be_cleared() {
        let store = ConfigPolicy::new(LocalPolicy {
            stopped: true,
            ..live()
        });
        store.stop_switch().clear();
        assert!(store.current().stopped);
    }

    /// SEC-02 and process termination: the stop file is the operator's stop that outlives the
    /// process (a restarted process reads it again), whereas the in-process switch dies with
    /// the process that holds it. Ctrl+C ends the process and runs no code of the engine.
    #[test]
    fn sec_02_the_stop_file_outlives_a_restart_and_the_in_process_switch_does_not() {
        let dir = temp_dir("restart");
        let file = dir.join("STOP");
        let engaged = ConfigPolicy::new(live()).with_stop_file(&file);
        engaged.stop_switch().engage();
        std::fs::write(&file, b"").unwrap();
        assert!(engaged.current().stopped);

        drop(engaged);
        let restarted = ConfigPolicy::new(live()).with_stop_file(&file);
        assert!(
            restarted.current().stopped,
            "the file still stops the new process"
        );
        assert!(
            !restarted.stop_switch().is_engaged(),
            "the switch of the old process is gone"
        );

        std::fs::remove_file(&file).unwrap();
        assert!(!restarted.current().stopped, "removing the file clears it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Writes the purge file and sets its modification time: one instance of the file, which
    /// a test can replace by another with the same size and a later time.
    fn purge_instance(path: &Path, contents: &[u8], modified_secs: u64) {
        std::fs::write(path, contents).unwrap();
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(modified_secs))
            .unwrap();
    }

    #[test]
    fn the_purge_file_requests_one_purge_per_instance_and_is_left_to_the_operator() {
        let dir = temp_dir("purge");
        let file = dir.join("PURGE");
        let store = ConfigPolicy::new(live()).with_purge_file(&file);
        assert!(!store.purge_requested(), "no file, no request");

        purge_instance(&file, b"", 1_000);
        assert!(store.purge_requested(), "the operator created the file");
        store.purge_done();
        assert!(
            file.exists(),
            "the file is the operator's: the engine does not delete it"
        );
        assert!(
            !store.purge_requested(),
            "an honoured instance is not honoured again"
        );

        purge_instance(&file, b"", 2_000);
        assert!(
            store.purge_requested(),
            "a new modification time is a new instance"
        );
        store.purge_done();

        purge_instance(&file, b"longer", 2_000);
        assert!(store.purge_requested(), "a new size is a new instance");
        store.purge_done();
        assert!(!store.purge_requested());

        std::fs::remove_file(&file).unwrap();
        assert!(!store.purge_requested(), "no file, no request");
        purge_instance(&file, b"longer", 2_000);
        assert!(
            store.purge_requested(),
            "a file that was removed and written again is a new request"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_purge_file_no_file_requests_a_purge() {
        let dir = temp_dir("purge-off");
        std::fs::write(dir.join("PURGE"), b"").unwrap();
        let store = ConfigPolicy::new(live());
        assert!(!store.purge_requested());
        store.purge_done();
        assert_eq!(store.current(), live());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_purge_request_changes_nothing_else_in_the_policy() {
        let dir = temp_dir("purge-else");
        let purge = dir.join("PURGE");
        let stop = dir.join("STOP");
        let store = ConfigPolicy::new(live())
            .with_purge_file(&purge)
            .with_stop_file(&stop);
        purge_instance(&purge, b"", 1_000);
        assert!(store.purge_requested());
        store.purge_done();
        assert_eq!(store.current(), live());
        assert_eq!(store.stop_reason(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_purge_notice_reads_as_the_dossier_sentence() {
        assert_eq!(
            PURGE_NOTICE,
            "ommatix: the engine's state and cache were deleted on the operator's request"
        );
    }
}
