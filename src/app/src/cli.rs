//! Command-line surface. Pure data: no wiring decisions live here.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

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
    /// Serve the Ommatix tools over MCP on stdio (point your agent's MCP client at this
    /// command). Diagnostics go to stderr; stdout carries protocol messages only.
    Serve(RunArgs),
    /// Run a few observations over the source and print them, plus one dry-run click,
    /// for a human.
    Demo(DemoArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SourceKind {
    /// The simulated desktop, with its own detector, recogniser and (never live)
    /// actuator.
    Sim,
    /// A directory of PNG frames (`--images DIR`), with the `[models]` detector and OCR
    /// (by default, the flat-region detector and no OCR).
    Images,
    /// Live capture of the primary monitor through `xcap`, with the `[models]` detector and
    /// OCR (build with `--features screen`). Interim; superseded by `dxgi` on Windows.
    Screen,
    /// Live capture of one monitor (`--output`, default 0) through DXGI Desktop
    /// Duplication, in physical pixels, with the `[models]` detector and OCR (Windows; build
    /// with `--features dxgi`). Frames stay in memory.
    Dxgi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InputKind {
    /// Simulate and record actions; never touch the OS.
    DryRun,
    /// Real mouse and keyboard through `enigo`. Needs `--source screen`, `[safety]
    /// actuation_enabled = true` and a build with `--features native-input`. It cannot
    /// attest the foreground application, so the engine refuses its live actions.
    Native,
    /// Real mouse and keyboard through Win32 `SendInput`. Needs `--source dxgi`, `[safety]
    /// actuation_enabled = true`, a non-empty `[safety] allowed_applications` of lowercase
    /// executable names (e.g. "notepad.exe") and a Windows build with `--features
    /// sendinput`.
    Sendinput,
}

/// Colour scheme of the simulated desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ThemeKind {
    Light,
    Dark,
    ContrastingWallpaper,
}

#[derive(Debug, Clone, clap::Args)]
pub struct RunArgs {
    /// Configuration file shaped like the dossier's `configuracion_mvp.toml`; without
    /// it, the template's defaults apply (no actuation, no values).
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = SourceKind::Sim)]
    pub source: SourceKind,
    /// Directory with PNG frames, for `--source images`.
    #[arg(long, value_name = "DIR")]
    pub images: Option<PathBuf>,
    /// Output to duplicate, for `--source dxgi`: the n-th display output attached to the
    /// desktop, in DXGI order (0 is normally the primary monitor).
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub output: u32,
    /// Input adapter; defaults to `[engine] mode` (`dry-run` in the template).
    #[arg(long, value_enum)]
    pub input: Option<InputKind>,
    /// Theme of the simulated desktop.
    #[arg(long, value_enum, default_value_t = ThemeKind::Light)]
    pub theme: ThemeKind,
    /// Session identifier (1-64 visible ASCII characters). Defaults to one derived from
    /// the start time, unique per process; set it for reproducible output.
    #[arg(long, value_name = "ID")]
    pub session_id: Option<String>,
    /// Also serve the legacy MCP 2025-11-25 profile (same as `[mcp] legacy_enabled =
    /// true`).
    #[arg(long)]
    pub legacy: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct DemoArgs {
    #[command(flatten)]
    pub run: RunArgs,
    /// Observations to run.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(1..=20))]
    pub observations: u8,
    /// Also write one record per observation to stderr (`ommatix-observation` and a JSON
    /// object: stage timings in whole microseconds, the plan and the element signatures). They
    /// depend on the run, so they never reach stdout.
    #[arg(long)]
    pub timings: bool,
}
