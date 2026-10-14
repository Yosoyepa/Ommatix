//! `ommatix` binary: parse the CLI, wire the services, run a driving adapter.

mod cli;
mod clock;
mod wiring;

use std::io::{self, BufWriter};

use anyhow::Result;
use clap::Parser;
use ommatix_adapter_mcp::McpServer;
use ommatix_application::PerceptionEngine;

use crate::cli::{Cli, Command};

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Serve(args) => {
            let session = wiring::build(&args)?;
            eprintln!(
                "ommatix: MCP server on stdio (source={}, detector={}, input={})",
                session.perception().source_name(),
                session.perception().detector_name(),
                session.actuation().input_name(),
            );
            let stdin = io::stdin().lock();
            let stdout = BufWriter::new(io::stdout().lock());
            McpServer::new(session).serve(stdin, stdout)?;
        }
        Command::Demo(args) => run_demo(wiring::build(&args)?.perception_mut())?,
    }
    Ok(())
}

/// Frames printed by `demo` when the source never ends (`--loop`, `sim`, `screen`).
const DEMO_MAX_FRAMES: u64 = 20;

/// `demo` is the one command whose output is meant for a human on stdout.
#[allow(clippy::print_stdout)]
fn run_demo(engine: &mut PerceptionEngine) -> Result<()> {
    println!(
        "source={} detector={}",
        engine.source_name(),
        engine.detector_name()
    );
    while let Some(r) = engine.tick()? {
        println!(
            "frame {:>3} | dirty {:>4}/{:<4} cells | analysed {:>7} px in {:>2} regions | +{} ~{} -{} | elements {:>3} | {} us",
            r.frame_no,
            r.dirty_cells,
            r.total_cells,
            r.analysed_px,
            r.analysed_regions.len(),
            r.delta.added.len(),
            r.delta.updated.len(),
            r.delta.removed.len(),
            r.element_count,
            r.timings.total.as_micros(),
        );
        if engine.frame_no() >= DEMO_MAX_FRAMES {
            break;
        }
    }
    for e in engine.elements() {
        let (b, c) = (e.bounds, e.bounds.center());
        println!(
            "  e{:<3} {:?} at ({},{}) {}x{} click=({},{})",
            e.id.0, e.kind, b.x, b.y, b.w, b.h, c.x, c.y
        );
    }
    Ok(())
}
