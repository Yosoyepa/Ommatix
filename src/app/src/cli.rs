//! Command-line surface. Pure data: no wiring decisions live here.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use ommatix_adapter_vision::ContrastBlobConfig;
use ommatix_application::PerceptionConfig;

#[derive(Debug, Parser)]
#[command(
    name = "ommatix",
    version,
    about = "Spatial indexing and incremental perception for GUI agents"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the MCP server on stdio (point your agent's MCP client at this command).
    Serve(EngineArgs),
    /// Run the perception pipeline over a source and print one line per frame.
    Demo(EngineArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SourceKind {
    /// Built-in scripted desktop session (replay, ignores input).
    Demo,
    /// Interactive simulated desktop: clicks and typing change the next frame.
    Sim,
    /// A directory of PNG frames (`--images`).
    Images,
    /// Live capture of the primary monitor (build with `--features screen`).
    Screen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InputKind {
    /// `sim` when `--source sim`, otherwise `dry-run`.
    Auto,
    /// Record actions, never touch the OS.
    DryRun,
    /// Real mouse and keyboard (build with `--features native-input`).
    Native,
}

#[derive(Debug, clap::Args)]
pub struct EngineArgs {
    #[arg(long, value_enum, default_value_t = SourceKind::Demo)]
    pub source: SourceKind,
    /// Directory with PNG frames, used with `--source images`.
    #[arg(long)]
    pub images: Option<PathBuf>,
    /// Replay the demo script forever instead of stopping after the last frame.
    #[arg(long = "loop")]
    pub looping: bool,
    #[arg(long, value_enum, default_value_t = InputKind::Auto)]
    pub input: InputKind,
    /// Grid cell edge in pixels.
    #[arg(long, default_value_t = PerceptionConfig::default().cell_size)]
    pub cell_size: u32,
    /// Max per-channel colour distance treated as background by the detector.
    #[arg(long, default_value_t = ContrastBlobConfig::default().tolerance)]
    pub tolerance: u8,
}
