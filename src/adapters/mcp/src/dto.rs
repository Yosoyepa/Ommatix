//! Wire DTOs of the Ommatix 1.0 tool contract and their mapping to the application
//! contract types (`ommatix_application::contract`).
//!
//! The DTOs mirror the JSON schemas in `docs/thesis/contratos/esquemas/`:
//!
//! - **Inputs** (`observe_screen`, `execute_action`, `get_engine_metrics`) are parsed
//!   strictly: unknown fields (coordinates included), `null` where the schema wants a value,
//!   wrong types, values outside the bounds and the per-action conditions are rejected with
//!   [`InvalidArguments`]. The runtime defaults the schemas only annotate are applied here:
//!   `limit = 100`, `include_values = false`, `dry_run = true`, `expected_effect = none`.
//! - **Outputs** serialise the contract objects field by field in schema order. They also
//!   deserialise with the same strictness, so a payload can be checked by parsing it back
//!   into the contract type (`TryFrom`); this is how the contract examples are verified.
//!
//! String lengths are counted in Unicode scalar values, as JSON Schema `maxLength` does.

use std::fmt;

use ommatix_application::contract::{
    ActionKind, ActionRequest, ActionResult, EngineMetrics, EngineMode, ErrorCode, ExpectedEffect,
    LabelSource, NextStep, ObservePage, ObserveRequest, ObservedEffect, ScrollDirection,
    StageTimings, TimingsSource, ToolError, Zone, limits,
};
use ommatix_application::contract::{ElementView, SCHEMA_VERSION};
use ommatix_domain::fusion::{Actionability, Role, TextKind};
use ommatix_domain::identity::{ActionStatus, TargetId};
use serde::de::{self, DeserializeOwned, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// The only `mode` of `observe_screen` in the MVP (chapter 07 §7.3).
pub const OBSERVE_MODE_FULL: &str = "full";

/// Tool arguments that do not satisfy the tool's input schema. The message names the
/// offending field or rule, never its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidArguments(pub String);

impl fmt::Display for InvalidArguments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InvalidArguments {}

fn invalid(message: impl Into<String>) -> InvalidArguments {
    InvalidArguments(message.into())
}

/// A payload that does not satisfy an output schema (or that the contract types cannot
/// represent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidPayload(pub String);

impl fmt::Display for InvalidPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InvalidPayload {}

fn bad(message: impl Into<String>) -> InvalidPayload {
    InvalidPayload(message.into())
}

// ---- strict serde helpers ----

/// For optional fields: absent → `None` (via `#[serde(default)]`); present → must be a `T`
/// (so `null` is rejected unless `T` itself accepts it).
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// For required, nullable fields: the key must be present; its value may be `null`.
/// (`deserialize_with` disables serde's "missing `Option` is `None`" shortcut.)
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// A JSON Schema `integer`: any JSON number with no fractional part (`3` and `3.0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SchemaInt(i128);

impl<'de> Deserialize<'de> for SchemaInt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IntVisitor;

        impl Visitor<'_> for IntVisitor {
            type Value = SchemaInt;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an integer")
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<SchemaInt, E> {
                Ok(SchemaInt(i128::from(v)))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<SchemaInt, E> {
                Ok(SchemaInt(i128::from(v)))
            }

            #[allow(clippy::float_cmp)] // exact integrality test on a finite float
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<SchemaInt, E> {
                // 2^63 bounds keep the cast exact; larger values are out of every bound here.
                if v.is_finite() && v.trunc() == v && v.abs() < 9.223_372_036_854_776e18 {
                    Ok(SchemaInt(v as i128))
                } else {
                    Err(E::invalid_type(de::Unexpected::Float(v), &self))
                }
            }
        }

        deserializer.deserialize_any(IntVisitor)
    }
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

fn check_len(field: &str, s: &str, min: usize, max: usize) -> Result<(), String> {
    let n = chars(s);
    if n < min {
        Err(format!("`{field}` must have at least {min} character(s)"))
    } else if n > max {
        Err(format!("`{field}` must have at most {max} characters"))
    } else {
        Ok(())
    }
}

