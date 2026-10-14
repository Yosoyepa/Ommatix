//! Composition root: the only place that chooses concrete adapters and injects them into
//! the engine.
//!
//! | `--source` | capturer | detector | recogniser | `--input dry-run` | `--input native` |
//! |---|---|---|---|---|---|
//! | `sim` | simulated desktop | simulated desktop | simulated desktop | simulated desktop (never live) | refused |
//! | `images` | PNG sequence | flat-region | none | dry-run recorder | refused |
//! | `screen` | primary monitor | flat-region | none | dry-run recorder | `enigo`, if `[safety] actuation_enabled` |
//!
//! Live input needs all of: `--input native` (or `[engine] mode = "native"`), the
//! `native-input` build, `--source screen` and `[safety] actuation_enabled = true`. Even
//! then the engine only delivers an action the agent explicitly asks for with
//! `dry_run: false`, after its own gate.

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use ommatix_adapter_capture::ImageSequenceSource;
use ommatix_adapter_input::DryRunInput;
use ommatix_adapter_sim::{SimulatedDesktop, Theme};
use ommatix_adapter_vision::{FlatRegionDetector, NullTextRecognizer};
use ommatix_application::contract::limits;
use ommatix_application::{
    Clock, Engine, EngineConfig, FrameCapturer, IconResolverConfig, OsActuator, Ports,
    TextRecognizer, VisionDetector,
};

use crate::cli::{InputKind, RunArgs, SourceKind, ThemeKind};
use crate::clock::SystemClock;
use crate::config::{Config, EngineMode};
use crate::policy::ConfigPolicy;

/// The four driven adapters chosen for a source.
type Adapters = (
    Box<dyn FrameCapturer>,
    Box<dyn VisionDetector>,
    Box<dyn TextRecognizer>,
    Box<dyn OsActuator>,
);

/// The engine and what the front ends need to know about it.
pub struct Wired {
    pub engine: Engine,
    /// The simulated desktop, when it is the source: the demo plays its user.
    pub sim: Option<SimulatedDesktop>,
    /// `source=… detector=… recognizer=… input=…`, for diagnostics.
    pub summary: String,
    /// Configured values the engine does not enforce, for stderr.
    pub notes: Vec<String>,
}

/// The session identifier: `--session-id`, or one derived from the start time and the
/// process id, unique per process.
pub fn session_id(requested: Option<&str>) -> Result<String> {
    let Some(id) = requested else {
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        return Ok(format!(
            "ommatix-{:x}-{:x}",
            since_epoch.as_nanos(),
            std::process::id()
        ));
    };
    let valid =
        (1..=limits::MAX_ID_CHARS).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_graphic());
    if !valid {
        bail!("--session-id must be 1 to 64 visible ASCII characters (no spaces)");
    }
    Ok(id.to_owned())
}

/// The input adapter: `--input`, else `[engine] mode`.
pub fn input_kind(args: &RunArgs, config: &Config) -> InputKind {
    args.input.unwrap_or(match config.engine.mode {
        EngineMode::DryRun => InputKind::DryRun,
        EngineMode::Native => InputKind::Native,
    })
}

/// Refuses live input unless every condition holds. Checked before any adapter opens.
pub fn check_input(source: SourceKind, input: InputKind, config: &Config) -> Result<()> {
    if input == InputKind::DryRun {
        return Ok(());
    }
    if source != SourceKind::Screen {
        bail!(
            "native input requires --source screen: input must land on the screen the agent \
             observes"
        );
    }
    if !config.safety.actuation_enabled {
        bail!(
            "native input requires `[safety] actuation_enabled = true` in the --config file; \
             the agent can never enable actuation"
        );
    }
    if !cfg!(feature = "native-input") {
        bail!("this build has no native input; rebuild with `--features native-input`");
    }
    Ok(())
}

