//! The action gate (SDD §2.9, ADR-0007) and the honest delivery report (chapter 07 §7.5).
//!
//! Order: arguments → `request_id` deduplication → a snapshot that reflects the last
//! observation and has not been spent by a delivery → session, epoch, snapshot, target,
//! revision and actionability (the registry's checks) → the action's own needs → dry run
//! (`simulated`, the default) or, only when the operator's policy allows it, a live
//! actuator exists and attests an allowlisted foreground application, a freshness check
//! (the screen is still the committed base), a last look at the operator's stop and the
//! foreground application right before the events go out, delivery and a report of
//! exactly what is known: `input_delivered` never claims an effect, and any doubt is
//! `indeterminate`, never an automatic retry. Any delivery spends the snapshot: the next
//! action needs a new observation.
//!
//! The operator's scope on windows (ADR-0020) narrows the application allowlist to windows:
//! when the policy lists window classes for the foreground application, or pins a window,
//! the foreground window and the window under the action's point must be within it, checked
//! at the gate and again in the last look. The actuator only attests the windows.
//!
//! `type` is delivered in two phases when accessibility evidence reads keyboard focus
//! (ADR-0018, SDD §2.9): the focusing click, then a poll on the injected clock until UI
//! Automation reports keyboard focus on the target element, then the keys. No focus in
//! time means no key at all: the click was delivered, so the action is `indeterminate`.
//!
//! The operator's stop (chapter 08 §8.2, SEC-02) is read from the policy store at the
//! gate and again after the freshness capture, immediately before delivery: an action
//! that meets it is `POLICY_DENIED` and sends nothing. It never undoes what was already
//! delivered: those results stay recorded as they were reported.
//!
//! The snapshot and the accessibility evidence that confirms the target must be within
//! `[engine] max_snapshot_age_ms` (ADR-0025). The capture instant bounds the snapshot, the
//! instant its accessibility read started bounds the evidence, both on the injected clock.
//! An expired bound is `STALE_SNAPSHOT`: observe again, never re-target. The gate checks it
//! (simulated actions included), and so does each check right before a real input, after
//! the freshness capture and, for `type`, after the focusing click.

use std::time::Duration;

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Role;
use ommatix_domain::identity::{
    ActionRef, ActionRejection, ActionStatus, Clearance, Delivery, TargetId,
};
use ommatix_domain::incremental::{BgraFrame, dirty_tiles};

use super::accessibility::focus_is_on;
use super::journal::{Lookup, arguments_hash};
use super::{Engine, aged_out};
use crate::contract::{
    ActionKind, ActionRequest, ActionResult, ActionTimings, ErrorCode, ExpectedEffect,
    ObservedEffect, ToolError, ValidatedAction, limits,
};
use crate::ports::LocalPolicy;
use crate::window_scope::{window_scope_applies, window_scope_problem};