fn int_in(field: &str, v: SchemaInt, min: i128, max: i128) -> Result<i128, String> {
    if v.0 < min {
        Err(format!("`{field}` must be at least {min}"))
    } else if v.0 > max {
        Err(format!("`{field}` must be at most {max}"))
    } else {
        Ok(v.0)
    }
}

/// Parses `value` (tool `arguments`) as `T`, turning serde's message into an
/// [`InvalidArguments`] without echoing values.
fn parse_object<T: DeserializeOwned>(value: &Value) -> Result<T, InvalidArguments> {
    if !value.is_object() {
        return Err(invalid("arguments must be a JSON object"));
    }
    T::deserialize(value).map_err(|e| invalid(sanitize_serde_message(&e.to_string())))
}

/// serde messages quote the offending value (`invalid type: string "…"`, ``unknown variant
/// `…` ``), and its escaping makes stripping the quotes unreliable. Every message is
/// therefore replaced by a fixed one per error category. Only a field name may appear, and
/// only one shaped like this server's field names (`[a-z_]{1,32}`): an unknown field's
/// name is agent text too. Nothing else the agent sent is ever echoed.
fn sanitize_serde_message(message: &str) -> String {
    const FIELD_ENDS: [&str; 2] = ["`, expected", "`, there are no fields"];
    const NAMED: [(&str, &str, &[&str]); 3] = [
        ("unknown field `", "unknown field", &FIELD_ENDS),
        ("missing field `", "missing field", &[]),
        ("duplicate field `", "duplicate field", &[]),
    ];
    /// The name serde wrapped in backticks after `prefix`: up to the last `terminator`
    /// (serde's own text after it holds none), or up to the closing backtick at the end.
    fn named<'a>(message: &'a str, prefix: &str, terminators: &[&str]) -> Option<&'a str> {
        let rest = message.strip_prefix(prefix)?;
        let end = terminators
            .iter()
            .filter_map(|t| rest.rfind(t))
            .max()
            .or_else(|| rest.strip_suffix('`').map(str::len))?;
        rest.get(..end)
    }
    fn tame(name: &str) -> bool {
        (1..=32).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
    }
    for (prefix, category, ends) in NAMED {
        if message.starts_with(prefix) {
            return match named(message, prefix, ends).filter(|n| tame(n)) {
                Some(name) => format!("{category} `{name}`"),
                None => category.to_owned(),
            };
        }
    }
    let fixed = if message.starts_with("unknown variant") {
        "unknown variant, expected one of the values the input schema lists"
    } else if message.starts_with("invalid type") {
        "invalid type for a field of the input schema"
    } else if message.starts_with("invalid value") {
        "invalid value for a field of the input schema"
    } else if message.starts_with("invalid length") {
        "invalid length for a field of the input schema"
    } else {
        "arguments do not match the input schema"
    };
    fixed.to_owned()
}

// ---- enums: one wire enum per contract enum ----

