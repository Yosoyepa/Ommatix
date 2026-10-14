//! `ommatix` binary: parse the CLI, wire the services, run a driving adapter.

mod cli;
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
            let services = wiring::build(&args)?;
            eprintln!(
                "ommatix: MCP server on stdio (source={}, detector={}, input={})",
                services.perception.source_name(),
                services.perception.detector_name(),
                services.actuation.input_name(),
            );
            let stdin = io::stdin().lock();
            let stdout = BufWriter::new(io::stdout().lock());
            McpServer::new(services.perception, services.actuation).serve(stdin, stdout)?;
        }
        Command::Demo(args) => run_demo(wiring::build(&args)?.perception)?,
    }
    Ok(())
}

/// Frames printed by `demo` when the source never ends (`--loop`, `sim`, `screen`).
const DEMO_MAX_FRAMES: u64 = 20;

fn run_demo(mut engine: PerceptionEngine) -> Result<()> {
    println!(
        "source={} detector={}",
        engine.source_name(),
        engine.detector_name()
    );
    while let Some(r) = engine.tick()? {
        println!(
            "frame {:>3} | dirty {:>4}/{:<4} cells | regions {:>2} | +{} ~{} -{} | elements {:>3} | {} us",
            r.frame_no,
            r.dirty_cells,
            r.total_cells,
            r.dirty_regions.len(),
            r.delta.added.len(),
            r.delta.updated.len(),
            r.delta.removed.len(),
            r.element_count,
            r.timings.total_us,
        );
        if engine.frame_no() >= DEMO_MAX_FRAMES {
            break;
        }
    }
    for e in engine.elements() {
        let c = e.bounds.center();
        println!(
            "  {:<4} {:?} at ({},{}) {}x{} click=({},{})",
            e.id.to_string(),
            e.kind,
            e.bounds.x,
            e.bounds.y,
            e.bounds.w,
            e.bounds.h,
            c.x,
            c.y
        );
    }
    Ok(())
}