/// Builds the engine for `args` under `config`.
pub fn build(args: &RunArgs, config: &Config) -> Result<Wired> {
    let session_id = session_id(args.session_id.as_deref())?;
    let input = input_kind(args, config);
    check_input(args.source, input, config)?;
    let icons = config.icon_resolver()?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());

    let mut sim = None;
    let (capturer, detector, recognizer, actuator): Adapters = match args.source {
        SourceKind::Sim => {
            let desktop = SimulatedDesktop::with_theme(theme(args.theme)).with_clock(clock.clone());
            let ports = (
                Box::new(desktop.capturer()) as Box<dyn FrameCapturer>,
                Box::new(desktop.detector()) as Box<dyn VisionDetector>,
                Box::new(desktop.recognizer()) as Box<dyn TextRecognizer>,
                Box::new(desktop.actuator()) as Box<dyn OsActuator>,
            );
            sim = Some(desktop);
            ports
        }
        SourceKind::Images => {
            let dir = args
                .images
                .as_deref()
                .context("--source images requires --images <DIR>")?;
            (
                images(dir, &clock)?,
                Box::new(FlatRegionDetector::default()),
                Box::new(NullTextRecognizer),
                dry_run(),
            )
        }
        SourceKind::Screen => {
            let (capturer, actuator) = screen_ports(&clock, input)?;
            (
                capturer,
                Box::new(FlatRegionDetector::default()),
                Box::new(NullTextRecognizer),
                actuator,
            )
        }
    };

    let live = actuator.is_live();
    let summary = format!(
        "source={} detector={} recognizer={} input={}{}",
        capturer.name(),
        detector.name(),
        recognizer.name(),
        actuator.name(),
        if live { " (live)" } else { " (never live)" },
    );
    let mut notes = config.notes();
    notes.push(icon_note(icons.as_ref()));
    if live && config.safety.actuation_enabled {
        notes.push(
            "[safety] live input is denied until the input adapter can attest that the \
             foreground application is in allowed_applications (none can before S3)"
                .to_owned(),
        );
    }
    if config.safety.actuation_enabled && !live {
        notes.push(
            "[safety] actuation_enabled = true has no effect: the input adapter is not live"
                .to_owned(),
        );
    }

    let mut engine_config = EngineConfig::new(session_id);
    engine_config.plan = config.plan_params();
    engine_config.icons = icons;
    let engine = Engine::new(
        engine_config,
        Ports {
            capturer,
            detector,
            recognizer,
            actuator,
            policy: Arc::new(ConfigPolicy::new(config.local_policy())),
            clock,
        },
    );
    Ok(Wired {
        engine,
        sim,
        summary,
        notes,
    })
}

/// What Algorithm 3A will do, for stderr.
fn icon_note(icons: Option<&IconResolverConfig>) -> String {
    let Some(icons) = icons else {
        return "[cache] no curated icon prototypes are configured: icon labels stay unknown"
            .to_owned();
    };
    let total = icons.cache.prototypes().len();
    let in_context = icons
        .cache
        .prototypes()
        .iter()
        .filter(|p| p.context == icons.context)
        .count();
    format!(
        "[cache] {total} curated icon prototypes, {in_context} for context {:?}; the          thresholds are the operator's, not calibrated by this build",
        icons.context
    )
}

fn theme(kind: ThemeKind) -> Theme {
    match kind {
        ThemeKind::Light => Theme::Light,
        ThemeKind::Dark => Theme::Dark,
        ThemeKind::ContrastingWallpaper => Theme::ContrastingWallpaper,
    }
}

fn images(dir: &Path, clock: &Arc<dyn Clock>) -> Result<Box<dyn FrameCapturer>> {
    Ok(Box::new(ImageSequenceSource::from_dir(dir, clock.clone())?))
}

/// Records actions on stderr (never their text) and submits nothing.
fn dry_run() -> Box<dyn OsActuator> {
    Box::new(DryRunInput::default().echo_to_stderr())
}

#[cfg(feature = "screen")]
fn screen_ports(
    clock: &Arc<dyn Clock>,
    input: InputKind,
) -> Result<(Box<dyn FrameCapturer>, Box<dyn OsActuator>)> {
    let screen = ommatix_adapter_capture::ScreenSource::primary(clock.clone())?;
    let actuator = match input {
        InputKind::DryRun => dry_run(),
        InputKind::Native => native_input(&screen)?,
    };
    Ok((Box::new(screen), actuator))
}

#[cfg(not(feature = "screen"))]
fn screen_ports(
    _: &Arc<dyn Clock>,
    _: InputKind,
) -> Result<(Box<dyn FrameCapturer>, Box<dyn OsActuator>)> {
    bail!("this build has no screen capture; rebuild with `--features screen`")
}