macro_rules! wire_enum {
    ($(#[$m:meta])* $dto:ident <=> $model:ident, $case:literal { $($v:ident),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = $case)]
        pub enum $dto { $($v),+ }

        impl From<$model> for $dto {
            fn from(v: $model) -> Self {
                match v { $($model::$v => Self::$v),+ }
            }
        }

        impl From<$dto> for $model {
            fn from(v: $dto) -> Self {
                match v { $($dto::$v => Self::$v),+ }
            }
        }
    };
}

wire_enum!(RoleDto <=> Role, "snake_case" {
    Button, Input, Checkbox, Dropdown, IconButton, TextNode, Unknown
});
wire_enum!(LabelSourceDto <=> LabelSource, "snake_case" {
    Ocr, CuratedIcon, Model, Accessibility, Unknown
});
wire_enum!(ActionabilityDto <=> Actionability, "snake_case" {
    Confirmed, Unknown, NotActionable
});
wire_enum!(TextKindDto <=> TextKind, "snake_case" { Value, Placeholder, Unknown });
wire_enum!(ZoneDto <=> Zone, "snake_case" {
    TopBar, LeftSidebar, CenterMain, BottomBar, Unknown
});
wire_enum!(StatusDto <=> ActionStatus, "snake_case" {
    Simulated, Rejected, InputDelivered, EffectObserved, Indeterminate
});
wire_enum!(ErrorCodeDto <=> ErrorCode, "SCREAMING_SNAKE_CASE" {
    StaleSnapshot, StaleTarget, TargetNotFound, AmbiguousTarget, NotActionable,
    PolicyDenied, PermissionDenied, CaptureUnavailable, InferenceFailed,
    UnsupportedGeometry, IndeterminateEffect, RequestConflict, Cancelled, LimitExceeded,
    SessionMismatch, EpochMismatch
});
wire_enum!(NextStepDto <=> NextStep, "snake_case" {
    ObserveAgain, OperatorReview, ReduceRequest, ReconcileEffect, Stop
});
wire_enum!(ExpectedEffectDto <=> ExpectedEffect, "snake_case" {
    None, FocusChanged, ValueUpdated, ScrollMoved, VisualChange
});
wire_enum!(ObservedEffectDto <=> ObservedEffect, "snake_case" {
    None, FocusChanged, ValueUpdated, ScrollMoved, VisualChange, Unknown
});
wire_enum!(ScrollDirectionDto <=> ScrollDirection, "snake_case" { Up, Down });
wire_enum!(EngineModeDto <=> EngineMode, "snake_case" { Simulation, Native });
wire_enum!(TimingsSourceDto <=> TimingsSource, "snake_case" { NotMeasured, RuntimeMonotonic });

/// `execute_action.action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionNameDto {
    Click,
    Type,
    Scroll,
}

// ---- inputs ----

/// `observe_screen` arguments as they arrive.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObserveScreenInput {
    #[serde(default, deserialize_with = "present")]
    query: Option<String>,
    #[serde(default, deserialize_with = "present")]
    limit: Option<SchemaInt>,
    #[serde(default, deserialize_with = "present")]
    cursor: Option<String>,
    #[serde(default, deserialize_with = "present")]
    include_values: Option<bool>,
}

/// Validates `observe_screen` arguments and applies the runtime defaults.
pub fn parse_observe_args(arguments: &Value) -> Result<ObserveRequest, InvalidArguments> {
    let input: ObserveScreenInput = parse_object(arguments)?;
    if let Some(query) = &input.query {
        check_len("query", query, 0, limits::MAX_QUERY_CHARS).map_err(invalid)?;
    }
    let limit = match input.limit {
        Some(v) => int_in(
            "limit",
            v,
            i128::from(limits::MIN_LIMIT),
            i128::from(limits::MAX_LIMIT),
        )
        .map_err(invalid)? as u16,
        None => limits::DEFAULT_LIMIT,
    };
    if let Some(cursor) = &input.cursor {
        check_len("cursor", cursor, 1, limits::MAX_CURSOR_CHARS).map_err(invalid)?;
    }
    Ok(ObserveRequest {
        query: input.query,
        limit,
        cursor: input.cursor,
        include_values: input.include_values.unwrap_or(false),
    })
}

/// `execute_action` arguments as they arrive.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecuteActionInput {
    request_id: String,
    session_id: String,
    snapshot_id: String,
    geometry_epoch: SchemaInt,
    target_id: String,
    expected_revision: SchemaInt,
    action: ActionNameDto,
    #[serde(default, deserialize_with = "present")]
    text: Option<String>,
    #[serde(default, deserialize_with = "present")]
    direction: Option<ScrollDirectionDto>,
    #[serde(default, deserialize_with = "present")]
    steps: Option<SchemaInt>,
    #[serde(default, deserialize_with = "present")]
    dry_run: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    expected_effect: Option<ExpectedEffectDto>,
}

const U64_MAX: i128 = u64::MAX as i128;

