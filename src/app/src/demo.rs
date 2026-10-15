//! `ommatix demo`: a few observations and one dry-run click, printed for a human.
//!
//! This is the one module allowed to print to stdout. Its output depends only on the
//! source and the session id (no timings), so `--session-id` makes it reproducible. With
//! `--timings`, each observation also writes one record to stderr (`ommatix-observation`
//! followed by a JSON object, schema `ommatix.observation/1`, dossier chapter 09 §9.9): the
//! stage timings in whole microseconds, the plan and a signature per element. They depend
//! on the run, so they never reach stdout, and no label is written, only its signature.

// `demo` is the one command whose output is meant for a human on stdout.
#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::time::Duration;

use ommatix_adapter_sim::{SimulatedDesktop, layout};
use ommatix_application::Engine;
use ommatix_application::contract::{
    ActionKind, ActionRequest, ElementView, ExpectedEffect, ObservePage, ObserveRequest, ToolError,
};
use ommatix_domain::fusion::Actionability;
use ommatix_domain::incremental::FullRefreshReason;
use ommatix_domain::{Action, MouseButton};
use serde_json::{Value, json};

/// The schema of the per-observation record written by `--timings`.
const OBSERVATION_SCHEMA: &str = "ommatix.observation/1";

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
/// go to stderr.
pub fn run(
    engine: &mut Engine,
    sim: Option<&SimulatedDesktop>,
    summary: &str,
    observations: u8,
    timings: bool,
) {
    println!("ommatix demo | session {} | {summary}", engine.session_id());
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
                last = Some(page);
            }
            Err(e) => {
                println!("observation {n}: {} ({})", e.code.as_str(), e.message);
                if timings {
                    eprintln!("ommatix-observation {}", error_record(n, &e));
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
