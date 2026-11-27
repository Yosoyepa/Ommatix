//! `ommatix` binary: read the configuration, wire the engine, run a front end.
//!
//! - `serve`: the MCP server on stdio. stdout carries protocol messages only; every
//!   diagnostic goes to stderr. With `[mcp] transport = "pipe"` (a Windows build with the
//!   `broker-pipe` feature; ADR-0043, proposed) it serves the broker over a local named pipe
//!   instead, and nothing on stdio.
//! - `attach` (with `broker-pipe`): relays stdio to that pipe, for one agent.
//! - `demo`: a few observations and one dry-run click, printed for a human.

#[cfg(feature = "broker-pipe")]
mod attach;
mod cli;
mod clock;
mod config;
mod demo;
mod freshness_probe;
#[cfg(all(windows, feature = "broker-pipe"))]
mod pipe_end;
mod prototypes;
mod wiring;

use anyhow::{Context, Result};
use clap::Parser;
use ommatix_adapter_mcp::{LEGACY_PROTOCOL_VERSION, MODERN_PROTOCOL_VERSION, McpServer};

use crate::cli::{Cli, Command, DemoArgs, RunArgs};
use crate::config::Config;

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Serve(args) => exit_if_the_pipe_was_refused(serve(&args)),
        Command::Demo(args) => run_demo(&args),
        Command::FreshnessProbe(args) => freshness_probe::run(&args),
        #[cfg(feature = "broker-pipe")]
        Command::Attach(args) => run_attach(&args),
    }
}

/// `attach`: one stderr line that says which side ended, then the exit code that tells it
/// (`attach::Outcome::code`). A refusal to start is an error, exit code 1.
///
/// Nothing here touches stdout: the relay wrote the engine's bytes to its own duplicate of the
/// handle, past Rust's buffer, so the runtime's cleanup in `exit` has nothing to write, and no
/// flush waits on a host that stopped reading (see the `attach` module). The one stderr line is
/// the only write `run_attach` itself makes after the relay returns (the relay's other direction,
/// detached, can still write to stdout and can remain blocked after its source closes); the
/// exit's budget assumes that line can be written.
#[cfg(feature = "broker-pipe")]
fn run_attach(args: &cli::AttachArgs) -> Result<()> {
    let ended = attach::run(args)?;
    eprintln!("ommatix: attach: {}", ended.message());
    // Exit now: the relay's other direction may still wait on a read or on a write to a host
    // that does not read, and nothing of this process needs to outlive its relay.
    std::process::exit(ended.code())
}

/// `serve`'s exit code 7 ([`pipe_end::BIND_REFUSED`]): the system refused the first instance of
/// the pipe's name with access denied. It is told after `serve` returned, so that everything it
/// built is dropped first; any other error stays exit code 1.
#[cfg(all(windows, feature = "broker-pipe"))]
fn exit_if_the_pipe_was_refused(served: Result<()>) -> Result<()> {
    if let Err(error) = &served
        && let Some(refused) = error.downcast_ref::<pipe_end::BindRefused>()
    {
        eprintln!("ommatix: {refused}");
        std::process::exit(pipe_end::BIND_REFUSED);
    }
    served
}

#[cfg(not(all(windows, feature = "broker-pipe")))]
fn exit_if_the_pipe_was_refused(served: Result<()>) -> Result<()> {
    served
}

fn report_notes(notes: &[String]) {
    for note in notes {
        eprintln!("ommatix: note: {note}");
    }
}

fn serve(args: &RunArgs) -> Result<()> {
    let config = Config::load(args.config.as_deref())?;
    if config.pipe() {
        return serve_pipe(args, &config);
    }
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
    let outcome = runtime.block_on(server.serve_stdio());
    // The session is over, however it ended: the engine has deleted its state and cache.
    eprintln!("ommatix: session ended; the engine's state and cache were deleted");
    outcome.context("the stdio transport failed")
}

/// `serve` with `[mcp] transport = "pipe"`: the broker of ADR-0039 behind the pipe of ADR-0043
/// (proposed), and nothing on stdio. It serves until the process ends (Ctrl+C) or the pipe's
/// listener stops accepting; then it shuts the listener down, says why and where it was left,
/// and exits with 3, 4 or 5 ([`pipe_end`]). When the system refuses the first instance of the
/// name with access denied, it returns [`pipe_end::BindRefused`] (exit code 7).
#[cfg(all(windows, feature = "broker-pipe"))]
fn serve_pipe(args: &RunArgs, config: &Config) -> Result<()> {
    use ommatix_adapter_mcp::pipe::{PipeError, PipeName, PipeServer};
    use ommatix_application::JOURNAL_CLIENTS;

    if args.legacy {
        anyhow::bail!(
            "--legacy cannot be combined with [mcp] transport = \"pipe\": the broker serves the \
             2026-07-28 profile only"
        );
    }
    let name =
        PipeName::new(&config.pipe_name()?).map_err(|e| anyhow::anyhow!("[mcp] pipe_name: {e}"))?;
    let wired = wiring::build_broker(args, config)?;
    let policy = config.local_policy();
    let mut server = match PipeServer::bind(wired.broker, name.clone()) {
        Ok(server) => server,
        Err(PipeError::NameTaken) => {
            return Err(pipe_end::BindRefused {
                path: name.path().to_owned(),
            }
            .into());
        }
        Err(error) => anyhow::bail!("cannot serve on the pipe {}: {error}", name.path()),
    };
    eprintln!(
        "ommatix: serving MCP {MODERN_PROTOCOL_VERSION} on the pipe {} to at most \
         {JOURNAL_CLIENTS} clients through `ommatix attach` (ADR-0043, proposed) | session {} | \
         {} | actuation {} | values {}",
        name.path(),
        wired.session_id,
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
    eprintln!("ommatix: {}", wired.grants);
    report_notes(&wired.notes);
    if let Some(indicator) = &wired.active_control {
        eprintln!("ommatix: {indicator}");
    }
    // The listener serves on its own threads; this one only watches whether it still accepts.
    let ended = pipe_end::watch(&mut server, pipe_end::WATCH_EVERY, pipe_end::SHUTDOWN_WAIT);
    for line in ended.lines() {
        eprintln!("ommatix: {line}");
    }
    // Exit now, without dropping the server: whatever the listener still holds (a worker not
    // joined, a request that passed its stop check and still runs, an accept thread still
    // waiting, the reaper) ends with the process. `exit` runs no destructor, so the broker's
    // final teardown is the end of the process, never a detached thread of the listener.
    std::process::exit(ended.code())
}

#[cfg(not(all(windows, feature = "broker-pipe")))]
fn serve_pipe(_: &RunArgs, _: &Config) -> Result<()> {
    // `Config::validate` refuses the switch on this build before anything is built.
    anyhow::bail!("[mcp] transport = \"pipe\" needs a Windows build with `--features broker-pipe`")
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
        args.timings,
    );
    Ok(())
}
