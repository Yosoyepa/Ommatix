//! `ommatix demo`: a few observations and one dry-run click, printed for a human.
//!
//! This is the one module allowed to print to stdout. Its output depends only on the
//! source and the session id (no timings), so `--session-id` makes it reproducible. With
//! `--timings`, each observation's per-stage timings also go to stderr: they depend on the
//! run, so they never reach stdout.

// `demo` is the one command whose output is meant for a human on stdout.
#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::time::Duration;

use ommatix_adapter_sim::{SimulatedDesktop, layout};
use ommatix_application::Engine;
use ommatix_application::contract::{
    ActionKind, ActionRequest, ElementView, ExpectedEffect, ObservePage, ObserveRequest,
    StageTimings,
};
use ommatix_domain::fusion::Actionability;
use ommatix_domain::{Action, MouseButton};

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

/// Milliseconds with three decimals.
fn ms(d: Duration) -> String {
    format!("{:.3}", d.as_secs_f64() * 1000.0)
}

/// One stderr line of per-stage timings, in milliseconds.
fn timings_line(n: u8, t: &StageTimings) -> String {
    format!(
        "timings {n} (ms): capture {} plan {} detect {} recognize {} fuse {} track {} total {}",
        ms(t.capture),
        ms(t.plan),
        ms(t.detect),
        ms(t.recognize),
        ms(t.fuse),
        ms(t.track),
        ms(t.total),
    )
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
                println!("  totals: {}", tally(&all_elements(engine, &page)));
                if timings {
                    eprintln!("{}", timings_line(n, &page.timings));
                }
                last = Some(page);
            }
            Err(e) => {
                println!("observation {n}: {} ({})", e.code.as_str(), e.message);
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
    fn timings_are_printed_in_milliseconds_per_stage() {
        let t = StageTimings {
            capture: Duration::from_micros(1500),
            plan: Duration::from_micros(250),
            detect: Duration::from_millis(900),
            recognize: Duration::from_millis(40),
            fuse: Duration::from_micros(10),
            track: Duration::from_micros(20),
            total: Duration::from_micros(941_780),
        };
        assert_eq!(
            timings_line(2, &t),
            "timings 2 (ms): capture 1.500 plan 0.250 detect 900.000 recognize 40.000 \
             fuse 0.010 track 0.020 total 941.780"
        );
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