/// Parses `s<n>` (`n >= 1`, no leading zeros) as the registry's snapshot number.
fn parse_snapshot_id(text: &str) -> Option<u64> {
    let digits = text.strip_prefix('s')?;
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn rejection_error(rejection: ActionRejection) -> ToolError {
    match rejection {
        ActionRejection::SessionMismatch => ToolError::new(
            ErrorCode::SessionMismatch,
            "the action belongs to another session",
        ),
        ActionRejection::EpochMismatch => ToolError::new(
            ErrorCode::EpochMismatch,
            "the screen geometry changed since that observation",
        ),
        ActionRejection::StaleSnapshot => stale_snapshot(),
        ActionRejection::TargetNotFound => ToolError::new(
            ErrorCode::TargetNotFound,
            "no live element has that target ID",
        ),
        ActionRejection::StaleTarget => ToolError::new(
            ErrorCode::StaleTarget,
            "the element changed since that observation",
        ),
        ActionRejection::NotActionable(reason) => ToolError::new(
            ErrorCode::NotActionable,
            format!("the element cannot be acted on: {reason}"),
        ),
        ActionRejection::ActuatorUnavailable => policy_denied(),
    }
}

fn stale_snapshot() -> ToolError {
    ToolError::new(
        ErrorCode::StaleSnapshot,
        "the action does not reference the published snapshot; observe again",
    )
}

/// The snapshot was captured more than `[engine] max_snapshot_age_ms` ago (ADR-0025).
fn snapshot_expired() -> ToolError {
    ToolError::new(
        ErrorCode::StaleSnapshot,
        "the snapshot is older than [engine] max_snapshot_age_ms; observe again",
    )
}

/// The accessibility evidence that confirms the target was read more than
/// `[engine] max_snapshot_age_ms` ago (ADR-0025).
fn evidence_expired() -> ToolError {
    ToolError::new(
        ErrorCode::StaleSnapshot,
        "the accessibility evidence for the target is older than [engine] max_snapshot_age_ms; \
         observe again",
    )
}

fn policy_denied() -> ToolError {
    ToolError::new(
        ErrorCode::PolicyDenied,
        "live input is not allowed by the local policy or no live actuator is available",
    )
}

fn operator_stopped() -> ToolError {
    ToolError::new(
        ErrorCode::PolicyDenied,
        "the operator stopped live input; nothing was delivered",
    )
}

fn not_allowlisted(message: &str) -> ToolError {
    ToolError::new(ErrorCode::PolicyDenied, message)
}

fn indeterminate(message: &str) -> ToolError {
    ToolError::new(ErrorCode::IndeterminateEffect, message)
}

/// Bounds the schema already enforces, checked again because the engine trusts no caller.
fn check_arguments(request: &ActionRequest) -> Result<(), ToolError> {
    let too_long = |s: &str| s.is_empty() || s.chars().count() > limits::MAX_ID_CHARS;
    if too_long(&request.request_id) {
        return Err(ToolError::new(
            ErrorCode::LimitExceeded,
            "request_id must have 1 to 64 characters",
        ));
    }
    match &request.action {
        ActionKind::Type { text }
            if text.is_empty() || text.chars().count() > limits::MAX_TYPE_TEXT_CHARS =>
        {
            Err(ToolError::new(
                ErrorCode::LimitExceeded,
                "type text must have 1 to 2048 characters",
            ))
        }
        ActionKind::Scroll { steps, .. } if !(1..=limits::MAX_SCROLL_STEPS).contains(steps) => {
            Err(ToolError::new(
                ErrorCode::LimitExceeded,
                "scroll steps must be between 1 and 5",
            ))
        }
        _ => Ok(()),
    }
}

/// The stages of an action that [`ActionTimings`] tells apart.
#[derive(Debug, Clone, Copy)]
enum Stage {
    Freshness,
    Delivery,
    Focus,
    Verification,
}

/// What the gate and the actuator established about one action.
struct Outcome {
    status: ActionStatus,
    error: Option<ToolError>,
    observed_effect: Option<ObservedEffect>,
    /// The actuator reported every event inserted.
    delivered: bool,
}

impl Outcome {
    fn rejected(error: ToolError) -> Self {
        Self {
            status: ActionStatus::Rejected,
            error: Some(error),
            observed_effect: None,
            delivered: false,
        }
    }

    const fn simulated() -> Self {
        Self {
            status: ActionStatus::Simulated,
            error: None,
            observed_effect: None,
            delivered: false,
        }
    }

    const fn delivered(status: ActionStatus, observed_effect: Option<ObservedEffect>) -> Self {
        Self {
            status,
            error: None,
            observed_effect,
            delivered: true,
        }
    }

    /// `indeterminate`: `delivered` says whether the actuator reported the input
    /// delivered; the effect is unknown either way.
    fn uncertain(delivered: bool, message: &str) -> Self {
        Self {
            status: ActionStatus::Indeterminate,
            error: Some(indeterminate(message)),
            observed_effect: Some(ObservedEffect::Unknown),
            delivered,
        }
    }
}

impl Engine {
    pub(super) fn act(&mut self, request: &ActionRequest) -> ActionResult {
        // A replay or a refusal before the gate has no timings of its own.
        self.action_timings = None;
        let reply = |outcome: Outcome| ActionResult {
            request_id: request.request_id.clone(),
            status: outcome.status,
            target_id: request.target_id.clone(),
            snapshot_id: request.snapshot_id.clone(),
            error: outcome.error,
            observed_effect: outcome.observed_effect,
            delivered: outcome.delivered,
        };
        if let Err(error) = check_arguments(request) {
            self.counters.rejected_actions += 1;
            return reply(Outcome::rejected(error));
        }
        let hash = arguments_hash(request);
        match self.journal.lookup(&request.request_id, hash) {
            Lookup::Replay(stored) => return stored,
            Lookup::Conflict => {
                self.counters.rejected_actions += 1;
                let error = ToolError::new(
                    ErrorCode::RequestConflict,
                    "this request_id was already used with other arguments",
                );
                return reply(Outcome::rejected(error));
            }
            Lookup::New => {}
        }

        let started = self.ports.clock.now();
        self.action_timings = Some(ActionTimings::default());
        let result = reply(
            self.gate_and_perform(request)
                .unwrap_or_else(Outcome::rejected),
        );
        // The gate is what no other stage accounts for.
        let total = self.ports.clock.now().saturating_sub(started);
        if let Some(t) = self.action_timings.as_mut() {
            let staged = t.freshness + t.delivery + t.focus + t.verification;
            t.gate = total.saturating_sub(staged);
            t.total = total;
        }
        // Every action whose input the actuator reported delivered counts, including an
        // indeterminate one whose effect could not be verified.
        if result.status == ActionStatus::Rejected {
            self.counters.rejected_actions += 1;
        } else if result.delivered {
            self.counters.delivered_actions += 1;
        }
        self.journal
            .record(&request.request_id, hash, result.clone());
        result
    }

    /// Adds the time since `since` to `stage` of the running action.
    fn spend(&mut self, since: Duration, stage: Stage) {
        let elapsed = self.ports.clock.now().saturating_sub(since);
        if let Some(t) = self.action_timings.as_mut() {
            let slot = match stage {
                Stage::Freshness => &mut t.freshness,
                Stage::Delivery => &mut t.delivery,
                Stage::Focus => &mut t.focus,
                Stage::Verification => &mut t.verification,
            };
            *slot += elapsed;
        }
    }

    /// Runs the checks and, if they pass, simulates or delivers the action.
    fn gate_and_perform(&mut self, request: &ActionRequest) -> Result<Outcome, ToolError> {
        if !self.store.actions_allowed() || self.snapshot_spent {
            return Err(stale_snapshot());
        }
        let published = self.store.published().ok_or_else(stale_snapshot)?;
        let snapshot_id = parse_snapshot_id(&request.snapshot_id).ok_or_else(stale_snapshot)?;
        let action = ActionRef {
            session_id: &request.session_id,
            geometry_epoch: request.geometry_epoch,
            snapshot_id,
            target_id: &request.target_id,
            expected_revision: request.expected_revision,
        };
        let registry = &published.registry;
        let target_id = match registry
            .check_action(&action, Delivery::DryRun)
            .map_err(rejection_error)?
        {
            Clearance::Simulate { target_id } | Clearance::Deliver { target_id, .. } => target_id,
        };
        let (role, target_box) = registry
            .get(target_id)
            .map(|t| (t.observation.role, t.observation.bbox))
            .ok_or_else(|| rejection_error(ActionRejection::TargetNotFound))?;
        if matches!(request.action, ActionKind::Type { .. }) && role != Role::Input {
            return Err(ToolError::new(
                ErrorCode::NotActionable,
                "type needs a confirmed input field",
            ));
        }
        // Simulation meets the same bound, so it never accepts what the live path would
        // refuse. It sends nothing, so it has no boundary after this one (ADR-0025).
        self.check_age(target_id)?;

        if request.dry_run {
            return Ok(Outcome::simulated());
        }

        let policy = self.ports.policy.current();
        if policy.stopped {
            return Err(operator_stopped());
        }
        let allowed = policy.actuation_enabled && self.ports.actuator.is_live();
        if !allowed {
            return Err(policy_denied());
        }
        // Fail closed on the application allowlist (chapter 08): an empty list allows
        // nothing, and a foreground application the actuator cannot attest is not on it.
        if policy.allowed_applications.is_empty() {
            return Err(not_allowlisted("no application is on the local allowlist"));
        }
        let foreground = self.ports.actuator.foreground_application();
        if !foreground
            .as_ref()
            .is_some_and(|app| policy.allowed_applications.contains(app))
        {
            return Err(not_allowlisted(
                "foreground application is not on the local allowlist",
            ));
        }
        let Clearance::Deliver { bbox, .. } = registry
            .check_action(
                &action,
                Delivery::Live {
                    actuator_available: true,
                },
            )
            .map_err(rejection_error)?
        else {
            return Err(policy_denied());
        };
        let point = bbox.center();
        // The operator's scope on windows (ADR-0020): the allowlist names a process, and
        // the scope narrows it to the windows of that process that may be acted on.
        self.check_window_scope(&policy, foreground.as_deref(), point)?;
        let since = self.ports.clock.now();
        let checked = self.fresh_frame(target_id, &target_box).and_then(|fresh| {
            self.last_look(foreground.as_deref(), point, target_id)?;
            self.check_freshness_group(&fresh, &target_box, foreground.as_deref(), target_id)?;
            self.check_age(target_id)?;
            Ok(fresh.frame)
        });
        self.spend(since, Stage::Freshness);
        let before = checked?;
        let validated = ValidatedAction {
            target_id,
            kind: request.action.clone(),
            point,
        };
        if matches!(validated.kind, ActionKind::Type { .. }) && self.types_in_phases() {
            return Ok(self.deliver_typing(
                &validated,
                &target_box,
                foreground.as_deref(),
                request.expected_effect,
            ));
        }
        Ok(self.deliver(&validated, &before, request.expected_effect))
    }

    /// The snapshot and the accessibility evidence that confirms `target` are within
    /// `[engine] max_snapshot_age_ms` (ADR-0025). The capture instant bounds the snapshot;
    /// the instant the confirming accessibility read started bounds that evidence, which a
    /// newer publication never renews. A target no accessibility read confirms has no
    /// evidence age. Only reads the clock and the published state.
    fn check_age(&self, target: TargetId) -> Result<(), ToolError> {
        let Some(published) = self.store.published() else {
            return Err(stale_snapshot());
        };
        let now = self.ports.clock.now();
        let max = self.config.max_snapshot_age;
        if aged_out(now, published.captured_at, max) {
            return Err(snapshot_expired());
        }
        let read_at = published
            .elements
            .iter()
            .find(|e| e.target_id == target)
            .and_then(|e| e.evidence_at);
        if read_at.is_some_and(|read_at| aged_out(now, read_at, max)) {
            return Err(evidence_expired());
        }
        Ok(())
    }

    /// The checks that can change while the engine works, repeated immediately before
    /// delivery (SDD §2.9, chapter 08 §8.2): the operator's stop and consent, the age of the
    /// snapshot and of the evidence for `target` (ADR-0025), and the foreground application
    /// attested at the gate. A residual window between this look and the OS remains (§8.3);
    /// the live actuator narrows it with its own checks.
    pub(super) fn last_look(
        &self,
        foreground: Option<&str>,
        point: (f64, f64),
        target: TargetId,
    ) -> Result<(), ToolError> {
        let policy = self.ports.policy.current();
        if policy.stopped {
            return Err(operator_stopped());
        }
        if !policy.actuation_enabled {
            return Err(policy_denied());
        }
        self.check_age(target)?;
        let now = self.ports.actuator.foreground_application();
        if now.is_none() || now.as_deref() != foreground {
            return Err(not_allowlisted(
                "the foreground application changed after it was checked; nothing was delivered",
            ));
        }
        if !now
            .as_ref()
            .is_some_and(|app| policy.allowed_applications.contains(app))
        {
            return Err(not_allowlisted(
                "foreground application is not on the local allowlist",
            ));
        }
        self.check_window_scope(&policy, now.as_deref(), point)
    }

    /// The operator's scope on windows (ADR-0020): refuses unless the foreground window and
    /// the window under `point` are within the scope the policy sets for `application`.
    /// The actuator is asked only when a scope applies, so the default policy costs nothing.
    fn check_window_scope(
        &self,
        policy: &LocalPolicy,
        application: Option<&str>,
        point: (f64, f64),
    ) -> Result<(), ToolError> {
        let Some(application) = application else {
            return Ok(());
        };
        if !window_scope_applies(policy, application) {
            return Ok(());
        }
        let foreground = self.ports.actuator.foreground_window();
        let at_point = self.ports.actuator.window_at(point);
        match window_scope_problem(policy, application, foreground.as_ref(), at_point.as_ref()) {
            Some(problem) => Err(not_allowlisted(problem)),
            None => Ok(()),
        }
    }

    /// Hands a validated action to the actuator and reports what is known, no more.
    fn deliver(
        &mut self,
        action: &ValidatedAction,
        before: &BgraFrame,
        expected: ExpectedEffect,
    ) -> Outcome {
        // Whatever happens next, input may have reached the OS: the snapshot is spent.
        self.snapshot_spent = true;
        let since = self.ports.clock.now();
        let delivered = self.ports.actuator.deliver(action);
        self.spend(since, Stage::Delivery);
        let Ok(receipt) = delivered else {
            return Outcome::uncertain(
                false,
                "the actuator failed; input may or may not have reached the OS; do not retry",
            );
        };
        if receipt.submitted == 0 || receipt.inserted != receipt.submitted {
            return Outcome::uncertain(
                false,
                "the OS did not accept every input event; do not retry",
            );
        }
        self.report_effect(Some(before), expected)
    }

    /// What is known once the actuator reported every event delivered: an unverified
    /// effect keeps `input_delivered` true (chapter 07 §7.5). `before` is the frame the
    /// visual change is measured against, `None` when it could not be captured.
    fn report_effect(&mut self, before: Option<&BgraFrame>, expected: ExpectedEffect) -> Outcome {
        let since = self.ports.clock.now();
        let outcome = self.report_effect_untimed(before, expected);
        self.spend(since, Stage::Verification);
        outcome
    }

    fn report_effect_untimed(
        &mut self,
        before: Option<&BgraFrame>,
        expected: ExpectedEffect,
    ) -> Outcome {
        match expected {
            ExpectedEffect::None => Outcome::delivered(ActionStatus::InputDelivered, None),
            ExpectedEffect::VisualChange => match before {
                Some(before) if self.screen_changed_since(before) => Outcome::delivered(
                    ActionStatus::EffectObserved,
                    Some(ObservedEffect::VisualChange),
                ),
                Some(_) => Outcome::uncertain(
                    true,
                    "input was delivered but no visual change was observed within the \
                     verification window; do not retry",
                ),
                None => Outcome::uncertain(
                    true,
                    "input was delivered but the screen could not be captured to measure \
                     its effect; do not retry",
                ),
            },
            ExpectedEffect::FocusChanged
            | ExpectedEffect::ValueUpdated
            | ExpectedEffect::ScrollMoved => Outcome::uncertain(
                true,
                "input was delivered; this effect cannot be verified yet",
            ),
        }
    }

    /// Whether `type` is delivered in two phases (ADR-0018): the actuator can split it and
    /// accessibility evidence can say where keyboard focus is. Otherwise the actuator's own
    /// window-level check between its click and its keys is all there is.
    fn types_in_phases(&self) -> bool {
        self.ports.actuator.delivers_in_phases() && self.ports.accessibility.reads_keyboard_focus()
    }

    /// `type` in two phases (ADR-0018, SDD §2.9). The focusing click goes first. Keys follow
    /// only once the accessibility provider reports keyboard focus on the target element
    /// within the verification window, and only if the operator's stop and consent and the
    /// foreground application still hold. Whatever stops the keys leaves the action
    /// `indeterminate` with the delivered click reported: never retried, never rolled back.
    /// The visual change is measured against a frame captured after the click and before
    /// the keys, so the click's own redraw is not the effect of typing. The field's value
    /// is never read.
    fn deliver_typing(
        &mut self,
        action: &ValidatedAction,
        target: &BBox,
        foreground: Option<&str>,
        expected: ExpectedEffect,
    ) -> Outcome {
        // Whatever happens next, input may have reached the OS: the snapshot is spent.
        self.snapshot_spent = true;
        let since = self.ports.clock.now();
        let clicked = self.ports.actuator.deliver_focus_click(action);
        self.spend(since, Stage::Delivery);
        let Ok(click) = clicked else {
            return Outcome::uncertain(
                false,
                "the actuator failed; input may or may not have reached the OS; do not retry",
            );
        };
        if click.submitted == 0 || click.inserted != click.submitted {
            return Outcome::uncertain(
                false,
                "the OS did not accept every input event; do not retry",
            );
        }
        // From here the click is delivered: nothing below reports the action as not
        // delivered and nothing sends it again.
        let since = self.ports.clock.now();
        let focused = self.keyboard_focus_on(target);
        self.spend(since, Stage::Focus);
        if !focused {
            return Outcome::uncertain(
                true,
                "the click was delivered but keyboard focus on the target field was not \
                 confirmed in time; no key was sent; do not retry",
            );
        }
        let since = self.ports.clock.now();
        let reference = if expected == ExpectedEffect::VisualChange {
            self.ports.capturer.capture().ok().map(|c| c.frame)
        } else {
            None
        };
        let checked = self.last_look(foreground, target.center(), action.target_id);
        self.spend(since, Stage::Freshness);
        if let Err(error) = checked {
            // The window can end while the focus is awaited; that is named apart (ADR-0025).
            let message = if error.code == ErrorCode::StaleSnapshot {
                "the click was delivered, then the snapshot expired; no key was sent; observe \
                 again; do not retry"
            } else {
                "the click was delivered, then the operator's stop or consent or the \
                 foreground application changed; no key was sent; do not retry"
            };
            return Outcome::uncertain(true, message);
        }
        let since = self.ports.clock.now();
        let sent = self.ports.actuator.deliver_keys(action);
        self.spend(since, Stage::Delivery);
        let Ok(keys) = sent else {
            return Outcome::uncertain(
                true,
                "the click was delivered but the actuator failed on the keys; some may have \
                 reached the OS; do not retry",
            );
        };
        if keys.submitted == 0 || keys.inserted != keys.submitted {
            return Outcome::uncertain(
                true,
                "the click was delivered but the OS did not accept every key; do not retry",
            );
        }
        self.report_effect(reference.as_ref(), expected)
    }

    /// Whether the accessibility provider reports keyboard focus on `target` within the
    /// verification window. A failed read counts as no focus. Only reads: no input and no
    /// focus change.
    fn keyboard_focus_on(&mut self, target: &BBox) -> bool {
        let min_iou = self.config.fusion.accessibility_iou;
        self.within_verification_window(|engine| {
            matches!(
                engine.ports.accessibility.keyboard_focus(),
                Ok(Some(focused)) if focus_is_on(&focused, target, min_iou)
            )
        })
    }

    /// Whether the screen differs from `before` within the settle window
    /// ([`VerifyParams`](super::VerifyParams)). Changes that happened before `before` was
    /// captured are not this action's effect. Only captures: nothing is published and no
    /// input is sent again.
    fn screen_changed_since(&mut self, before: &BgraFrame) -> bool {
        self.within_verification_window(|engine| engine.capture_differs(before))
    }

    /// Whether `probe` holds within the verification window: a check right away, then one
    /// every `poll` on the injected clock until `timeout` has elapsed.
    fn within_verification_window(&mut self, mut probe: impl FnMut(&mut Self) -> bool) -> bool {
        let verify = self.config.verify;
        let poll = verify.poll.max(Duration::from_millis(1));
        // Bounds the probes even under a clock that does not advance.
        let max_waits = verify.timeout.as_nanos().div_ceil(poll.as_nanos());
        let start = self.ports.clock.now();
        let mut waits = 0u128;
        loop {
            if probe(self) {
                return true;
            }
            let elapsed = self.ports.clock.now().saturating_sub(start);
            let Some(left) = verify.timeout.checked_sub(elapsed) else {
                return false;
            };
            if left.is_zero() || waits >= max_waits {
                return false;
            }
            self.ports.clock.sleep(poll.min(left));
            waits += 1;
        }
    }

    /// Whether one fresh capture differs from `before`; a failed capture shows nothing.
    fn capture_differs(&mut self, before: &BgraFrame) -> bool {
        let Ok(captured) = self.ports.capturer.capture() else {
            return false;
        };
        dirty_tiles(Some(before), &captured.frame, self.config.plan.tile_size)
            .is_ok_and(|diff| diff.full_refresh.is_some() || !diff.tiles.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_canonical_snapshot_ids_parse() {
        assert_eq!(parse_snapshot_id("s1"), Some(1));
        assert_eq!(parse_snapshot_id("s42"), Some(42));
        for bad in [
            "",
            "s",
            "s0",
            "s01",
            "1",
            "S1",
            "s-1",
            "s1x",
            "s99999999999999999999",
        ] {
            assert_eq!(parse_snapshot_id(bad), None, "{bad}");
        }
    }
}
