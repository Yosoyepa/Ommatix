//! Composition root: the only place that knows every concrete adapter. It parses the
//! CLI, wires driven adapters (capture, vision) into the application core and hands the
//! engine to a driving adapter (MCP server or demo printer).

use std::io::{self, BufWriter};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use ommatix_adapter_capture::{ScriptedSource, demo_script};
use ommatix_adapter_mcp::McpServer;
use ommatix_adapter_vision::{ContrastBlobConfig, ContrastBlobDetector};
use ommatix_application::{FrameSource, PerceptionConfig, PerceptionEngine};

#[derive(Debug, Parser)]
#[command(
    name = "ommatix",
    version,
    about = "Spatial indexing and incremental perception for GUI agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the MCP server on stdio (point your agent's MCP client at this command).
    Serve(EngineArgs),
    /// Run the perception pipeline over a source and print one line per frame.
    Demo(EngineArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SourceKind {
    /// Built-in synthetic desktop session (deterministic).
    Demo,
    /// A directory of PNG frames (`--images`).
    Images,
    /// Live capture of the primary monitor (build with `--features screen`).
    Screen,
}

#[derive(Debug, clap::Args)]
struct EngineArgs {
    #[arg(long, value_enum, default_value_t = SourceKind::Demo)]
    source: SourceKind,
    /// Directory with PNG frames, used with `--source images`.
    #[arg(long)]
    images: Option<PathBuf>,
    /// Replay the demo script forever instead of stopping after the last frame.
    #[arg(long = "loop")]
    looping: bool,
    /// Grid cell edge in pixels.
    #[arg(long, default_value_t = PerceptionConfig::default().cell_size)]
    cell_size: u32,
    /// Max per-channel colour distance treated as background by the detector.
    #[arg(long, default_value_t = ContrastBlobConfig::default().tolerance)]
    tolerance: u8,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve(args) => {
            let engine = build_engine(&args)?;
            eprintln!(
                "ommatix: MCP server on stdio (source={}, detector={})",
                engine.source_name(),
                engine.detector_name()
            );
            let stdin = io::stdin().lock();
            let stdout = BufWriter::new(io::stdout().lock());
            McpServer::new(engine).serve(stdin, stdout)?;
        }
        Command::Demo(args) => run_demo(build_engine(&args)?)?,
    }
    Ok(())
}

fn build_engine(args: &EngineArgs) -> Result<PerceptionEngine> {
    let source: Box<dyn FrameSource> = match args.source {
        SourceKind::Demo => {
            let s = ScriptedSource::new(demo_script());
            Box::new(if args.looping { s.looping() } else { s })
        }
        SourceKind::Images => {
            let dir = args
                .images
                .as_ref()
                .context("--source images requires --images <DIR>")?;
            Box::new(ommatix_adapter_capture::ImageSequenceSource::from_dir(dir)?)
        }
        SourceKind::Screen => screen_source()?,
    };
    let detector = ContrastBlobDetector::new(ContrastBlobConfig {
        tolerance: args.tolerance,
        ..ContrastBlobConfig::default()
    });
    let config = PerceptionConfig {
        cell_size: args.cell_size,
        ..PerceptionConfig::default()
    };
    Ok(PerceptionEngine::new(source, Box::new(detector), config)?)
}

#[cfg(feature = "screen")]
fn screen_source() -> Result<Box<dyn FrameSource>> {
    Ok(Box::new(ommatix_adapter_capture::ScreenSource::primary()?))
}

#[cfg(not(feature = "screen"))]
fn screen_source() -> Result<Box<dyn FrameSource>> {
    anyhow::bail!("this build has no screen capture; rebuild with `cargo build --features screen`")
}

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
        if engine.frame_no() > 10_000 {
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
