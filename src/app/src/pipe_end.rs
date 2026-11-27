//! How `serve` over the pipe ends (ADR-0043, proposed): it serves until the process is ended or
//! the pipe's listener stops accepting, and in the second case it never lingers as if serving.
//!
//! The listener stops accepting on its own only after a fatal error of accepting (the transient
//! ones, `ACCEPT_TRANSIENT_ERRORS`, are retried inside it), or if its accept thread ends without
//! one. `serve` looks every [`WATCH_EVERY`]; once the listener is not accepting, it shuts it down
//! once, with a deadline of [`SHUTDOWN_WAIT`] for the listener's own waits, and says on stderr
//! why it stopped and where the shutdown left the listener, with an exit code for each state of
//! `ShutdownState`:
//!
//! - **3**, complete: the accept thread, every worker and the reaper are joined;
//! - **4**, remaining: at the end of the wait, workers were not joined yet (connections or
//!   refusals still held: a client that neither sends nor leaves, or a request that passed the
//!   listener's stop check before the stop and still runs), or no worker was left and only the
//!   reaper's end was awaited (`only_reaper_left`); the end of the process ends them;
//! - **5**, the accept thread still alive: no wake-up reached the listener's own instance
//!   (another process that can open the name may hold instances of it); the end of the process
//!   ends it.
//!
//! When workers of the listener had panicked, the second line counts them
//! (`PipeStats::worker_panics`); the listener keeps their panic payloads, and the end of the
//! process frees them.
//!
//! A refusal to start is exit code 1, except one: **7** ([`BIND_REFUSED`]), when the system
//! refused to create the first instance of the pipe's name with access denied. The usual cause
//! is a name another process already holds (another `serve` with the same name, or another
//! program), but access denied alone does not prove it, and the message says so. The process
//! ends right after the lines, without dropping
//! the listener: `std::process::exit` runs no destructor, so the listener's limit for a server
//! dropped before it is complete (the broker's final teardown on a detached thread) does not
//! arise here, and the end of the process is that teardown.

use std::time::Duration;

use ommatix_adapter_mcp::pipe::{PipeError, PipeServer, ShutdownReport, ShutdownState};

/// How often `serve` looks at whether the listener still accepts.
pub const WATCH_EVERY: Duration = Duration::from_secs(1);

/// The deadline `serve` gives the listener's shutdown, once it stopped accepting.
pub const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);

/// `serve`'s exit code when the system refused the first instance of the pipe's name with
/// access denied ([`BindRefused`]). Distinct from 1 (any other refusal to start, every other
/// bind failure included) and from 3, 4 and 5 (the listener's shutdown).
pub const BIND_REFUSED: i32 = 7;

/// `serve` could not create the first instance of the pipe's name: the system answered access
/// denied (the adapter's `PipeError::NameTaken`, which is that error and nothing more). It is
/// printed as one line and ends the process with [`BIND_REFUSED`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindRefused {
    /// The pipe's path, as the configuration gave its name.
    pub path: String,
}

impl std::fmt::Display for BindRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cannot serve on the pipe {}: the system refused to create the first instance of \
             this name with access denied (os error 5); the usual cause is that another process \
             already holds the name (another `ommatix serve` with the same name, or another \
             program), but access denied alone does not prove it; exit code {BIND_REFUSED}",
            self.path
        )
    }
}

impl std::error::Error for BindRefused {}

/// What `serve` needs of the listener: [`PipeServer`], or a double in tests.
pub trait Listening {
    /// Whether it accepts connections now.
    fn accepting(&self) -> bool;
    /// The error that stopped it accepting, if one did.
    fn failure(&self) -> Option<PipeError>;
    /// Waits about `wait`, or less.
    fn pause(&self, wait: Duration);
    /// Stops it and says what is left.
    fn shutdown(&mut self, wait: Duration) -> ShutdownReport;
}

impl Listening for PipeServer {
    fn accepting(&self) -> bool {
        PipeServer::accepting(self)
    }

    fn failure(&self) -> Option<PipeError> {
        PipeServer::failure(self)
    }

    fn pause(&self, wait: Duration) {
        let _ = self.wait_until(wait, |_| false);
    }

    fn shutdown(&mut self, wait: Duration) -> ShutdownReport {
        PipeServer::shutdown(self, wait)
    }
}

/// How the listener ended, as `serve` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ended {
    /// The error that stopped it accepting; `None` when its accept thread ended without one.
    pub failure: Option<PipeError>,
    /// Where the shutdown left it.
    pub report: ShutdownReport,
}

