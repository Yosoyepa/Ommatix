//! Composition root: the only place that chooses concrete adapters and injects them
//! into the application services.

use std::sync::Arc;

use anyhow::{Context, Result};
use ommatix_adapter_capture::{ImageSequenceSource, ScriptedSource};
use ommatix_adapter_input::DryRunInput;
use ommatix_adapter_sim::{SimulatedDesktop, Theme, scripted_session};
use ommatix_adapter_vision::{FlatRegionConfig, FlatRegionDetector};
use ommatix_application::{
    ActuationPolicy, ActuationService, AgentSession, Clock, FrameSource, InputController, Observer,
    PerceptionConfig, PerceptionEngine, PortError, SettlePolicy,
};
use ommatix_domain::DisplayMapping;

use crate::cli::{EngineArgs, InputKind, SourceKind, ThemeKind};
use crate::clock::SystemClock;

pub fn build(args: &EngineArgs) -> Result<AgentSession> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
    let theme = theme(args.theme);
    // The simulator is one object behind two ports; both must share it.
    let sim = (args.source == SourceKind::Sim).then(|| SimulatedDesktop::with_theme(theme));

    let (source, mapping) = frame_source(args, theme, sim.as_ref())?;
    let input = input_controller(args.input, sim.as_ref(), mapping)?;
    let detector = FlatRegionDetector::new(FlatRegionConfig {
        tolerance: args.tolerance,
        ..FlatRegionConfig::default()
    });
    let config = PerceptionConfig {
        cell_size: args.cell_size,
        ..PerceptionConfig::default()
    };

    let perception = PerceptionEngine::new(source, Box::new(detector), clock.clone(), config)?;
    let observer = Observer::new(clock, SettlePolicy::default())?;
    let actuation = ActuationService::new(input, ActuationPolicy::default());
    Ok(AgentSession::new(perception, observer, actuation))
}

fn theme(kind: ThemeKind) -> Theme {
    match kind {
        ThemeKind::Light => Theme::Light,
        ThemeKind::Dark => Theme::Dark,
        ThemeKind::ContrastingWallpaper => Theme::ContrastingWallpaper,
    }
}

/// How a real screen's pixels map to OS input coordinates, or why native input cannot be
/// aimed on this system.
type ScreenMapping = std::result::Result<DisplayMapping, PortError>;

/// The frame source, plus its [`ScreenMapping`] when it is a real screen (only then can
/// native input be aimed).
fn frame_source(
    args: &EngineArgs,
    theme: Theme,
    sim: Option<&SimulatedDesktop>,
) -> Result<(Box<dyn FrameSource>, Option<ScreenMapping>)> {
    Ok(match args.source {
        SourceKind::Demo => {
            let s = ScriptedSource::new(scripted_session(theme));
            (Box::new(if args.looping { s.looping() } else { s }), None)
        }
        SourceKind::Sim => (
            Box::new(sim.context("simulator not created")?.screen()),
            None,
        ),
        SourceKind::Images => {
            let dir = args
                .images
                .as_ref()
                .context("--source images requires --images <DIR>")?;
            (Box::new(ImageSequenceSource::from_dir(dir)?), None)
        }
        SourceKind::Screen => {
            let (source, mapping) = screen_source()?;
            (source, Some(mapping))
        }
    })
}

fn input_controller(
    kind: InputKind,
    sim: Option<&SimulatedDesktop>,
    screen: Option<ScreenMapping>,
) -> Result<Box<dyn InputController>> {
    Ok(match (kind, sim) {
        (InputKind::Auto, Some(sim)) => Box::new(sim.input()),
        (InputKind::Auto | InputKind::DryRun, _) => {
            Box::new(DryRunInput::default().echo_to_stderr())
        }
        (InputKind::Native, _) => {
            let mapping = screen.context(
                "--input native requires --source screen: clicks must land on the screen \
                 the agent is looking at",
            )??;
            native_input(mapping)?
        }
    })
}

#[cfg(feature = "screen")]
fn screen_source() -> Result<(Box<dyn FrameSource>, ScreenMapping)> {
    let screen = ommatix_adapter_capture::ScreenSource::primary()?;
    let mapping = screen.display_mapping();
    Ok((Box::new(screen), mapping))
}

#[cfg(not(feature = "screen"))]
fn screen_source() -> Result<(Box<dyn FrameSource>, ScreenMapping)> {
    anyhow::bail!("this build has no screen capture; rebuild with `--features screen`")
}

#[cfg(feature = "native-input")]
fn native_input(mapping: DisplayMapping) -> Result<Box<dyn InputController>> {
    Ok(Box::new(ommatix_adapter_input::NativeInput::new(mapping)?))
}

#[cfg(not(feature = "native-input"))]
fn native_input(_: DisplayMapping) -> Result<Box<dyn InputController>> {
    anyhow::bail!("this build has no native input; rebuild with `--features native-input`")
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::{Cli, Command};

    fn args(line: &str) -> EngineArgs {
        let argv = std::iter::once("ommatix").chain(line.split_whitespace());
        match Cli::parse_from(argv).command {
            Command::Serve(a) | Command::Demo(a) => a,
        }
    }

    #[test]
    fn native_input_is_refused_unless_the_source_is_the_screen() {
        for line in ["serve --input native", "serve --source sim --input native"] {
            let err = build(&args(line)).unwrap_err();
            assert!(
                format!("{err:#}").contains("requires --source screen"),
                "{line}: {err:#}"
            );
        }
    }

    #[test]
    fn the_simulator_drives_both_ports_by_default() {
        let session = build(&args("serve --source sim --theme dark")).unwrap();
        assert_eq!(session.perception().source_name(), "simulated-desktop");
        assert_eq!(session.actuation().input_name(), "simulated-desktop");
        let replay = build(&args("demo")).unwrap();
        assert_eq!(replay.actuation().input_name(), "dry-run");
        assert_eq!(replay.perception().detector_name(), "flat-region");
    }

    #[test]
    fn invalid_settings_are_reported() {
        let err = build(&args("demo --cell-size 0")).unwrap_err();
        assert!(
            format!("{err:#}").contains("invalid configuration"),
            "{err:#}"
        );
    }
}
