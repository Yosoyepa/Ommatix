//! How `serve` over the pipe ends (ADR-0043, proposed): it serves until the process is ended or
//! the pipe's listener stops accepting, and in the second case it never lingers as if serving.
//!
//! The listener stops accepting on its own only after a fatal error of accepting (the transient
//! ones, `ACCEPT_TRANSIENT_ERRORS`, are retried inside it), or if its accept thread ends without
//! one. `serve` looks every [`WATCH_EVERY`]; once the listener is not accepting, it shuts it down,
//! waiting at most [`SHUTDOWN_WAIT`] for its connections, and says on stderr why it stopped and
//! where the shutdown left the listener, with an exit code for each state:
//!
//! - **3**, complete: the listener reports its shutdown complete (see `ShutdownState`);
//! - **4**, connections remaining: the accept thread ended, but connections (or refusals) were
//!   still open at the end of the wait, each on a client that neither sent nor left; the end of
//!   the process closes them;
//! - **5**, the accept thread still alive: no wake-up connection reached it (another process of
//!   the same user may hold instances of the name); the end of the process ends it.
//!
//! A refusal to start stays exit code 1. The process ends right after the lines, so nothing of
//! the listener outlives them.

use std::time::Duration;

use ommatix_adapter_mcp::pipe::{PipeError, PipeServer, ShutdownReport, ShutdownState};

/// How often `serve` looks at whether the listener still accepts.
pub const WATCH_EVERY: Duration = Duration::from_secs(1);

/// How long `serve` waits, once the listener stopped accepting, for its connections to end.
pub const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);

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
            ShutdownState::Complete => {
                format!("shutdown reported complete; exit code {}", self.code())
            }
            ShutdownState::Remaining {
                connections,
                refusing,
            } => format!(
                "shutdown incomplete: {connections} connection(s) and {refusing} refusal(s) were \
                 still open after {} s, on clients that neither sent nor left; the end of this \
                 process closes them; exit code {}",
                SHUTDOWN_WAIT.as_secs(),
                self.code()
            ),
            ShutdownState::AcceptAlive => format!(
                "shutdown incomplete: the accept thread could not be woken and still waits on \
                 the pipe (another process of this user may hold instances of its name); the end \
                 of this process ends it; exit code {}",
                self.code()
            ),
        };
        [why, left]
    }
}

/// Waits while the listener accepts, looking every `every`; once it does not, shuts it down,
/// waiting at most `wait` for its connections, and returns how it ended.
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

    #[test]
    fn ut_80_serve_watches_while_the_listener_accepts_and_shuts_it_down_once_it_stops() {
        let remaining = ShutdownState::Remaining {
            connections: 2,
            refusing: 1,
        };
        for (looks, failure, state) in [
            (0, Some(FATAL), ShutdownState::Complete),
            (3, Some(FATAL), remaining),
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
        let remaining = ShutdownState::Remaining {
            connections: 2,
            refusing: 1,
        };
        let ended = |failure, state| Ended {
            failure,
            report: ShutdownReport {
                state,
                stats: PipeStats::default(),
                failure,
            },
        };
        let mut codes = Vec::new();
        for (state, code, says) in [
            (ShutdownState::Complete, 3, "shutdown reported complete"),
            (
                remaining,
                4,
                "2 connection(s) and 1 refusal(s) were still open",
            ),
            (
                ShutdownState::AcceptAlive,
                5,
                "the accept thread could not be woken",
            ),
        ] {
            let with_error = ended(Some(FATAL), state);
            assert_eq!(with_error.code(), code);
            let [why, left] = with_error.lines();
            assert!(why.contains(&FATAL.to_string()), "{why}");
            assert!(why.contains("no longer serving"), "{why}");
            assert!(left.contains(says), "{left}");
            assert!(left.ends_with(&format!("exit code {code}")), "{left}");
            let [why, _] = ended(None, state).lines();
            assert!(why.contains("without an error"), "{why}");
            codes.push(code);
        }
        // Distinct from each other, from success and from a refusal to start (1) or a usage
        // error (2).
        codes.dedup();
        assert_eq!(codes, vec![3, 4, 5]);
    }
}
