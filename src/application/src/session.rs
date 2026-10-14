use ommatix_domain::{Key, MouseButton};

use crate::{
    ActionReceipt, ActuationError, ActuationService, AppError, ClickTarget, Observation, Observer,
    PerceptionEngine, PerceptionReport,
};

/// Result of an action that was performed.
#[derive(Debug)]
pub struct ActionOutcome {
    pub receipt: ActionReceipt,
    /// What happened on screen afterwards: `None` when no observation was requested. An
    /// observation can fail on its own (e.g. the capture broke); the action still
    /// happened and must not be retried as if it had not.
    pub observation: Option<Result<Observation, AppError>>,
}

/// The use cases an agent drives, wired together: perceive, act, observe the effect.
///
/// Each use case stays independent; the session only adds the rule that an action's
/// observation never turns a performed action into a failure.
#[derive(Debug)]
pub struct AgentSession {
    perception: PerceptionEngine,
    observer: Observer,
    actuation: ActuationService,
}

impl AgentSession {
    pub fn new(
        perception: PerceptionEngine,
        observer: Observer,
        actuation: ActuationService,
    ) -> Self {
        Self {
            perception,
            observer,
            actuation,
        }
    }

    pub fn perception(&self) -> &PerceptionEngine {
        &self.perception
    }

    pub fn perception_mut(&mut self) -> &mut PerceptionEngine {
        &mut self.perception
    }

    pub fn actuation(&self) -> &ActuationService {
        &self.actuation
    }

    /// One perception tick; an exhausted source is an error here.
    pub fn perceive(&mut self) -> Result<PerceptionReport, AppError> {
        self.perception.tick()?.ok_or(AppError::SourceExhausted)
    }

    pub fn click(
        &mut self,
        target: ClickTarget,
        button: MouseButton,
        observe: bool,
    ) -> Result<ActionOutcome, ActuationError> {
        let receipt = self.actuation.click(&self.perception, target, button)?;
        Ok(self.observed(receipt, observe))
    }

    pub fn type_text(
        &mut self,
        text: &str,
        observe: bool,
    ) -> Result<ActionOutcome, ActuationError> {
        let receipt = self.actuation.type_text(text)?;
        Ok(self.observed(receipt, observe))
    }

    pub fn press_key(&mut self, key: Key, observe: bool) -> Result<ActionOutcome, ActuationError> {
        let receipt = self.actuation.press_key(key)?;
        Ok(self.observed(receipt, observe))
    }

    fn observed(&mut self, receipt: ActionReceipt, observe: bool) -> ActionOutcome {
        ActionOutcome {
            receipt,
            observation: observe.then(|| self.observer.observe(&mut self.perception)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ommatix_domain::{Action, Detection, Frame, Rect};

    use super::*;
    use crate::{
        ActuationPolicy, ElementDetector, FrameSource, InputController, ManualClock,
        PerceptionConfig, PortError, SettlePolicy,
    };

    /// Serves one frame, then runs dry.
    struct OneFrame(Option<Frame>);

    impl FrameSource for OneFrame {
        fn name(&self) -> &'static str {
            "one-frame"
        }
        fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
            Ok(self.0.take())
        }
    }

    struct NoElements;

    impl ElementDetector for NoElements {
        fn name(&self) -> &'static str {
            "none"
        }
        fn detect(&self, _: &Frame, _: Rect) -> Result<Vec<Detection>, PortError> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct Counter(Arc<std::sync::atomic::AtomicUsize>);

    impl InputController for Counter {
        fn name(&self) -> &'static str {
            "counter"
        }
        fn is_live(&self) -> bool {
            false
        }
        fn perform(&mut self, _: &Action) -> Result<(), PortError> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn a_failed_observation_never_hides_a_performed_action() {
        let clock = Arc::new(ManualClock::new());
        let frame = Frame::solid(32, 32, [255; 4]).unwrap();
        let perception = PerceptionEngine::new(
            Box::new(OneFrame(Some(frame))),
            Box::new(NoElements),
            clock.clone(),
            PerceptionConfig::default(),
        )
        .unwrap();
        let observer = Observer::new(clock, SettlePolicy::default()).unwrap();
        let input = Counter::default();
        let performed = Arc::clone(&input.0);
        let actuation = ActuationService::new(Box::new(input), ActuationPolicy::default());
        let mut session = AgentSession::new(perception, observer, actuation);

        session.perceive().unwrap();
        let outcome = session.type_text("hello", true).unwrap();
        assert_eq!(performed.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(matches!(
            outcome.observation,
            Some(Err(AppError::SourceExhausted))
        ));
        assert!(
            session
                .press_key(Key::Enter, false)
                .unwrap()
                .observation
                .is_none()
        );
        assert!(matches!(session.perceive(), Err(AppError::SourceExhausted)));
    }
}