/// Validates `execute_action` arguments (bounds and the per-action conditions) and applies
/// the runtime defaults (`dry_run = true`, `expected_effect = none`).
pub fn parse_action_args(arguments: &Value) -> Result<ActionRequest, InvalidArguments> {
    let input: ExecuteActionInput = parse_object(arguments)?;
    for (field, value) in [
        ("request_id", &input.request_id),
        ("session_id", &input.session_id),
        ("snapshot_id", &input.snapshot_id),
        ("target_id", &input.target_id),
    ] {
        check_len(field, value, 1, limits::MAX_ID_CHARS).map_err(invalid)?;
    }
    let geometry_epoch =
        int_in("geometry_epoch", input.geometry_epoch, 1, U64_MAX).map_err(invalid)? as u64;
    let expected_revision =
        int_in("expected_revision", input.expected_revision, 1, U64_MAX).map_err(invalid)? as u64;

    let action = match input.action {
        ActionNameDto::Click => {
            if input.text.is_some() || input.direction.is_some() || input.steps.is_some() {
                return Err(invalid(
                    "`click` accepts no `text`, `direction` or `steps` (and never coordinates)",
                ));
            }
            ActionKind::Click
        }
        ActionNameDto::Type => {
            if input.direction.is_some() || input.steps.is_some() {
                return Err(invalid("`type` accepts no `direction` or `steps`"));
            }
            let text = input
                .text
                .ok_or_else(|| invalid("`type` requires `text`"))?;
            check_len("text", &text, 1, limits::MAX_TYPE_TEXT_CHARS).map_err(invalid)?;
            ActionKind::Type { text }
        }
        ActionNameDto::Scroll => {
            if input.text.is_some() {
                return Err(invalid("`scroll` accepts no `text`"));
            }
            let (Some(direction), Some(steps)) = (input.direction, input.steps) else {
                return Err(invalid("`scroll` requires `direction` and `steps`"));
            };
            let steps =
                int_in("steps", steps, 1, i128::from(limits::MAX_SCROLL_STEPS)).map_err(invalid)?;
            ActionKind::Scroll {
                direction: direction.into(),
                steps: steps as u8,
            }
        }
    };

    Ok(ActionRequest {
        request_id: input.request_id,
        session_id: input.session_id,
        snapshot_id: input.snapshot_id,
        geometry_epoch,
        target_id: input.target_id,
        expected_revision,
        action,
        dry_run: input.dry_run.unwrap_or(true),
        expected_effect: input
            .expected_effect
            .map_or(ExpectedEffect::None, Into::into),
    })
}

/// `get_engine_metrics` takes no arguments.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineMetricsInput {}

/// Validates `get_engine_metrics` arguments (an empty object).
pub fn parse_metrics_args(arguments: &Value) -> Result<(), InvalidArguments> {
    parse_object::<EngineMetricsInput>(arguments).map(|_| ())
}

// ---- outputs ----

fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((cut, _)) => s[..cut].to_owned(),
        None => s.to_owned(),
    }
}

fn check_schema_version(v: &str) -> Result<(), InvalidPayload> {
    if v == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(bad("`schema_version` must be \"1.0\""))
    }
}

/// The `error` object shared by `execute_action.output` and `tool_error.output`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorOutput {
    pub code: ErrorCodeDto,
    pub message: String,
    pub retryable: bool,
    pub next_step: NextStepDto,
}

impl From<&ToolError> for ErrorOutput {
    fn from(e: &ToolError) -> Self {
        let mut message = truncate_chars(&e.message, limits::MAX_MESSAGE_CHARS);
        if message.is_empty() {
            e.code.as_str().clone_into(&mut message);
        }
        Self {
            code: e.code.into(),
            message,
            retryable: e.retryable,
            next_step: e.next_step.into(),
        }
    }
}

impl TryFrom<ErrorOutput> for ToolError {
    type Error = InvalidPayload;

    fn try_from(e: ErrorOutput) -> Result<Self, InvalidPayload> {
        check_len("error.message", &e.message, 1, limits::MAX_MESSAGE_CHARS).map_err(bad)?;
        Ok(Self {
            code: e.code.into(),
            message: e.message,
            retryable: e.retryable,
            next_step: e.next_step.into(),
        })
    }
}

