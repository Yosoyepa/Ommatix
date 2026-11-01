//! The link between the engine's thread and the thread that owns the COM objects.
//!
//! `UiaProvider` keeps its UI Automation client on a worker thread of its own and talks to
//! it through two channels. This module is that conversation without the platform: safe
//! Rust, compiled and tested on every OS, so a worker that never answers can be tested with
//! a fake.
//!
//! A provider of another process that does not answer blocks the call that asked it, and a
//! cross-process UI Automation call has no timeout we can set (the pinned `uiautomation`
//! crate exposes `IUIAutomation` only, not the `IUIAutomation2` connection and transaction
//! timeouts). So the wait is bounded here, on the engine's side of the channel:
//!
//! - A snapshot that is not answered in time is an **empty, truncated read**: what the tree
//!   holds is unknown, not absent. The engine then keeps the visual path, leaves the
//!   regions it could not read unconfirmed and reads them again on its next observation
//!   (ADR-0017). An error would end the observation instead (`INFERENCE_FAILED`).
//! - The worker cannot be interrupted, so it stays occupied by the abandoned request. The
//!   link remembers that (`pending`) and **refuses fast** while it does: no second request
//!   is queued behind it, no thread is started, and the next call costs one `try_recv`.
//!   When the worker finally answers, its late reply is dropped and the link serves again.
//!   There is never more than one worker thread and one request in flight.
//! - Keyboard focus that is not answered is an error: `type` then has no confirmation and
//!   delivers no key (ADR-0018).

use std::fmt;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

use ommatix_application::{AccessibilityProvider, AccessibleElement, AccessibleRead, PortError};
use ommatix_domain::DisplayMapping;
use ommatix_domain::bbox::BBox;

use crate::{ADAPTER, Budget};

/// The live mapping from frame pixels to desktop coordinates (the capturer's).
pub(crate) type MappingSource = Box<dyn Fn() -> Result<DisplayMapping, PortError> + Send + Sync>;

/// What the engine's side asks of the worker.
pub(crate) enum Request {
    Snapshot {
        mapping: DisplayMapping,
        regions: Vec<BBox>,
    },
    Focus {
        mapping: DisplayMapping,
    },
}

/// What the worker answers.
pub(crate) enum Answer {
    Elements(AccessibleRead),
    Focus(Option<AccessibleElement>),
}

pub(crate) type Reply = Result<Answer, String>;

/// What one request came to.
enum Asked {
    Answered(Answer),
    /// The worker is busy with an earlier request, or did not answer this one in time.
    Unanswered,
}

fn ended() -> PortError {
    PortError::new(ADAPTER, "the UI Automation thread ended")
}

/// An [`AccessibilityProvider`] whose reads are done by a worker thread behind two channels,
/// with a bounded wait for every answer.
pub(crate) struct ThreadedProvider {
    requests: Sender<Request>,
    replies: Receiver<Reply>,
    mapping: MappingSource,
    /// How long one request may take: the walk's time budget plus a margin for the one call
    /// that can overrun it, since the budget is checked between calls.
    deadline: Duration,
    /// A request the worker has not answered yet: it was abandoned at its deadline and the
    /// worker is still inside it.
    pending: bool,
}

impl fmt::Debug for ThreadedProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThreadedProvider").finish_non_exhaustive()
    }
}

impl ThreadedProvider {
    pub(crate) fn over(
        requests: Sender<Request>,
        replies: Receiver<Reply>,
        mapping: MappingSource,
        budget: &Budget,
    ) -> Self {
        Self {
            requests,
            replies,
            mapping,
            deadline: budget.reply_deadline(),
            pending: false,
        }
    }

