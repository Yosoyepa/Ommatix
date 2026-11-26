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
    /// M1: read-only DXGI frame pairs and window-group counts; no engine or live input.
    FreshnessProbe(FreshnessProbeArgs),
}

#[derive(Debug, Clone, clap::Args)]
pub struct FreshnessProbeArgs {
    #[arg(long)]
    pub config: PathBuf,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100_000))]
    pub pairs: u32,
    #[arg(long, default_value_t = 150, value_parser = clap::value_parser!(u64).range(1..=60_000))]
    pub interval_ms: u64,
    /// Owner's pre-registered criterion: minimum pairs with changes outside the group.
    /// No default; this assesses a measurement, never authorises input.
    #[arg(
        long,
        conflicts_with = "descriptive",
        required_unless_present = "descriptive"
    )]
    pub min_outside_pairs: Option<u32>,
    /// Record the owner's descriptive M1 protocol, with no pass/fail threshold.
    #[arg(
        long,
        conflicts_with = "min_outside_pairs",
        required_unless_present = "min_outside_pairs"
    )]
    pub descriptive: bool,
    #[arg(long, default_value_t = 0)]
    pub output: u32,
    /// Exact policy-allowed class of the foreground window the operator selected.
    #[arg(long)]
    pub window_class: String,
    /// Local operator coordinates in the captured output's physical pixels; never emitted.
    #[arg(long)]
    pub target_x: u32,
    #[arg(long)]
    pub target_y: u32,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    pub target_width: u32,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    pub target_height: u32,
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
    /// the start time, unique per process; set it for reproducible output. With live input a
    /// fixed identifier is refused at start: a restarted run with the same identifier would
    /// accept an earlier request (chapter 08 §8.4).
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
    /// Also write one line per observation to stderr (`ommatix-plan-trace` and a JSON object):
    /// why the planner chose that work, as counts and areas only (changed tiles, components,
    /// each region before and after the closure over the last evidence, the sum the budget
    /// compared, the check that decided the work that ran, and what the crop-edge defence did).
    /// No pixel, label or coordinate; never on stdout.
    #[arg(long)]
    pub plan_trace: bool,
    /// Also compare each regional observation with a full read of the same frame and write
    /// one line to stderr (`ommatix-plan-check` and a JSON object): how many boxes each read
    /// has and the other lacks, per kind, split by whether they overlap a box the
    /// support-aware closure let a region meet without containing (ADR-0045). A diagnostic:
    /// it runs perception a second time after the observation is committed, so it doubles
    /// the cost of a regional observation, and its time is never reported as the
    /// observation's. For the words that differ it also says where each lies against the
    /// regions and the crops read and how the two reads' words pair up: a differing word
    /// that meets a region and lies clear of its crop's edge band is the stated limit L3 (the
    /// recogniser read it whole from a crop); one that meets a region and reaches the band
    /// (it may have been cut) or that lies outside every region (missed, or held from an
    /// earlier read) is never L3, and `never_l3` counts those. Counts and one distance in pixels; no text, no coordinate;
    /// never on stdout.
    #[arg(long)]
    pub plan_check: bool,
}