/// `tool_error.output`: an expected domain error of a tool without its own error shape
/// (`observe_screen`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolErrorOutput {
    pub schema_version: String,
    pub status: StatusDto,
    pub error: ErrorOutput,
}

impl From<&ToolError> for ToolErrorOutput {
    fn from(e: &ToolError) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_owned(),
            status: StatusDto::Rejected,
            error: e.into(),
        }
    }
}

impl TryFrom<ToolErrorOutput> for ToolError {
    type Error = InvalidPayload;

    fn try_from(o: ToolErrorOutput) -> Result<Self, InvalidPayload> {
        check_schema_version(&o.schema_version)?;
        if o.status != StatusDto::Rejected {
            return Err(bad("`status` of a tool error must be \"rejected\""));
        }
        o.error.try_into()
    }
}

/// One element of `observe_screen.output.elements`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementOutput {
    pub target_id: String,
    pub revision: u64,
    pub role: RoleDto,
    pub label: String,
    pub label_source: LabelSourceDto,
    pub actionability: ActionabilityDto,
    #[serde(deserialize_with = "nullable")]
    pub enabled: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub visible: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub text: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub text_kind: Option<TextKindDto>,
    pub zone: ZoneDto,
}

impl From<&ElementView> for ElementOutput {
    fn from(e: &ElementView) -> Self {
        let (text, text_kind) = match &e.text {
            Some((text, kind)) => (
                Some(truncate_chars(text, limits::MAX_TEXT_CHARS)),
                Some((*kind).into()),
            ),
            None => (None, None),
        };
        Self {
            target_id: e.target_id.to_string(),
            revision: e.revision,
            role: e.role.into(),
            label: truncate_chars(&e.label, limits::MAX_LABEL_CHARS),
            label_source: e.label_source.into(),
            actionability: e.actionability.into(),
            enabled: e.enabled,
            visible: e.visible,
            text,
            text_kind,
            zone: e.zone.into(),
        }
    }
}

impl TryFrom<ElementOutput> for ElementView {
    type Error = InvalidPayload;

    fn try_from(e: ElementOutput) -> Result<Self, InvalidPayload> {
        check_len("target_id", &e.target_id, 1, limits::MAX_ID_CHARS).map_err(bad)?;
        let target_id: TargetId = e
            .target_id
            .parse()
            .map_err(|()| bad("`target_id` is not an engine-issued id (`e<n>`)"))?;
        if e.revision < 1 {
            return Err(bad("`revision` must be at least 1"));
        }
        check_len("label", &e.label, 0, limits::MAX_LABEL_CHARS).map_err(bad)?;
        let text = match (e.text, e.text_kind) {
            (Some(text), Some(kind)) => {
                check_len("text", &text, 0, limits::MAX_TEXT_CHARS).map_err(bad)?;
                Some((text, kind.into()))
            }
            (None, None) => None,
            _ => return Err(bad("`text` and `text_kind` go together")),
        };
        Ok(Self {
            target_id,
            revision: e.revision,
            role: e.role.into(),
            label: e.label,
            label_source: e.label_source.into(),
            actionability: e.actionability.into(),
            enabled: e.enabled,
            visible: e.visible,
            text,
            zone: e.zone.into(),
        })
    }
}

/// `observe_screen.output`: one page of a full snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserveScreenOutput {
    pub schema_version: String,
    pub session_id: String,
    pub snapshot_id: String,
    pub geometry_epoch: u64,
    pub frame_seq: u64,
    pub mode: String,
    pub age_ms: u64,
    pub degraded: bool,
    pub elements: Vec<ElementOutput>,
    pub total_elements: u64,
    pub truncated: bool,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub next_cursor: Option<String>,
}

