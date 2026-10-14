//! `ommatix` binary: read the configuration, wire the engine, run a front end.
//!
//! - `serve`: the MCP server on stdio. stdout carries protocol messages only; every
//!   diagnostic goes to stderr.
//! - `demo`: a few observations and one dry-run click, printed for a human.

mod cli;
mod clock;
mod config;
mod demo;
mod prototypes;
mod wiring;

use anyhow::{Context, Result};
use clap::Parser;
use ommatix_adapter_mcp::{LEGACY_PROTOCOL_VERSION, MODERN_PROTOCOL_VERSION, McpServer};

use crate::cli::{Cli, Command, DemoArgs, RunArgs};
use crate::config::Config;

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Serve(args) => serve(&args),
        Command::Demo(args) => run_demo(&args),
    }
}

fn report_notes(notes: &[String]) {
    for note in notes {
        eprintln!("ommatix: note: {note}");
    }
}

fn serve(args: &RunArgs) -> Result<()> {
    let config = Config::load(args.config.as_deref())?;
    let wired = wiring::build(args, &config)?;
    let mcp = config.mcp_config(args.legacy);
    let policy = config.local_policy();
    eprintln!(
        "ommatix: serving MCP {MODERN_PROTOCOL_VERSION}{} on stdio | session {} | {} | \
         actuation {} | values {}",
        if mcp.legacy_enabled {
            format!(" (+ legacy {LEGACY_PROTOCOL_VERSION})")
        } else {
            String::new()
        },
        wired.engine.session_id(),
        wired.summary,
        if policy.actuation_enabled {
            "enabled"
        } else {
            "disabled"
        },
        if policy.include_values {
            "allowed"
        } else {
            "redacted"
        },
    );
    report_notes(&wired.notes);
    if let Some(indicator) = &wired.active_control {
        eprintln!("ommatix: {indicator}");
    }
    let server = McpServer::new(wired.engine, mcp);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .context("cannot start the I/O runtime")?;
    runtime
        .block_on(server.serve_stdio())
        .context("the stdio transport failed")
}

fn run_demo(args: &DemoArgs) -> Result<()> {
    let config = Config::load(args.run.config.as_deref())?;
    let mut wired = wiring::build(&args.run, &config)?;
    report_notes(&wired.notes);
    demo::run(
        &mut wired.engine,
        wired.sim.as_ref(),
        &wired.summary,
        args.observations,
    );
    Ok(())
}
