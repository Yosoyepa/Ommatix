//! The action gate (SDD §2.9, ADR-0007) and the honest delivery report (chapter 07 §7.5).
//!
//! Order: arguments → `request_id` deduplication → a snapshot that reflects the last
//! observation and has not been spent by a delivery → session, epoch, snapshot, target,
//! revision and actionability (the registry's checks) → the action's own needs → dry run
//! (`simulated`, the default) or, only when the operator's policy allows it, a live
//! actuator exists and attests an allowlisted foreground application, a freshness check
//! (the screen is still the committed base), delivery and a report of exactly what is
//! known: `input_delivered` never claims an effect, and any doubt is `indeterminate`,
//! never an automatic retry. Any delivery spends the snapshot: the next action needs a
//! new observation.

use ommatix_domain::fusion::Role;
use ommatix_domain::identity::{ActionRef, ActionRejection, ActionStatus, Clearance, Delivery};
use ommatix_domain::incremental::{BgraFrame, dirty_tiles};

use super::Engine;
use super::journal::{Lookup, arguments_hash};
use crate::contract::{
    ActionKind, ActionRequest, ActionResult, ErrorCode, ExpectedEffect, ObservedEffect, ToolError,
    ValidatedAction, limits,
};

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

fn policy_denied() -> ToolError {
    ToolError::new(
        ErrorCode::PolicyDenied,
        "live input is not allowed by the local policy or no live actuator is available",
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

impl Engine {
    pub(super) fn act(&mut self, request: &ActionRequest) -> ActionResult {
        let reply = |status, error, observed_effect| ActionResult {
            request_id: request.request_id.clone(),
            status,
            target_id: request.target_id.clone(),
            snapshot_id: request.snapshot_id.clone(),
            error,
            observed_effect,
        };
        if let Err(error) = check_arguments(request) {
            self.counters.rejected_actions += 1;
            return reply(ActionStatus::Rejected, Some(error), None);
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
                return reply(ActionStatus::Rejected, Some(error), None);
            }
            Lookup::New => {}
        }

        let result = match self.gate_and_perform(request) {
            Ok((status, error, effect)) => reply(status, error, effect),
            Err(error) => reply(ActionStatus::Rejected, Some(error), None),
        };
        match result.status {
            ActionStatus::Rejected => self.counters.rejected_actions += 1,
            ActionStatus::InputDelivered | ActionStatus::EffectObserved => {
                self.counters.delivered_actions += 1;
            }
            ActionStatus::Simulated | ActionStatus::Indeterminate => {}
        }
        self.journal
            .record(&request.request_id, hash, result.clone());
        result
    }

    /// Runs the checks and, if they pass, simulates or delivers the action.
    #[allow(clippy::type_complexity)] // a private triple, named by its only caller
    fn gate_and_perform(
        &mut self,
        request: &ActionRequest,
    ) -> Result<(ActionStatus, Option<ToolError>, Option<ObservedEffect>), ToolError> {
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
        let role = registry
            .get(target_id)
            .map(|t| t.observation.role)
            .ok_or_else(|| rejection_error(ActionRejection::TargetNotFound))?;
        if matches!(request.action, ActionKind::Type { .. }) && role != Role::Input {
            return Err(ToolError::new(
                ErrorCode::NotActionable,
                "type needs a confirmed input field",
            ));
        }

        if request.dry_run {
            return Ok((ActionStatus::Simulated, None, None));
        }

        let policy = self.ports.policy.current();
        let allowed = policy.actuation_enabled && !policy.stopped && self.ports.actuator.is_live();
        if !allowed {
            return Err(policy_denied());
        }
        // Fail closed on the application allowlist (chapter 08): an empty list allows
        // nothing, and a foreground application the actuator cannot attest is not on it.
        if policy.allowed_applications.is_empty() {
            return Err(not_allowlisted("no application is on the local allowlist"));
        }
        let foreground = self.ports.actuator.foreground_application();
        if !foreground.is_some_and(|app| policy.allowed_applications.contains(&app)) {
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
        let before = self.fresh_frame()?;
        let validated = ValidatedAction {
            target_id,
            kind: request.action.clone(),
            point: bbox.center(),
        };
        Ok(self.deliver(&validated, &before, request.expected_effect))
    }

    /// Freshness (SDD §2.9): a frame captured right before delivery must still be the
    /// committed base, or the snapshot no longer shows the screen and nothing is
    /// delivered. Returns that frame, the reference for the action's visual effect.
    fn fresh_frame(&mut self) -> Result<BgraFrame, ToolError> {
        let Ok(captured) = self.ports.capturer.capture() else {
            return Err(ToolError::new(
                ErrorCode::CaptureUnavailable,
                "the screen could not be captured to confirm the snapshot; nothing was delivered",
            ));
        };
        let unchanged = dirty_tiles(
            self.store.base(),
            &captured.frame,
            self.config.plan.tile_size,
        )
        .is_ok_and(|diff| diff.full_refresh.is_none() && diff.tiles.is_empty());
        if unchanged {
            Ok(captured.frame)
        } else {
            self.snapshot_spent = true;
            Err(ToolError::new(
                ErrorCode::StaleSnapshot,
                "the screen changed since the observation; nothing was delivered; observe again",
            ))
        }
    }

    /// Hands a validated action to the actuator and reports what is known, no more.
    fn deliver(
        &mut self,
        action: &ValidatedAction,
        before: &BgraFrame,
        expected: ExpectedEffect,
    ) -> (ActionStatus, Option<ToolError>, Option<ObservedEffect>) {
        // Whatever happens next, input may have reached the OS: the snapshot is spent.
        self.snapshot_spent = true;
        let uncertain = |message| {
            (
                ActionStatus::Indeterminate,
                Some(indeterminate(message)),
                Some(ObservedEffect::Unknown),
            )
        };
        let Ok(receipt) = self.ports.actuator.deliver(action) else {
            return uncertain(
                "the actuator failed; input may or may not have reached the OS; do not retry",
            );
        };
        if receipt.submitted == 0 || receipt.inserted != receipt.submitted {
            return uncertain("the OS did not accept every input event; do not retry");
        }
        match expected {
            ExpectedEffect::None => (ActionStatus::InputDelivered, None, None),
            ExpectedEffect::VisualChange => {
                if self.screen_changed_since(before) {
                    (
                        ActionStatus::EffectObserved,
                        None,
                        Some(ObservedEffect::VisualChange),
                    )
                } else {
                    uncertain("input was delivered but no visual change was observed")
                }
            }
            ExpectedEffect::FocusChanged
            | ExpectedEffect::ValueUpdated
            | ExpectedEffect::ScrollMoved => {
                uncertain("input was delivered; this effect cannot be verified yet")
            }
        }
    }

    /// Whether a fresh capture differs from `before`, the frame captured right before
    /// delivery; changes that happened earlier are not this action's effect. Nothing is
    /// published.
    fn screen_changed_since(&mut self, before: &BgraFrame) -> bool {
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