impl From<&ObservePage> for ObserveScreenOutput {
    fn from(p: &ObservePage) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_owned(),
            session_id: p.session_id.clone(),
            snapshot_id: p.snapshot_id.clone(),
            geometry_epoch: p.geometry_epoch,
            frame_seq: p.frame_seq,
            mode: OBSERVE_MODE_FULL.to_owned(),
            age_ms: p.age_ms,
            degraded: p.degraded,
            elements: p.elements.iter().map(ElementOutput::from).collect(),
            total_elements: p.total_elements as u64,
            truncated: p.truncated(),
            next_cursor: p.next_cursor.clone(),
        }
    }
}

impl TryFrom<ObserveScreenOutput> for ObservePage {
    type Error = InvalidPayload;

    /// Run-dependent timings are not part of the payload: they come back as zero.
    fn try_from(o: ObserveScreenOutput) -> Result<Self, InvalidPayload> {
        check_schema_version(&o.schema_version)?;
        check_len("session_id", &o.session_id, 1, limits::MAX_ID_CHARS).map_err(bad)?;
        check_len("snapshot_id", &o.snapshot_id, 1, limits::MAX_ID_CHARS).map_err(bad)?;
        if o.geometry_epoch < 1 {
            return Err(bad("`geometry_epoch` must be at least 1"));
        }
        if o.mode != OBSERVE_MODE_FULL {
            return Err(bad("`mode` must be \"full\""));
        }
        if o.elements.len() > usize::from(limits::MAX_LIMIT) {
            return Err(bad("`elements` has more than 200 items"));
        }
        if let Some(cursor) = &o.next_cursor {
            check_len("next_cursor", cursor, 1, limits::MAX_CURSOR_CHARS).map_err(bad)?;
        }
        if o.truncated != o.next_cursor.is_some() {
            return Err(bad(
                "`next_cursor` must be present exactly when `truncated` is true",
            ));
        }
        let elements = o
            .elements
            .into_iter()
            .map(ElementView::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            session_id: o.session_id,
            snapshot_id: o.snapshot_id,
            geometry_epoch: o.geometry_epoch,
            frame_seq: o.frame_seq,
            age_ms: o.age_ms,
            degraded: o.degraded,
            elements,
            total_elements: usize::try_from(o.total_elements)
                .map_err(|_| bad("`total_elements` is out of range"))?,
            next_cursor: o.next_cursor,
            timings: StageTimings::default(),
        })
    }
}

/// `execute_action.output`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteActionOutput {
    pub schema_version: String,
    pub request_id: String,
    pub status: StatusDto,
    pub target_id: String,
    pub snapshot_id: String,
    pub input_delivered: bool,
    pub verified: bool,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub error: Option<ErrorOutput>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub observed_effect: Option<ObservedEffectDto>,
}

impl From<&ActionResult> for ExecuteActionOutput {
    fn from(r: &ActionResult) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_owned(),
            request_id: r.request_id.clone(),
            status: r.status.into(),
            target_id: r.target_id.clone(),
            snapshot_id: r.snapshot_id.clone(),
            input_delivered: r.input_delivered(),
            verified: r.verified(),
            error: r.error.as_ref().map(ErrorOutput::from),
            observed_effect: r.observed_effect.map(Into::into),
        }
    }
}

impl TryFrom<ExecuteActionOutput> for ActionResult {
    type Error = InvalidPayload;

    /// Besides the schema rules, `verified` must be the one the status implies, and so must
    /// `input_delivered` except for `indeterminate`, which may report either: a delivered
    /// action with an unverified effect, or an uncertain delivery (chapter 07 §7.5).
    fn try_from(o: ExecuteActionOutput) -> Result<Self, InvalidPayload> {
        check_schema_version(&o.schema_version)?;
        for (field, value) in [
            ("request_id", &o.request_id),
            ("target_id", &o.target_id),
            ("snapshot_id", &o.snapshot_id),
        ] {
            check_len(field, value, 1, limits::MAX_ID_CHARS).map_err(bad)?;
        }
        let status: ActionStatus = o.status.into();
        let delivery_ok =
            status == ActionStatus::Indeterminate || o.input_delivered == status.input_delivered();
        if !delivery_ok || o.verified != status.verified() {
            return Err(bad(
                "`input_delivered`/`verified` contradict the status (a simulation never claims delivery)",
            ));
        }
        if status == ActionStatus::Rejected && o.error.is_none() {
            return Err(bad("a rejected action carries an `error`"));
        }
        if status == ActionStatus::EffectObserved && o.observed_effect.is_none() {
            return Err(bad("`effect_observed` carries an `observed_effect`"));
        }
        Ok(Self {
            request_id: o.request_id,
            status,
            target_id: o.target_id,
            snapshot_id: o.snapshot_id,
            error: o.error.map(ToolError::try_from).transpose()?,
            observed_effect: o.observed_effect.map(Into::into),
            delivered: o.input_delivered,
        })
    }
}