/// Native input aimed through the screen's live mapping, which fails closed when the
/// screen changes size.
#[cfg(feature = "native-input")]
fn native_input(screen: &ommatix_adapter_capture::ScreenSource) -> Result<Box<dyn OsActuator>> {
    Ok(Box::new(
        ommatix_adapter_input::NativeInput::with_mapping_source(screen.mapping_source())?,
    ))
}

#[cfg(all(feature = "screen", not(feature = "native-input")))]
fn native_input(_: &ommatix_adapter_capture::ScreenSource) -> Result<Box<dyn OsActuator>> {
    bail!("this build has no native input; rebuild with `--features native-input`")
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use ommatix_application::contract::ObserveRequest;

    use super::*;
    use crate::cli::{Cli, Command};

    fn args(line: &str) -> RunArgs {
        let argv = std::iter::once("ommatix").chain(line.split_whitespace());
        match Cli::parse_from(argv).command {
            Command::Serve(a) => a,
            Command::Demo(d) => d.run,
        }
    }

    fn error(line: &str, config: &Config) -> String {
        format!(
            "{:#}",
            build(&args(line), config).err().expect("the build fails")
        )
    }

    fn actuation_enabled() -> Config {
        Config::parse("[safety]\nactuation_enabled = true\n").unwrap()
    }

    #[test]
    fn native_input_needs_the_screen_source_and_the_operators_consent() {
        let default = Config::default();
        for line in [
            "serve --input native",
            "serve --source images --input native",
        ] {
            assert!(
                error(line, &default).contains("requires --source screen"),
                "{line}"
            );
            assert!(
                error(line, &actuation_enabled()).contains("requires --source screen"),
                "{line}"
            );
        }
        assert!(
            error("serve --source screen --input native", &default)
                .contains("actuation_enabled = true")
        );
        // `[engine] mode = "native"` is held to the same rules.
        let native_mode = Config::parse("[engine]\nmode = \"native\"\n").unwrap();
        assert!(error("serve", &native_mode).contains("requires --source screen"));
        // And `--input dry-run` always wins over it.
        assert!(build(&args("serve --input dry-run"), &native_mode).is_ok());
    }

    #[test]
    fn the_simulator_drives_every_port_by_default() {
        let mut wired = build(
            &args("serve --session-id s-1 --theme dark"),
            &Config::default(),
        )
        .expect("the default build works");
        assert_eq!(
            wired.summary,
            "source=simulated-desktop detector=simulated-desktop \
             recognizer=simulated-desktop input=simulated-desktop (never live)"
        );
        assert!(wired.sim.is_some());
        assert_eq!(wired.engine.session_id(), "s-1");
        let page = wired
            .engine
            .observe_screen(&ObserveRequest::default())
            .expect("the simulator observes");
        assert_eq!(page.total_elements, 3);
        assert!(
            wired
                .notes
                .iter()
                .any(|n| n.contains("max_snapshot_age_ms"))
        );
        assert!(
            wired
                .notes
                .iter()
                .any(|n| n.contains("no curated icon prototypes"))
        );
    }

    #[test]
    fn dry_run_with_actuation_enabled_says_it_has_no_effect() {
        let wired = build(&args("serve"), &actuation_enabled()).unwrap();
        assert!(wired.notes.iter().any(|n| n.contains("has no effect")));
    }

    #[test]
    fn the_image_source_needs_a_directory() {
        assert!(error("serve --source images", &Config::default()).contains("--images"));
        assert!(
            error(
                "serve --source images --images no/such/dir",
                &Config::default()
            )
            .contains("cannot read")
        );
    }

    #[test]
    fn session_ids_are_validated_or_unique_per_process() {
        for bad in ["", "has space", &"x".repeat(65), "tab\t"] {
            assert!(session_id(Some(bad)).is_err(), "{bad:?}");
        }
        assert_eq!(session_id(Some("demo")).unwrap(), "demo");
        let generated = session_id(None).unwrap();
        assert!(generated.starts_with("ommatix-") && generated.len() <= limits::MAX_ID_CHARS);
        assert!(session_id(Some(&generated)).is_ok());
    }
}
