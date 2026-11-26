//! `ommatix demo`: a few observations and one dry-run click, printed for a human.
//!
//! This is the one module allowed to print to stdout. Its output depends only on the
//! source and the session id (no timings), so `--session-id` makes it reproducible. With
//! `--timings`, each observation also writes one record to stderr (`ommatix-observation`
//! followed by a JSON object, schema `ommatix.observation/1`, dossier chapter 09 §9.9): the
//! stage timings in whole microseconds, the plan and a signature per element. They depend
//! on the run, so they never reach stdout, and no label is written, only its signature.
//!
//! With `--plan-trace`, each observation also writes one line to stderr (`ommatix-plan-trace`
//! followed by a JSON object, schema `ommatix.plan-trace/2`): the numbers behind the plan's
//! choice of work, counts and areas only. An observation that failed gets its line too, with
//! `outcome` `error`, its code and nothing else measured, as `--timings` does. It is the
//! planner's own record of why an observation was a full refresh; it is off unless asked for
//! and no MCP client can reach it. Every field is defined in `docs/guide/configuration.md`.
//! Schema 2 (ADR-0045) adds which closure made the plan (`chosen`), what the support-aware
//! closure did (`spanning`) and the class of each component's and region's largest support
//! (`component_support_classes`, `region_support_classes`): counts, areas and class names.
//!
//! With `--plan-check`, each regional observation is also compared with a full read of the
//! same frame, run after it is committed (its time is not the observation's), and one line
//! goes to stderr (`ommatix-plan-check`, schema `ommatix.plan-check/1`): how many boxes of
//! each kind one read has and the other lacks, split by whether they overlap a box ADR-0045
//! lets a region meet without containing. Counts only.

// `demo` is the one command whose output is meant for a human on stdout.
#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::time::Duration;

use ommatix_adapter_sim::{SimulatedDesktop, layout};
use ommatix_application::contract::{
    ActionKind, ActionRequest, ElementView, ExpectedEffect, ObservePage, ObserveRequest, ToolError,
};
use ommatix_application::{CheckCount, CheckKinds, Engine, PlanCheck};
use ommatix_domain::fusion::Actionability;
use ommatix_domain::incremental::{FullRefreshReason, PlanTrace, SpanningTrace, SupportClass};
use ommatix_domain::{Action, MouseButton};
use serde_json::{Value, json};

/// The schema of the per-observation record written by `--timings`.
const OBSERVATION_SCHEMA: &str = "ommatix.observation/1";

/// The schema of the per-observation line written by `--plan-trace`.
const PLAN_TRACE_SCHEMA: &str = "ommatix.plan-trace/2";

/// The schema of the per-observation line written by `--plan-check`.
const PLAN_CHECK_SCHEMA: &str = "ommatix.plan-check/1";

/// `TextNode` → `text_node`: the contract's spelling of a variant name.
fn snake(debug: impl std::fmt::Debug) -> String {
    let mut out = String::new();
    for (i, c) in format!("{debug:?}").chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Plays the simulated user before observation `n`; returns what it did.
fn play_user(desktop: &SimulatedDesktop, n: u8) -> Option<&'static str> {
    let click = |rect: ommatix_domain::Rect| Action::Click {
        at: rect.center(),
        button: MouseButton::Left,
    };
    match n {
        2 => {
            desktop.perform(&click(layout::TEXT_FIELD));
            desktop.perform(&Action::TypeText {
                text: "hello".to_owned(),
            });
            Some("clicks the text field and types 5 characters")
        }
        3 => {
            desktop.perform(&click(layout::SAVE_BUTTON));
            Some("clicks Save; a confirmation dialog opens")
        }
        _ => None,
    }
}

