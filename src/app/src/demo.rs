//! `ommatix demo`: a few observations and one dry-run click, printed for a human.
//!
//! This is the one module allowed to print to stdout. Its output depends only on the
//! source and the session id (no timings), so `--session-id` makes it reproducible.

// `demo` is the one command whose output is meant for a human on stdout.
#![allow(clippy::print_stdout)]

use ommatix_adapter_sim::{SimulatedDesktop, layout};
use ommatix_application::Engine;
use ommatix_application::contract::{
    ActionKind, ActionRequest, ElementView, ExpectedEffect, ObservePage, ObserveRequest,
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
            "  {:<4} rev {:<3} {:<10} {:<14} {}{}",
            e.target_id.to_string(),
            e.revision,
            snake(e.role),
            format!("{:?}", e.label),
            snake(e.actionability),
            if e.enabled == Some(false) {
                " (disabled)"
            } else {
                ""
            },
        );
    }
}

fn clickable(page: &ObservePage) -> Option<&ElementView> {
    page.elements
        .iter()
        .find(|e| e.actionability == Actionability::Confirmed && e.enabled == Some(true))
}

/// Runs `observations` observations, then a dry-run click on the first confirmed,
/// enabled element of the last page.
pub fn run(engine: &mut Engine, sim: Option<&SimulatedDesktop>, summary: &str, observations: u8) {
    println!("ommatix demo | session {} | {summary}", engine.session_id());
    let mut last = None;
    for n in 1..=observations {
        if let Some(what) = sim.and_then(|desktop| play_user(desktop, n)) {
            println!("user: {what}");
        }
        match engine.observe_screen(&ObserveRequest::default()) {
            Ok(page) => {
                print_page(n, &page);
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
}