impl Ended {
    /// The process's exit code: 3, 4 or 5 by [`ShutdownState`] (see the module).
    pub const fn code(&self) -> i32 {
        match self.report.state {
            ShutdownState::Complete => 3,
            ShutdownState::Remaining { .. } => 4,
            ShutdownState::AcceptAlive => 5,
        }
    }

    /// The stderr lines, each after `ommatix: `: why it stopped, then where it was left. They
    /// carry counts and an error's kind and code, never traffic or a ticket.
    pub fn lines(&self) -> [String; 2] {
        let why = match self.failure {
            Some(failure) => format!(
                "the pipe's listener stopped accepting after an error that is not transient: \
                 {failure}; serve is no longer serving"
            ),
            None => "the pipe's listener stopped accepting: its accept thread ended without an \
                     error; serve is no longer serving"
                .to_owned(),
        };
        let left = match self.report.state {
            ShutdownState::Complete => "shutdown complete: the accept thread, every worker and \
                                        the reaper were joined"
                .to_owned(),
            ShutdownState::Remaining {
                only_reaper_left: true,
                ..
            } => format!(
                "shutdown incomplete after {} s: no worker was left, but the listener's reaper \
                 had not ended; the end of this process ends it",
                SHUTDOWN_WAIT.as_secs()
            ),
            ShutdownState::Remaining {
                connections,
                refusing,
                only_reaper_left: false,
            } => format!(
                "shutdown incomplete after {} s: {connections} connection worker(s) and \
                 {refusing} refusal worker(s) were not joined yet (a client that neither sends \
                 nor leaves, or a request that passed the stop check and still runs); the end of \
                 this process ends them",
                SHUTDOWN_WAIT.as_secs()
            ),
            ShutdownState::AcceptAlive => "shutdown incomplete: no wake-up reached the \
                                           listener's own instance, so its accept thread still \
                                           waits on the pipe (another process that can open \
                                           the name may hold instances of it); the end of this \
                                           process ends it"
                .to_owned(),
        };
        let panics = match self.report.stats.worker_panics {
            0 => String::new(),
            n => format!("; {n} worker thread(s) of the listener had panicked"),
        };
        [why, format!("{left}{panics}; exit code {}", self.code())]
    }
}