fn print_page(n: u8, page: &ObservePage) {
    println!(
        "observation {n}: snapshot {} | frame {} | {} elements{}",
        page.snapshot_id,
        page.frame_seq,
        page.total_elements,
        if page.truncated() {
            " (first page shown)"
        } else {
            ""
        },
    );
    for e in &page.elements {
        println!(
            "  {:<4} rev {:<3} {:<10} {:<14} {:<12} {}{}",
            e.target_id.to_string(),
            e.revision,
            snake(e.role),
            format!("{:?}", e.label),
            snake(e.label_source),
            snake(e.actionability),
            if e.enabled == Some(false) {
                " (disabled)"
            } else {
                ""
            },
        );
    }
}

/// `role: unknown 3, text_node 5 | actionability: … | label_source: …` over `elements`,
/// each tally in name order.
fn tally(elements: &[ElementView]) -> String {
    let count = |key: fn(&ElementView) -> String| {
        let mut counts = BTreeMap::<String, usize>::new();
        for e in elements {
            *counts.entry(key(e)).or_default() += 1;
        }
        counts
            .iter()
            .map(|(name, n)| format!("{name} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "role: {} | actionability: {} | label_source: {}",
        count(|e| snake(e.role)),
        count(|e| snake(e.actionability)),
        count(|e| snake(e.label_source)),
    )
}

/// Every element of the snapshot `page` starts, following its cursor.
fn all_elements(engine: &mut Engine, page: &ObservePage) -> Vec<ElementView> {
    let mut elements = page.elements.clone();
    let mut cursor = page.next_cursor.clone();
    while let Some(next) = cursor {
        let request = ObserveRequest {
            cursor: Some(next),
            ..ObserveRequest::default()
        };
        let Ok(more) = engine.observe_screen(&request) else {
            break;
        };
        elements.extend(more.elements);
        cursor = more.next_cursor;
    }
    elements
}

/// FNV-1a (64 bits) of `text`, in hex. It names an element by what it shows (role, label,
/// actionability) without keeping the label: two runs of the same screen give the same names.
fn signature(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// One observation of `--timings`: whole microseconds of the monotonic clock, the plan that
/// decided the work, and the sorted signatures of every element of the snapshot. `null` is a
/// value that was not measured.
fn observation_record(n: u8, page: &ObservePage, elements: &[ElementView]) -> Value {
    let us = |d: Duration| u64::try_from(d.as_micros()).unwrap_or(u64::MAX);
    let t = page.timings;
    let p = &page.plan;
    let mut signatures: Vec<String> = elements
        .iter()
        .map(|e| {
            signature(&format!(
                "{}|{}|{}",
                snake(e.role),
                e.label,
                snake(e.actionability)
            ))
        })
        .collect();
    signatures.sort();
    json!({
        "schema": OBSERVATION_SCHEMA,
        "observation_id": n,
        "outcome": "ok",
        "error_code": null,
        "snapshot_id": page.snapshot_id,
        "geometry_epoch": page.geometry_epoch,
        "timings_us": {
            "capture": us(t.capture),
            "plan": us(t.plan),
            "detect": us(t.detect),
            "recognize": us(t.recognize),
            "accessibility": us(t.accessibility),
            "fuse": us(t.fuse),
            "track": us(t.track),
            "commit": us(t.commit),
            "response_build": t.response_build.map(us),
            "total": us(t.total),
        },
        "plan": {
            "work": p.work.as_str(),
            "full_refresh_reason": p.full_refresh_reason.map(FullRefreshReason::as_str),
            "region_count": p.region_count,
            "region_area_px": p.region_area_px,
            "inference_count": p.inference_count,
            "input_shapes": p.input_shapes,
            "accessibility_truncated": p.accessibility_truncated,
            "controls_n": p.controls_n,
            "words_n": p.words_n,
            "supports_n": p.supports_n,
            "relations_n": p.relations_n,
        },
        "signatures": signatures,
    })
}

/// One observation of `--plan-trace`: the plan's work and reason (as in the observation
/// record) beside the planner's numbers. Counts and areas in pixels of the frame; nothing
/// that names a place on the screen. `null` is a check that did not decide.
fn plan_trace_record(n: u8, page: &ObservePage, trace: &PlanTrace) -> Value {
    let p = &page.plan;
    json!({
        "schema": PLAN_TRACE_SCHEMA,
        "observation_id": n,
        "outcome": "ok",
        "error_code": null,
        "work": p.work.as_str(),
        "full_refresh_reason": p.full_refresh_reason.map(FullRefreshReason::as_str),
        "frame_area_px": trace.frame_area_px,
        "tile_size": trace.tile_size,
        "changed_tiles": trace.changed_tiles,
        "pending_tiles": trace.pending_tiles,
        "dirty_tiles": trace.dirty_tiles,
        "components": trace.components.iter().map(|c| json!({
            "tiles": c.tiles,
            "tight_area_px": c.tight_area_px,
            "padded_area_px": c.padded_area_px,
            "closed_area_px": c.closed_area_px,
            "largest_support_px": c.largest_support_px,
        })).collect::<Vec<_>>(),
        "regions": trace.regions.iter().map(|r| json!({
            "area_px": r.area_px,
            "components": r.components,
            "pre_closure_area_px": r.pre_closure_area_px,
            "largest_support_px": r.largest_support_px,
        })).collect::<Vec<_>>(),
        "covered_area_px": trace.covered_area_px,
        "budget_area_px": trace.budget_area_px,
        "max_regions": trace.max_regions,
        "decided_by": trace.decided_by.map(FullRefreshReason::as_str),
        "decisive_region": trace.decisive_region,
        "chosen": trace.chosen.as_str(),
        "spanning": trace.spanning.as_ref().map(spanning_record),
        "component_support_classes": trace
            .components
            .iter()
            .map(|c| c.largest_support_class.map(SupportClass::as_str))
            .collect::<Vec<_>>(),
        "region_support_classes": trace
            .regions
            .iter()
            .map(|r| r.largest_support_class.map(SupportClass::as_str))
            .collect::<Vec<_>>(),
    })
}

/// What the support-aware closure did (ADR-0045): counts, areas and reasons.
fn spanning_record(s: &SpanningTrace) -> Value {
    json!({
        "surfaces_declared": s.surfaces_declared,
        "surfaces_spanned": s.surfaces_spanned,
        "largest_spanned_px": s.largest_spanned_px,
        "whole_on_read_cut": s.whole_on_read_cut,
        "largest_whole_on_read_cut_px": s.largest_whole_on_read_cut_px,
        "relations_ignored": s.relations_ignored,
        "attempt_reason": s.attempt_reason.map(FullRefreshReason::as_str),
        "classic": s.classic.map(|c| json!({
            "work": if c.full_refresh_reason.is_some() { "full" } else { "regions" },
            "full_refresh_reason": c.full_refresh_reason.map(FullRefreshReason::as_str),
            "region_count": c.region_count,
            "covered_area_px": c.covered_area_px,
        })),
    })
}

fn check_kinds(k: &CheckKinds) -> Value {
    let count = |c: &CheckCount| {
        json!({
            "at_spanned_or_whole_on_read": c.at_spanned_or_whole_on_read,
            "elsewhere": c.elsewhere,
        })
    };
    json!({
        "controls": count(&k.controls),
        "words": count(&k.words),
        "accessibility": count(&k.accessibility),
    })
}

/// One observation of `--plan-check`. `checked` is `false` (and the counts `null`) when no
/// comparison ran: the observation was not regional, or the full read failed.
fn plan_check_record(n: u8, page: &ObservePage, check: Option<&PlanCheck>) -> Value {
    json!({
        "schema": PLAN_CHECK_SCHEMA,
        "observation_id": n,
        "outcome": "ok",
        "error_code": null,
        "work": page.plan.work.as_str(),
        "checked": check.is_some(),
        "full_truncated": check.map(|c| c.full_truncated),
        "only_regional": check.map(|c| check_kinds(&c.only_regional)),
        "only_full": check.map(|c| check_kinds(&c.only_full)),
    })
}

/// The `--plan-check` line of an observation that failed.
fn plan_check_error_record(n: u8, error: &ToolError) -> Value {
    json!({
        "schema": PLAN_CHECK_SCHEMA,
        "observation_id": n,
        "outcome": "error",
        "error_code": error.code.as_str(),
        "work": null,
        "checked": false,
        "full_truncated": null,
        "only_regional": null,
        "only_full": null,
    })
}

/// The `--plan-trace` line of an observation that failed: the same keys as the line of one
/// that succeeded, its error code, and `null` (or an empty list) for everything the planner
/// did not get to report: a failed observation publishes nothing, so no trace is kept.
fn plan_trace_error_record(n: u8, error: &ToolError) -> Value {
    json!({
        "schema": PLAN_TRACE_SCHEMA,
        "observation_id": n,
        "outcome": "error",
        "error_code": error.code.as_str(),
        "work": null,
        "full_refresh_reason": null,
        "frame_area_px": null,
        "tile_size": null,
        "changed_tiles": null,
        "pending_tiles": null,
        "dirty_tiles": null,
        "components": [],
        "regions": [],
        "covered_area_px": null,
        "budget_area_px": null,
        "max_regions": null,
        "decided_by": null,
        "decisive_region": null,
        "chosen": null,
        "spanning": null,
        "component_support_classes": null,
        "region_support_classes": null,
    })
}

/// The record of an observation that failed: its code, and nothing it did not measure.
fn error_record(n: u8, error: &ToolError) -> Value {
    json!({
        "schema": OBSERVATION_SCHEMA,
        "observation_id": n,
        "outcome": "error",
        "error_code": error.code.as_str(),
        "snapshot_id": null,
        "geometry_epoch": null,
        "timings_us": null,
        "plan": null,
        "signatures": [],
    })
}

fn clickable(page: &ObservePage) -> Option<&ElementView> {
    page.elements
        .iter()
        .find(|e| e.actionability == Actionability::Confirmed && e.enabled == Some(true))
}

/// Runs `observations` observations, then a dry-run click on the first confirmed,
/// enabled element of the last page. With `timings`, each observation's per-stage timings
/// go to stderr; with `plan_trace`, so does the planner's trace of it; with `plan_check`, so
/// does the comparison of each regional observation with a full read of its frame.
pub fn run(
    engine: &mut Engine,
    sim: Option<&SimulatedDesktop>,
    summary: &str,
    observations: u8,
    timings: bool,
    plan_trace: bool,
    plan_check: bool,
) {
    println!("ommatix demo | session {} | {summary}", engine.session_id());
    engine.set_plan_trace(plan_trace);
    engine.set_plan_check(plan_check);
    let mut last = None;
    for n in 1..=observations {
        if let Some(what) = sim.and_then(|desktop| play_user(desktop, n)) {
            println!("user: {what}");
        }
        match engine.observe_screen(&ObserveRequest::default()) {
            Ok(page) => {
                print_page(n, &page);
                let elements = all_elements(engine, &page);
                println!("  totals: {}", tally(&elements));
                if timings {
                    eprintln!(
                        "ommatix-observation {}",
                        observation_record(n, &page, &elements)
                    );
                }
                if let Some(trace) = engine.last_plan_trace() {
                    eprintln!("ommatix-plan-trace {}", plan_trace_record(n, &page, trace));
                }
                if plan_check {
                    eprintln!(
                        "ommatix-plan-check {}",
                        plan_check_record(n, &page, engine.last_plan_check())
                    );
                }
                last = Some(page);
            }
            Err(e) => {
                println!("observation {n}: {} ({})", e.code.as_str(), e.message);
                if timings {
                    eprintln!("ommatix-observation {}", error_record(n, &e));
                }
                if plan_trace {
                    eprintln!("ommatix-plan-trace {}", plan_trace_error_record(n, &e));
                }
                if plan_check {
                    eprintln!("ommatix-plan-check {}", plan_check_error_record(n, &e));
                }
                break;
            }
        }
    }
    let Some(page) = last else {
        println!("no snapshot was published; nothing to click");
        return;
    };
    let Some(target) = clickable(&page) else {
        println!(
            "no confirmed, enabled element in {}: this detector reports no affordances, so \
             nothing is offered as clickable",
            page.snapshot_id
        );
        return;
    };
    let result = engine.execute_action(&ActionRequest {
        request_id: "demo-click-1".to_owned(),
        session_id: page.session_id.clone(),
        snapshot_id: page.snapshot_id.clone(),
        geometry_epoch: page.geometry_epoch,
        target_id: target.target_id.to_string(),
        expected_revision: target.revision,
        action: ActionKind::Click,
        dry_run: true,
        expected_effect: ExpectedEffect::None,
    });
    println!(
        "dry-run click on {} (rev {}, {:?}) in {}: status {}, input_delivered {}, verified {}{}",
        target.target_id,
        target.revision,
        target.label,
        page.snapshot_id,
        snake(result.status),
        result.input_delivered(),
        result.verified(),
        result
            .error
            .map(|e| format!(", error {}", e.code.as_str()))
            .unwrap_or_default(),
    );
}

#[cfg(test)]
mod tests {
    use ommatix_application::contract::ErrorCode;
    use ommatix_domain::fusion::Role;
    use ommatix_domain::identity::ActionStatus;

    use super::*;

    #[test]
    fn variant_names_are_spelled_like_the_contract() {
        assert_eq!(snake(Role::TextNode), "text_node");
        assert_eq!(snake(Actionability::NotActionable), "not_actionable");
        assert_eq!(snake(ActionStatus::InputDelivered), "input_delivered");
        assert_eq!(snake(Role::Button), "button");
    }

    #[test]
    fn signatures_are_fnv1a_of_the_text() {
        // The FNV-1a 64-bit reference values.
        assert_eq!(signature(""), "cbf29ce484222325");
        assert_eq!(signature("a"), "af63dc4c8601ec8c");
    }

    #[test]
    fn an_observation_record_has_the_research_fields_in_whole_microseconds() {
        let args = crate::cli::RunArgs {
            config: None,
            source: crate::cli::SourceKind::Sim,
            images: None,
            output: 0,
            input: None,
            theme: crate::cli::ThemeKind::Light,
            session_id: Some("record".to_owned()),
            legacy: false,
        };
        let mut wired = crate::wiring::build(&args, &crate::config::Config::default()).unwrap();
        let page = wired
            .engine
            .observe_screen(&ObserveRequest {
                limit: 1,
                ..ObserveRequest::default()
            })
            .unwrap();
        let elements = all_elements(&mut wired.engine, &page);
        let record = observation_record(2, &page, &elements);
        assert_eq!(record["schema"], OBSERVATION_SCHEMA);
        assert_eq!(record["observation_id"], 2);
        assert_eq!(record["outcome"], "ok");
        // The first observation has no committed base: a full refresh for that reason.
        assert_eq!(record["plan"]["work"], "full");
        assert_eq!(record["plan"]["full_refresh_reason"], "no_committed_base");
        // Stage timings are whole numbers; the response is built after the publication.
        assert!(record["timings_us"]["total"].is_u64(), "{record}");
        assert!(record["timings_us"]["commit"].is_u64(), "{record}");
        assert!(record["timings_us"]["response_build"].is_u64(), "{record}");
        // One signature per element of the whole snapshot, sorted.
        let signatures: Vec<&str> = record["signatures"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(signatures.len(), page.total_elements);
        assert!(signatures.windows(2).all(|w| w[0] <= w[1]));
    }

    /// The keys of one `--plan-trace` line (JSON writes them sorted). Every one is a
    /// count, an area, a closed-list name or an index: none names a place on the screen.
    const TRACE_KEYS: [&str; 22] = [
        "schema",
        "observation_id",
        "outcome",
        "error_code",
        "work",
        "full_refresh_reason",
        "frame_area_px",
        "tile_size",
        "changed_tiles",
        "pending_tiles",
        "dirty_tiles",
        "components",
        "regions",
        "covered_area_px",
        "budget_area_px",
        "max_regions",
        "decided_by",
        "decisive_region",
        "chosen",
        "spanning",
        "component_support_classes",
        "region_support_classes",
    ];

    fn sorted_keys(value: &Value) -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    }

    fn sim_wired(id: &str) -> crate::wiring::Wired {
        let args = crate::cli::RunArgs {
            config: None,
            source: crate::cli::SourceKind::Sim,
            images: None,
            output: 0,
            input: None,
            theme: crate::cli::ThemeKind::Light,
            session_id: Some(id.to_owned()),
            legacy: false,
        };
        crate::wiring::build(&args, &crate::config::Config::default()).unwrap()
    }

    #[test]
    fn a_plan_trace_line_has_the_documented_fields_and_nothing_that_names_a_place() {
        let mut wired = sim_wired("trace");
        wired.engine.set_plan_trace(true);
        let sim = wired.sim.as_ref().unwrap();
        let mut seen_regions = false;
        for n in 1..=3 {
            let _ = play_user(sim, n);
            let page = wired
                .engine
                .observe_screen(&ObserveRequest::default())
                .unwrap();
            let trace = wired
                .engine
                .last_plan_trace()
                .expect("a trace was asked for");
            let record = plan_trace_record(n, &page, trace);
            // The line is one JSON object that parses back, with exactly the documented keys.
            let line = record.to_string();
            assert!(!line.contains('\n'));
            let parsed: Value = serde_json::from_str(&line).unwrap();
            let mut expected: Vec<String> = TRACE_KEYS.iter().map(|k| (*k).to_owned()).collect();
            expected.sort();
            assert_eq!(sorted_keys(&parsed), expected, "{line}");
            assert_eq!(parsed["schema"], PLAN_TRACE_SCHEMA);
            assert_eq!(parsed["observation_id"], n);
            assert_eq!(parsed["outcome"], "ok");
            assert!(parsed["error_code"].is_null());
            // The work and the reason are the observation record's own.
            assert_eq!(parsed["work"], page.plan.work.as_str());
            assert_eq!(
                parsed["full_refresh_reason"],
                json!(page.plan.full_refresh_reason.map(FullRefreshReason::as_str))
            );
            // Counts and areas are numbers; the deciding check is a closed-list name or null.
            for key in [
                "frame_area_px",
                "tile_size",
                "changed_tiles",
                "pending_tiles",
                "dirty_tiles",
                "covered_area_px",
                "budget_area_px",
                "max_regions",
            ] {
                assert!(parsed[key].is_number(), "{key} in {line}");
            }
            let decided = &parsed["decided_by"];
            assert!(decided.is_null() || decided.is_string(), "{line}");
            assert_eq!(
                decided.as_str(),
                page.plan.full_refresh_reason.map(FullRefreshReason::as_str),
                "{line}"
            );
            for component in parsed["components"].as_array().unwrap() {
                assert_eq!(
                    sorted_keys(component),
                    [
                        "closed_area_px",
                        "largest_support_px",
                        "padded_area_px",
                        "tight_area_px",
                        "tiles"
                    ],
                    "{line}"
                );
                assert!(
                    component
                        .as_object()
                        .unwrap()
                        .values()
                        .all(Value::is_number)
                );
            }
            for region in parsed["regions"].as_array().unwrap() {
                assert_eq!(
                    sorted_keys(region),
                    [
                        "area_px",
                        "components",
                        "largest_support_px",
                        "pre_closure_area_px"
                    ],
                    "{line}"
                );
                assert!(region.as_object().unwrap().values().all(Value::is_number));
                seen_regions = true;
            }
            // The trace carries the screen's text nowhere.
            for text in ["Name", "Cancel", "Save", "hello"] {
                assert!(!line.contains(text), "{text:?} in {line}");
            }
        }
        assert!(seen_regions, "an observation of the demo planned regions");
    }

    /// A failed observation has its line: the same keys, the code, and nothing measured.
    #[test]
    fn a_failed_observation_has_a_plan_trace_line_that_measures_nothing() {
        let error = ToolError::new(ErrorCode::InferenceFailed, "detection failed");
        let record = plan_trace_error_record(3, &error);
        let mut expected: Vec<String> = TRACE_KEYS.iter().map(|k| (*k).to_owned()).collect();
        expected.sort();
        assert_eq!(sorted_keys(&record), expected, "{record}");
        assert_eq!(record["schema"], PLAN_TRACE_SCHEMA);
        assert_eq!(record["observation_id"], 3);
        assert_eq!(record["outcome"], "error");
        assert_eq!(record["error_code"], "INFERENCE_FAILED");
        assert_eq!(record["components"], json!([]));
        assert_eq!(record["regions"], json!([]));
        for key in expected {
            if ![
                "schema",
                "observation_id",
                "outcome",
                "error_code",
                "components",
                "regions",
            ]
            .contains(&key.as_str())
            {
                assert!(record[&key].is_null(), "{key} in {record}");
            }
        }
    }

    #[test]
    fn the_engine_keeps_no_plan_trace_unless_asked() {
        let mut wired = sim_wired("trace-off");
        assert!(wired.engine.last_plan_trace().is_none());
        let request = ObserveRequest::default();
        wired.engine.observe_screen(&request).unwrap();
        assert!(wired.engine.last_plan_trace().is_none(), "off by default");
        wired.engine.set_plan_trace(true);
        wired.engine.observe_screen(&request).unwrap();
        assert!(wired.engine.last_plan_trace().is_some());
        wired.engine.set_plan_trace(false);
        wired.engine.observe_screen(&request).unwrap();
        assert!(wired.engine.last_plan_trace().is_none());
    }

    #[test]
    fn asking_for_the_plan_trace_does_not_change_what_the_engine_does() {
        let run = |trace: bool| {
            let mut wired = sim_wired("trace-same");
            wired.engine.set_plan_trace(trace);
            let sim = wired.sim.as_ref().unwrap();
            let mut out = Vec::new();
            for n in 1..=3 {
                let _ = play_user(sim, n);
                let page = wired
                    .engine
                    .observe_screen(&ObserveRequest::default())
                    .unwrap();
                out.push((page.plan.clone(), all_elements(&mut wired.engine, &page)));
            }
            out
        };
        assert_eq!(run(true), run(false));
    }

    /// The keys of one `--plan-check` line (ADR-0045).
    const CHECK_KEYS: [&str; 9] = [
        "checked",
        "error_code",
        "full_truncated",
        "observation_id",
        "only_full",
        "only_regional",
        "outcome",
        "schema",
        "work",
    ];

    /// `--plan-check` on the simulated desktop: a regional observation is compared with a
    /// full read, and the line is counts only. The simulated detector reports whole ground
    /// truth, so nothing differs.
    #[test]
    fn a_plan_check_line_holds_counts_only_and_finds_no_difference_on_the_simulated_desktop() {
        let mut wired = sim_wired("check");
        wired.engine.set_plan_check(true);
        let sim = wired.sim.as_ref().unwrap();
        let mut regional = 0;
        for n in 1..=3 {
            let _ = play_user(sim, n);
            let page = wired
                .engine
                .observe_screen(&ObserveRequest::default())
                .unwrap();
            let check = wired.engine.last_plan_check();
            let record = plan_check_record(n, &page, check);
            let line = record.to_string();
            assert_eq!(sorted_keys(&record), CHECK_KEYS, "{line}");
            assert_eq!(record["schema"], PLAN_CHECK_SCHEMA);
            assert_eq!(
                record["checked"],
                page.plan.work == ommatix_application::contract::WorkKind::Regions
            );
            if let Some(check) = check {
                regional += 1;
                assert_eq!(check.only_regional, CheckKinds::default(), "{line}");
                assert_eq!(check.only_full, CheckKinds::default(), "{line}");
                for side in ["only_regional", "only_full"] {
                    for kind in ["controls", "words", "accessibility"] {
                        let counts = record[side][kind].as_object().unwrap();
                        assert!(counts.values().all(Value::is_number), "{line}");
                    }
                }
            }
            for text in ["Name", "Cancel", "Save", "hello"] {
                assert!(!line.contains(text), "{text:?} in {line}");
            }
        }
        assert!(regional > 0, "the demo planned a regional observation");
    }

    #[test]
    fn asking_for_the_plan_check_does_not_change_what_the_engine_does() {
        let run = |check: bool| {
            let mut wired = sim_wired("check-same");
            wired.engine.set_plan_check(check);
            let sim = wired.sim.as_ref().unwrap();
            let mut out = Vec::new();
            for n in 1..=3 {
                let _ = play_user(sim, n);
                let page = wired
                    .engine
                    .observe_screen(&ObserveRequest::default())
                    .unwrap();
                out.push((page.plan.clone(), all_elements(&mut wired.engine, &page)));
            }
            out
        };
        assert_eq!(run(true), run(false));
    }

    #[test]
    fn a_failed_observation_has_a_plan_check_line_that_measures_nothing() {
        let error = ToolError::new(ErrorCode::InferenceFailed, "detection failed");
        let record = plan_check_error_record(2, &error);
        assert_eq!(sorted_keys(&record), CHECK_KEYS);
        assert_eq!(record["outcome"], "error");
        assert_eq!(record["checked"], false);
        assert!(record["only_regional"].is_null() && record["only_full"].is_null());
    }

    /// Schema 2 of the trace (ADR-0045): on the simulated desktop the detector declares no
    /// surfaces, so the closure is the classic one and there is no `spanning` record.
    #[test]
    fn a_plan_trace_line_says_which_closure_ran() {
        let mut wired = sim_wired("trace-chosen");
        wired.engine.set_plan_trace(true);
        let page = wired
            .engine
            .observe_screen(&ObserveRequest::default())
            .unwrap();
        let record = plan_trace_record(1, &page, wired.engine.last_plan_trace().unwrap());
        assert_eq!(record["schema"], "ommatix.plan-trace/2");
        assert_eq!(record["chosen"], "classic");
        assert!(record["spanning"].is_null());
        assert!(record["component_support_classes"].is_array());
    }

    #[test]
    fn an_error_record_keeps_its_code_and_measures_nothing() {
        let error = ToolError::new(ErrorCode::InferenceFailed, "detection failed");
        let record = error_record(3, &error);
        assert_eq!(record["outcome"], "error");
        assert_eq!(record["error_code"], "INFERENCE_FAILED");
        assert!(
            record["timings_us"].is_null() && record["plan"].is_null(),
            "{record}"
        );
        assert_eq!(record["signatures"], json!([]));
    }

    #[test]
    fn the_tally_counts_every_element_of_the_simulated_snapshot() {
        let args = crate::cli::RunArgs {
            config: None,
            source: crate::cli::SourceKind::Sim,
            images: None,
            output: 0,
            input: None,
            theme: crate::cli::ThemeKind::Light,
            session_id: Some("tally".to_owned()),
            legacy: false,
        };
        let mut wired = crate::wiring::build(&args, &crate::config::Config::default()).unwrap();
        let request = ObserveRequest {
            limit: 1,
            ..ObserveRequest::default()
        };
        let page = wired.engine.observe_screen(&request).unwrap();
        assert!(
            page.total_elements > 1,
            "the simulated desktop has several elements"
        );
        let elements = all_elements(&mut wired.engine, &page);
        assert_eq!(elements.len(), page.total_elements);
        let line = tally(&elements);
        let counted: usize = line
            .split(" | ")
            .next()
            .unwrap()
            .trim_start_matches("role: ")
            .split(", ")
            .map(|kv| kv.rsplit_once(' ').unwrap().1.parse::<usize>().unwrap())
            .sum();
        assert_eq!(counted, page.total_elements, "{line}");
        assert!(line.contains("| actionability: ") && line.contains("| label_source: "));
    }
}