/// `get_engine_metrics.output`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineMetricsOutput {
    pub schema_version: String,
    pub mode: EngineModeDto,
    pub observations: u64,
    pub rejected_actions: u64,
    pub delivered_actions: u64,
    pub pending_regions: u64,
    pub models_loaded: bool,
    pub timings_source: TimingsSourceDto,
}

impl From<&EngineMetrics> for EngineMetricsOutput {
    fn from(m: &EngineMetrics) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_owned(),
            mode: m.mode.into(),
            observations: m.observations,
            rejected_actions: m.rejected_actions,
            delivered_actions: m.delivered_actions,
            pending_regions: m.pending_regions,
            models_loaded: m.models_loaded,
            timings_source: m.timings_source.into(),
        }
    }
}

impl TryFrom<EngineMetricsOutput> for EngineMetrics {
    type Error = InvalidPayload;

    fn try_from(o: EngineMetricsOutput) -> Result<Self, InvalidPayload> {
        check_schema_version(&o.schema_version)?;
        Ok(Self {
            mode: o.mode.into(),
            observations: o.observations,
            rejected_actions: o.rejected_actions,
            delivered_actions: o.delivered_actions,
            pending_regions: o.pending_regions,
            models_loaded: o.models_loaded,
            timings_source: o.timings_source.into(),
        })
    }
}

/// Parses an output payload strictly into its DTO.
pub fn parse_output<T: DeserializeOwned>(value: &Value) -> Result<T, InvalidPayload> {
    T::deserialize(value).map_err(|e| bad(e.to_string()))
}