    /// Sends one request to the worker and waits at most the deadline for its answer.
    fn ask(&mut self, request: Request) -> Result<Asked, PortError> {
        if self.pending {
            match self.replies.try_recv() {
                // The late reply to the abandoned request: the worker is free again.
                Ok(_) => self.pending = false,
                Err(TryRecvError::Empty) => return Ok(Asked::Unanswered),
                Err(TryRecvError::Disconnected) => return Err(ended()),
            }
        }
        self.requests.send(request).map_err(|_| ended())?;
        match self.replies.recv_timeout(self.deadline) {
            Ok(reply) => reply
                .map(Asked::Answered)
                .map_err(|e| PortError::new(ADAPTER, e)),
            Err(RecvTimeoutError::Timeout) => {
                self.pending = true;
                Ok(Asked::Unanswered)
            }
            Err(RecvTimeoutError::Disconnected) => Err(ended()),
        }
    }
}

impl AccessibilityProvider for ThreadedProvider {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn snapshot(&mut self, regions: &[BBox]) -> Result<AccessibleRead, PortError> {
        if regions.is_empty() {
            return Ok(AccessibleRead::complete(Vec::new()));
        }
        let mapping = (self.mapping)()?;
        match self.ask(Request::Snapshot {
            mapping,
            regions: regions.to_vec(),
        })? {
            Asked::Answered(Answer::Elements(read)) => Ok(read),
            Asked::Answered(Answer::Focus(_)) => Err(PortError::new(ADAPTER, "unexpected reply")),
            // Unknown, not absent: the engine reads these regions again.
            Asked::Unanswered => Ok(AccessibleRead {
                elements: Vec::new(),
                truncated: true,
            }),
        }
    }

    fn reads_keyboard_focus(&self) -> bool {
        true
    }