/// Waits while the listener accepts, looking every `every`; once it does not, shuts it down
/// once with the deadline `wait`, and returns how it ended.
pub fn watch(listener: &mut impl Listening, every: Duration, wait: Duration) -> Ended {
    while listener.accepting() {
        listener.pause(every);
    }
    let failure = listener.failure();
    let report = listener.shutdown(wait);
    Ended {
        failure: failure.or(report.failure),
        report,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io;

    use ommatix_adapter_mcp::pipe::PipeStats;

    use super::*;

    /// A listener that accepts for `accepting_for` looks, then stops with `failure` and shuts
    /// down to `state`.
    struct Scripted {
        accepting_for: Cell<u32>,
        failure: Option<PipeError>,
        state: ShutdownState,
        paused: Cell<u32>,
        shutdowns: Vec<Duration>,
    }

    impl Scripted {
        fn new(accepting_for: u32, failure: Option<PipeError>, state: ShutdownState) -> Self {
            Self {
                accepting_for: Cell::new(accepting_for),
                failure,
                state,
                paused: Cell::new(0),
                shutdowns: Vec::new(),
            }
        }
    }

    impl Listening for Scripted {
        fn accepting(&self) -> bool {
            self.accepting_for.get() > 0
        }

        fn failure(&self) -> Option<PipeError> {
            (!self.accepting()).then_some(self.failure).flatten()
        }

        fn pause(&self, wait: Duration) {
            assert_eq!(wait, WATCH_EVERY);
            assert!(self.shutdowns.is_empty(), "no pause after the shutdown");
            self.paused.set(self.paused.get() + 1);
            self.accepting_for.set(self.accepting_for.get() - 1);
        }

        fn shutdown(&mut self, wait: Duration) -> ShutdownReport {
            assert!(!self.accepting(), "never shut down while it accepts");
            self.shutdowns.push(wait);
            ShutdownReport {
                state: self.state,
                stats: PipeStats::default(),
                failure: self.failure,
            }
        }
    }

    const FATAL: PipeError = PipeError::Os {
        kind: io::ErrorKind::PermissionDenied,
        code: Some(5),
    };

    const REMAINING: ShutdownState = ShutdownState::Remaining {
        connections: 2,
        refusing: 1,
        only_reaper_left: false,
    };

    const ONLY_REAPER: ShutdownState = ShutdownState::Remaining {
        connections: 0,
        refusing: 0,
        only_reaper_left: true,
    };

    fn ended(failure: Option<PipeError>, state: ShutdownState, worker_panics: u64) -> Ended {
        Ended {
            failure,
            report: ShutdownReport {
                state,
                stats: PipeStats {
                    worker_panics,
                    ..PipeStats::default()
                },
                failure,
            },
        }
    }

    #[test]
    fn ut_80_serve_watches_while_the_listener_accepts_and_shuts_it_down_once_it_stops() {
        for (looks, failure, state) in [
            (0, Some(FATAL), ShutdownState::Complete),
            (3, Some(FATAL), REMAINING),
            (2, None, ONLY_REAPER),
            (5, None, ShutdownState::AcceptAlive),
        ] {
            let mut listener = Scripted::new(looks, failure, state);
            let ended = watch(&mut listener, WATCH_EVERY, SHUTDOWN_WAIT);
            assert_eq!(listener.paused.get(), looks, "it looks while it accepts");
            assert_eq!(
                listener.shutdowns,
                vec![SHUTDOWN_WAIT],
                "one bounded shutdown"
            );
            assert_eq!(ended.failure, failure);
            assert_eq!(ended.report.state, state);
        }
    }

    #[test]
    fn ut_80_each_way_the_listener_was_left_has_its_own_exit_code_and_line() {
        let mut codes = Vec::new();
        for (state, code, says) in [
            (
                ShutdownState::Complete,
                3,
                "shutdown complete: the accept thread, every worker and the reaper were joined",
            ),
            (
                REMAINING,
                4,
                "2 connection worker(s) and 1 refusal worker(s) were not joined yet",
            ),
            (
                ONLY_REAPER,
                4,
                "no worker was left, but the listener's reaper had not ended",
            ),
            (
                ShutdownState::AcceptAlive,
                5,
                "no wake-up reached the listener's own instance",
            ),
        ] {
            let with_error = ended(Some(FATAL), state, 0);
            assert_eq!(with_error.code(), code);
            let [why, left] = with_error.lines();
            assert!(why.contains(&FATAL.to_string()), "{why}");
            assert!(why.contains("no longer serving"), "{why}");
            assert!(left.contains(says), "{left}");
            assert!(!left.contains("panicked"), "{left}");
            assert!(left.ends_with(&format!("; exit code {code}")), "{left}");
            let [why, _] = ended(None, state, 0).lines();
            assert!(why.contains("without an error"), "{why}");
            codes.push(code);
        }
        // One code per state, distinct from each other, from success and from a refusal to start
        // (1) or a usage error (2); the two kinds of `Remaining` share 4 and differ by their line.
        codes.dedup();
        assert_eq!(codes, vec![3, 4, 5]);
    }

    #[test]
    fn ut_80_workers_that_panicked_are_counted_on_the_line_and_change_no_code() {
        for (state, code) in [
            (ShutdownState::Complete, 3),
            (REMAINING, 4),
            (ShutdownState::AcceptAlive, 5),
        ] {
            let quiet = ended(Some(FATAL), state, 0);
            let panicked = ended(Some(FATAL), state, 2);
            assert_eq!(panicked.code(), code);
            let [_, left] = panicked.lines();
            assert!(
                left.ends_with(&format!(
                    "; 2 worker thread(s) of the listener had panicked; exit code {code}"
                )),
                "{left}"
            );
            assert_eq!(quiet.lines()[0], panicked.lines()[0]);
        }
    }

    /// The refused first instance has a code of its own, distinct from success, from a refusal
    /// to start (1), a usage error (2) and the shutdown states (3, 4, 5); its line names the
    /// path and the error, and calls a taken name the usual cause, not a certainty.
    #[test]
    fn ut_80_a_refused_first_instance_has_its_own_exit_code_and_an_accurate_line() {
        assert!(![0, 1, 2, 3, 4, 5].contains(&BIND_REFUSED));
        assert_eq!(BIND_REFUSED, 7);
        let line = BindRefused {
            path: r"\\.\pipe\ommatix-x".to_owned(),
        }
        .to_string();
        assert!(
            line.starts_with(r"cannot serve on the pipe \\.\pipe\ommatix-x: "),
            "{line}"
        );
        for says in [
            "refused to create the first instance of this name with access denied (os error 5)",
            "the usual cause is that another process already holds the name",
            "access denied alone does not prove it",
            "; exit code 7",
        ] {
            assert!(line.contains(says), "{line}");
        }
        assert!(!line.contains('\n') && !line.contains("  "), "{line:?}");
    }
}
