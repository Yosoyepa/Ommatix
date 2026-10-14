//! Composition root: the only place that chooses concrete adapters and injects them
//! into the application services.

use anyhow::{Context, Result, bail};
use ommatix_adapter_capture::{ImageSequenceSource, ScriptedSource, demo_script};
use ommatix_adapter_input::DryRunInput;
use ommatix_adapter_sim::SimulatedDesktop;
use ommatix_adapter_vision::{ContrastBlobConfig, ContrastBlobDetector};
use ommatix_application::{
    ActuationPolicy, ActuationService, FrameSource, InputController, PerceptionConfig,
    PerceptionEngine,
};

use crate::cli::{EngineArgs, InputKind, SourceKind};

/// Fully wired application services.
#[derive(Debug)]
pub struct Services {
    pub perception: PerceptionEngine,
    pub actuation: ActuationService,
}

pub fn build(args: &EngineArgs) -> Result<Services> {
    // The simulator is one object behind two ports; both must share it.
    let sim = (args.source == SourceKind::Sim).then(SimulatedDesktop::new);

    let source = frame_source(args, sim.as_ref())?;
    let input = input_controller(args.input, sim.as_ref())?;

    let detector = ContrastBlobDetector::new(ContrastBlobConfig {
        tolerance: args.tolerance,
        ..ContrastBlobConfig::default()
    });
    let config = PerceptionConfig {
        cell_size: args.cell_size,
        ..PerceptionConfig::default()
    };
    Ok(Services {
        perception: PerceptionEngine::new(source, Box::new(detector), config)?,
        actuation: ActuationService::new(input, ActuationPolicy::default()),
    })
}

fn frame_source(args: &EngineArgs, sim: Option<&SimulatedDesktop>) -> Result<Box<dyn FrameSource>> {
    Ok(match args.source {
        SourceKind::Demo => {
            let s = ScriptedSource::new(demo_script());
            Box::new(if args.looping { s.looping() } else { s })
        }
        SourceKind::Sim => Box::new(sim.context("simulator not created")?.screen()),
        SourceKind::Images => {
            let dir = args
                .images
                .as_ref()
                .context("--source images requires --images <DIR>")?;
            Box::new(ImageSequenceSource::from_dir(dir)?)
        }
        SourceKind::Screen => screen_source()?,
    })
}

fn input_controller(
    kind: InputKind,
    sim: Option<&SimulatedDesktop>,
) -> Result<Box<dyn InputController>> {
    Ok(match (kind, sim) {
        (InputKind::Auto, Some(sim)) => Box::new(sim.input()),
        (InputKind::Auto | InputKind::DryRun, _) => {
            Box::new(DryRunInput::default().echo_to_stderr())
        }
        (InputKind::Native, Some(_)) => {
            bail!("--input native cannot be combined with --source sim")
        }
        (InputKind::Native, None) => native_input()?,
    })
}

#[cfg(feature = "screen")]
fn screen_source() -> Result<Box<dyn FrameSource>> {
    Ok(Box::new(ommatix_adapter_capture::ScreenSource::primary()?))
}

#[cfg(not(feature = "screen"))]
fn screen_source() -> Result<Box<dyn FrameSource>> {
    bail!("this build has no screen capture; rebuild with `--features screen`")
}

#[cfg(feature = "native-input")]
fn native_input() -> Result<Box<dyn InputController>> {
    Ok(Box::new(ommatix_adapter_input::NativeInput::new()?))
}

#[cfg(not(feature = "native-input"))]
fn native_input() -> Result<Box<dyn InputController>> {
    bail!("this build has no native input; rebuild with `--features native-input`")
}