    fn keyboard_focus(&mut self) -> Result<Option<AccessibleElement>, PortError> {
        let mapping = (self.mapping)()?;
        match self.ask(Request::Focus { mapping })? {
            Asked::Answered(Answer::Focus(element)) => Ok(element),
            Asked::Answered(Answer::Elements(_)) => {
                Err(PortError::new(ADAPTER, "unexpected reply"))
            }
            Asked::Unanswered => Err(PortError::new(
                ADAPTER,
                "the UI Automation provider did not answer in time",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Instant;

    use super::*;

    /// A budget with a short time bound, so the tests wait tens of milliseconds, not seconds.
    fn quick() -> Budget {
        Budget {
            max_duration: Duration::from_millis(20),
            ..Budget::default()
        }
    }

    fn everything() -> (DisplayMapping, BBox) {
        (
            DisplayMapping::identity(1000, 1000).expect("a valid size"),
            BBox::new(0.0, 0.0, 1000.0, 1000.0).expect("a valid box"),
        )
    }

    /// What the fake worker has been asked, and whether it may answer.
    #[derive(Default)]
    struct Gate {
        received: usize,
        open: bool,
    }

    /// A provider over a worker that takes requests and answers only while the gate is open:
    /// a closed gate is a provider that never answers.
    fn provider(budget: &Budget) -> (ThreadedProvider, Arc<Mutex<Gate>>) {
        let (requests, inbox) = mpsc::channel::<Request>();
        let (outbox, replies) = mpsc::channel::<Reply>();
        let gate = Arc::new(Mutex::new(Gate::default()));
        let seen = Arc::clone(&gate);
        thread::spawn(move || {
            for request in inbox {
                seen.lock().unwrap().received += 1;
                while !seen.lock().unwrap().open {
                    thread::sleep(Duration::from_millis(1));
                }
                let answer = match request {
                    Request::Snapshot { mapping, regions } => {
                        // The engine's side sends the live mapping and the regions asked for.
                        assert!(mapping.frame_box(0, 0, 10, 10).is_some());
                        assert_eq!(regions.len(), 1);
                        Answer::Elements(AccessibleRead::complete(vec![]))
                    }
                    Request::Focus { mapping } => {
                        assert!(mapping.frame_box(0, 0, 10, 10).is_some());
                        Answer::Focus(None)
                    }
                };
                if outbox.send(Ok(answer)).is_err() {
                    return;
                }
            }
        });
        let (mapping, _) = everything();
        let provider =
            ThreadedProvider::over(requests, replies, Box::new(move || Ok(mapping)), budget);
        (provider, gate)
    }

    /// Runs `f` on its own thread and fails the test, rather than hanging it, if `f` does
    /// not return within `limit`.
    fn within<T: Send + 'static>(limit: Duration, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(limit)
            .unwrap_or_else(|_| panic!("did not return within {limit:?}"))
    }

    fn all() -> Vec<BBox> {
        vec![everything().1]
    }

    #[test]
    fn a_provider_that_never_answers_does_not_stall_the_snapshot() {
        let (mut provider, _gate) = provider(&quick());
        let started = Instant::now();
        let read = within(Duration::from_secs(5), move || {
            provider.snapshot(&all()).unwrap()
        });
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the wait is bounded by the budget plus its margin"
        );
        assert_eq!(read.elements.len(), 0);
        assert!(
            read.truncated,
            "what the tree holds is unknown, not absent: the engine reads it again"
        );
    }

    #[test]
    fn a_read_after_a_timeout_refuses_fast_and_queues_nothing_behind_the_stuck_worker() {
        let (mut provider, gate) = provider(&quick());
        let (received, elapsed) = within(Duration::from_secs(5), move || {
            provider.snapshot(&all()).unwrap();
            let again = Instant::now();
            for _ in 0..50 {
                let read = provider.snapshot(&all()).unwrap();
                assert!(read.truncated);
                assert_eq!(read.elements.len(), 0);
            }
            (gate.lock().unwrap().received, again.elapsed())
        });
        assert_eq!(received, 1, "only the first request reached the worker");
        assert!(
            elapsed < Duration::from_millis(500),
            "fifty refused reads took {elapsed:?}: they must not each wait for the deadline"
        );
    }

    #[test]
    fn the_link_serves_again_once_the_stuck_worker_answers() {
        let (mut provider, gate) = provider(&quick());
        within(Duration::from_secs(5), move || {
            assert!(provider.snapshot(&all()).unwrap().truncated);
            gate.lock().unwrap().open = true;
            // The worker answers the abandoned request; the link drops that late reply
            // and carries the next request, which is answered in full.
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let read = provider.snapshot(&all()).unwrap();
                if !read.truncated {
                    break;
                }
                assert!(Instant::now() < deadline, "the link never recovered");
                thread::sleep(Duration::from_millis(5));
            }
        });
    }

    #[test]
    fn keyboard_focus_that_is_not_answered_is_an_error_not_a_focus() {
        let (mut provider, _gate) = provider(&quick());
        let (first, second) = within(Duration::from_secs(5), move || {
            (provider.keyboard_focus(), provider.keyboard_focus())
        });
        assert!(first.is_err(), "no answer is no confirmation of focus");
        assert!(second.is_err(), "the stuck worker is refused fast");
    }

    #[test]
    fn a_worker_that_answers_in_time_is_read_normally() {
        let (mut provider, gate) = provider(&Budget::default());
        gate.lock().unwrap().open = true;
        let read = within(Duration::from_secs(5), move || {
            provider.snapshot(&all()).unwrap()
        });
        assert!(!read.truncated);
    }

    #[test]
    fn a_worker_that_ended_is_an_error() {
        let (requests, inbox) = mpsc::channel::<Request>();
        let (outbox, replies) = mpsc::channel::<Reply>();
        drop((inbox, outbox));
        let (mapping, _) = everything();
        let mut provider =
            ThreadedProvider::over(requests, replies, Box::new(move || Ok(mapping)), &quick());
        assert!(provider.snapshot(&all()).is_err());
    }

    #[test]
    fn the_reply_deadline_is_the_time_budget_plus_a_margin() {
        let budget = Budget::default();
        assert!(budget.reply_deadline() > budget.max_duration);
        assert_eq!(
            budget.reply_deadline(),
            budget.max_duration + Budget::REPLY_MARGIN
        );
    }
}
