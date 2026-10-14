//! The local policy as the operator configured it (`[safety]`), and the operator's stop
//! (thesis chapter 08 §8.2, SEC-02).
//!
//! The stop is the operator's alone: it is engaged by creating the configured
//! `[safety] stop_file` (or through an in-process [`StopSwitch`]) and cleared by removing
//! it. No MCP tool and nothing on the screen can set or clear it, because the agent has no
//! port that writes the policy. The engine reads [`PolicyStore::current`] at the gate and
//! again right before each delivery, so the stop takes effect on the next action, even one
//! already past the gate; it never undoes input that was already delivered.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ommatix_application::{LocalPolicy, PolicyStore};

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

/// Whether the stop file is there. Fails closed: a file whose presence cannot be
/// established (a permission error, an unreachable volume) counts as present.
fn stop_file_present(path: &Path) -> bool {
    path.try_exists().unwrap_or(true)
}

/// A [`PolicyStore`] fixed at startup from the configuration file, plus the operator's
/// stop, read on every call. Nothing in the process loosens it, so the agent can never
/// change it (thesis chapter 08).
#[derive(Debug)]
pub struct ConfigPolicy {
    policy: LocalPolicy,
    stop_file: Option<PathBuf>,
    switch: StopSwitch,
    /// Whether to report the stop being engaged or cleared on stderr.
    notices: bool,
    /// The stop state last read, to report changes once.
    was_stopped: AtomicBool,
}

impl ConfigPolicy {
    pub fn new(policy: LocalPolicy) -> Self {
        Self {
            policy,
            stop_file: None,
            switch: StopSwitch::default(),
            notices: false,
            was_stopped: AtomicBool::new(false),
        }
    }

    /// Engages the stop while `path` exists.
    pub fn with_stop_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.stop_file = Some(path.into());
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
            .filter(|path| stop_file_present(path))
            .map(|path| format!("the stop file {} exists", path.display()))
    }

    /// Reports on stderr, once, each time the stop is found engaged or cleared.
    fn notice(&self, reason: Option<&str>) {
        let stopped = reason.is_some();
        let was = self.was_stopped.swap(stopped, Ordering::SeqCst);
        if !self.notices || was == stopped {
            return;
        }
        match reason {
            Some(reason) => eprintln!(
                "ommatix: STOP engaged by the operator ({reason}): no new input will be sent;                  actions already delivered keep their reported status"
            ),
            None => {
                eprintln!(
                    "ommatix: stop cleared by the operator: [safety] governs live input again"
                );
            }
        }
    }
}

impl PolicyStore for ConfigPolicy {
    fn current(&self) -> LocalPolicy {
        let reason = self.stop_reason();
        self.notice(reason.as_deref());
        LocalPolicy {
            stopped: reason.is_some(),
            ..self.policy.clone()
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
    fn a_stop_in_the_configuration_cannot_be_cleared() {
        let store = ConfigPolicy::new(LocalPolicy {
            stopped: true,
            ..live()
        });
        store.stop_switch().clear();
        assert!(store.current().stopped);
    }
}
