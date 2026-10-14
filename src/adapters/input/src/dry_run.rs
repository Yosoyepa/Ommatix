use std::sync::{Arc, Mutex, PoisonError};

use ommatix_application::contract::{ActionKind, ValidatedAction};
use ommatix_application::{DeliveryReceipt, OsActuator, PortError};
use ommatix_domain::identity::TargetId;

/// One action a dry-run actuator was handed. In memory only; never logged with its text.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedAction {
    pub target_id: TargetId,
    pub kind: ActionKind,
    /// Physical pixel the action was aimed at.
    pub point: (f64, f64),
}

/// Shared, append-only record of what a [`DryRunInput`] was handed. Clone it before
/// handing the actuator to the engine to inspect it.
#[derive(Debug, Clone, Default)]
pub struct ActionLog(Arc<Mutex<Vec<RecordedAction>>>);

impl ActionLog {
    pub fn snapshot(&self) -> Vec<RecordedAction> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn push(&self, action: RecordedAction) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(action);
    }
}

/// An actuator that never touches the OS: `is_live() == false`, so the engine never
/// hands it a live action. If it is handed one anyway it records it, optionally notes it
/// on stderr (never stdout, which belongs to MCP, and never the typed text) and reports
/// that nothing was submitted.
#[derive(Debug, Clone, Default)]
pub struct DryRunInput {
    log: ActionLog,
    echo: bool,
}

impl DryRunInput {
    pub fn new(log: ActionLog) -> Self {
        Self { log, echo: false }
    }

    #[must_use]
    pub fn echo_to_stderr(mut self) -> Self {
        self.echo = true;
        self
    }

    fn record(&self, target_id: TargetId, kind: &ActionKind, point: (f64, f64)) -> DeliveryReceipt {
        if self.echo {
            let what = match kind {
                ActionKind::Click => "click".to_owned(),
                ActionKind::Type { text } => format!("type ({} chars)", text.chars().count()),
                ActionKind::Scroll { direction, steps } => {
                    format!("scroll {direction:?} x{steps}")
                }
            };
            eprintln!("ommatix: dry-run {what} on {target_id}");
        }
        self.log.push(RecordedAction {
            target_id,
            kind: kind.clone(),
            point,
        });
        DeliveryReceipt {
            submitted: 0,
            inserted: 0,
        }
    }
}

impl OsActuator for DryRunInput {
    fn name(&self) -> &'static str {
        "dry-run"
    }

    fn is_live(&self) -> bool {
        false
    }

    fn deliver(&mut self, action: &ValidatedAction) -> Result<DeliveryReceipt, PortError> {
        Ok(self.record(action.target_id(), action.kind(), action.point()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_in_order_and_submits_nothing() {
        let log = ActionLog::default();
        let input = DryRunInput::new(log.clone()).echo_to_stderr();
        assert!(!input.is_live());
        let e1: TargetId = "e1".parse().unwrap();
        let typed = ActionKind::Type { text: "hi".into() };
        let receipt = input.record(e1, &ActionKind::Click, (1.0, 2.0));
        assert_eq!(receipt.submitted, 0);
        input.record(e1, &typed, (3.0, 4.0));
        assert_eq!(
            log.snapshot(),
            vec![
                RecordedAction {
                    target_id: e1,
                    kind: ActionKind::Click,
                    point: (1.0, 2.0)
                },
                RecordedAction {
                    target_id: e1,
                    kind: typed,
                    point: (3.0, 4.0)
                }
            ]
        );
    }
}
