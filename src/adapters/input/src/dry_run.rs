use std::sync::{Arc, Mutex, PoisonError};

use ommatix_application::{InputController, PortError};
use ommatix_domain::Action;

/// Shared, append-only record of performed actions. Clone it before handing the
/// controller to the engine to inspect what the agent did.
#[derive(Debug, Clone, Default)]
pub struct ActionLog(Arc<Mutex<Vec<Action>>>);

impl ActionLog {
    pub fn snapshot(&self) -> Vec<Action> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn push(&self, action: Action) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(action);
    }
}

/// Accepts every action without touching the OS. Optionally echoes each action to
/// stderr (never stdout, which belongs to MCP).
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
}

impl InputController for DryRunInput {
    fn name(&self) -> &'static str {
        "dry-run"
    }

    fn is_live(&self) -> bool {
        false
    }

    fn perform(&mut self, action: &Action) -> Result<(), PortError> {
        if self.echo {
            eprintln!("ommatix: dry-run {action:?}");
        }
        self.log.push(action.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ommatix_domain::Key;

    use super::*;

    #[test]
    fn records_actions_in_order() {
        let log = ActionLog::default();
        let mut input = DryRunInput::new(log.clone());
        input.perform(&Action::PressKey { key: Key::Tab }).unwrap();
        input
            .perform(&Action::TypeText { text: "hi".into() })
            .unwrap();
        assert_eq!(
            log.snapshot(),
            vec![
                Action::PressKey { key: Key::Tab },
                Action::TypeText { text: "hi".into() }
            ]
        );
        assert!(!input.is_live());
    }
}