/// Run-dependent stage timings, in whole microseconds, for result `_meta`.
pub fn timings_meta(t: &StageTimings) -> Value {
    let us = |d: std::time::Duration| u64::try_from(d.as_micros()).unwrap_or(u64::MAX);
    serde_json::json!({
        "capture_us": us(t.capture),
        "plan_us": us(t.plan),
        "detect_us": us(t.detect),
        "recognize_us": us(t.recognize),
        "fuse_us": us(t.fuse),
        "track_us": us(t.track),
        "total_us": us(t.total),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn click() -> Value {
        json!({
            "request_id": "r1", "session_id": "s", "snapshot_id": "s1",
            "geometry_epoch": 1, "target_id": "e1", "expected_revision": 1, "action": "click"
        })
    }

    fn indeterminate(delivered: bool) -> ActionResult {
        ActionResult {
            request_id: "r1".into(),
            status: ActionStatus::Indeterminate,
            target_id: "e1".into(),
            snapshot_id: "s1".into(),
            error: Some(ToolError::new(
                ErrorCode::IndeterminateEffect,
                "input was delivered but no visual change was observed",
            )),
            observed_effect: Some(ObservedEffect::Unknown),
            delivered,
        }
    }

    #[test]
    fn an_indeterminate_result_reports_whether_its_input_was_delivered() {
        // Found in the S3 supervised runs: a click the OS accepted, whose visual change the
        // engine did not see, went out as `input_delivered: false`.
        for delivered in [true, false] {
            let result = indeterminate(delivered);
            let output = ExecuteActionOutput::from(&result);
            assert_eq!(output.input_delivered, delivered);
            assert!(!output.verified, "an unverified effect is never claimed");
            assert_eq!(ActionResult::try_from(output).unwrap(), result);
        }
        // Every other status still fixes both flags.
        let mut output = ExecuteActionOutput::from(&indeterminate(true));
        output.status = StatusDto::Simulated;
        output.error = None;
        output.observed_effect = None;
        assert!(
            ActionResult::try_from(output).is_err(),
            "a simulation never claims delivery"
        );
    }

    #[test]
    fn defaults_are_applied_at_runtime() {
        let o = parse_observe_args(&json!({})).unwrap();
        assert_eq!(o, ObserveRequest::default());
        let a = parse_action_args(&click()).unwrap();
        assert!(a.dry_run);
        assert_eq!(a.expected_effect, ExpectedEffect::None);
    }

    #[test]
    fn null_is_not_an_absent_field() {
        assert!(parse_observe_args(&json!({ "query": null })).is_err());
        assert!(parse_observe_args(&json!({ "limit": null })).is_err());
        let mut a = click();
        a["dry_run"] = Value::Null;
        assert!(parse_action_args(&a).is_err());
    }

    #[test]
    fn integral_floats_are_schema_integers() {
        assert_eq!(
            parse_observe_args(&json!({ "limit": 5.0 })).unwrap().limit,
            5
        );
        assert!(parse_observe_args(&json!({ "limit": 5.5 })).is_err());
        assert!(parse_observe_args(&json!({ "limit": 1e300 })).is_err());
    }

    #[test]
    fn non_objects_and_unknown_fields_are_rejected() {
        assert!(parse_observe_args(&json!([])).is_err());
        assert!(parse_metrics_args(&json!({ "x": 1 })).is_err());
        let mut a = click();
        a["x"] = json!(10);
        assert!(
            parse_action_args(&a)
                .unwrap_err()
                .0
                .contains("unknown field")
        );
    }

    #[test]
    fn messages_do_not_echo_values() {
        let mut a = click();
        a["action"] = json!("rm -rf secret");
        let e = parse_action_args(&a).unwrap_err();
        assert!(!e.0.contains("secret"), "{e}");
        assert!(e.0.contains("expected one of"), "{e}");
        a["action"] = json!("click");
        a["geometry_epoch"] = json!("secret");
        let e = parse_action_args(&a).unwrap_err();
        assert!(!e.0.contains("secret"), "{e}");
    }

    #[test]
    fn messages_do_not_echo_values_with_escaped_quotes() {
        let hostile = [
            r#"x" secret "y"#,
            r#"a\" secret \"b"#,
            "` secret `",
            "`, expected secret `",
            "secret\u{1}\n\"",
        ];
        for value in hostile {
            let mut cases = Vec::new();
            for field in ["geometry_epoch", "action", "dry_run", "direction"] {
                let mut a = click();
                a[field] = json!(value);
                cases.push(a);
            }
            let mut a = click();
            a[value] = json!(1);
            cases.push(a);
            let mut a = click();
            a["action"] = json!("scroll");
            a["steps"] = json!(1);
            a["direction"] = json!([value]);
            cases.push(a);
            cases.push(json!({ "query": [value] }));
            let mut unknown = json!({});
            unknown[value] = json!(value);
            cases.push(unknown);
            for case in cases {
                let e = if case.get("request_id").is_some() {
                    parse_action_args(&case).unwrap_err()
                } else {
                    parse_observe_args(&case).unwrap_err()
                };
                assert!(!e.0.contains("secret"), "{value:?} -> {e}");
                assert!(!e.0.contains('"'), "{value:?} -> {e}");
            }
        }
        // Field names this server defines may be named; agent-chosen ones only when tame.
        let mut a = click();
        a["coordinates_x"] = json!(1);
        assert_eq!(
            parse_action_args(&a).unwrap_err().0,
            "unknown field `coordinates_x`"
        );
        let mut a = click();
        a["Secret Field"] = json!(1);
        assert_eq!(parse_action_args(&a).unwrap_err().0, "unknown field");
        let mut a = click();
        a.as_object_mut().unwrap().remove("target_id");
        assert_eq!(
            parse_action_args(&a).unwrap_err().0,
            "missing field `target_id`"
        );
    }
}
