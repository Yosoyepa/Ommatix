//! Composition root: the only place that chooses concrete adapters and injects them into
//! the engine.
//!
//! | `--source` | capturer | detector | recogniser | `--input dry-run` | live input |
//! |---|---|---|---|---|---|
//! | `sim` | simulated desktop | simulated desktop | simulated desktop | simulated desktop (never live) | refused |
//! | `images` | PNG sequence | `[models]` detector, else flat-region | `[models] ocr` | dry-run recorder | refused |
//! | `screen` | primary monitor (`xcap`) | `[models]` detector, else flat-region | `[models] ocr` | dry-run recorder | `--input native` (`enigo`) |
//! | `dxgi` | one output (DXGI Desktop Duplication, Windows) | `[models]` detector, else flat-region | `[models] ocr` | dry-run recorder | `--input sendinput` (Win32 `SendInput`) |
//!
//! Models fail closed: a configured detector or OCR that this build lacks, or whose files
//! do not match their pinned SHA-256, stops the engine from starting; nothing falls back
//! to another model or language. Without `[models]`, no model is loaded
//! (`models_loaded: false`) and roles stay `unknown`.
//!
//! Live input needs all of: `--input native` with `--source screen` (or `[engine] mode =
//! "native"`) and the `native-input` build, or `--input sendinput` with `--source dxgi`
//! and the Windows `sendinput` build; plus `[safety] actuation_enabled = true`. Even then
//! the engine only delivers an action the agent explicitly asks for with `dry_run: false`,
//! after its own gate, which checks the foreground application against `[safety]
//! allowed_applications`: only `sendinput` can attest it, so `native` never acts live.

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
use crate::config::{Config, EngineMode, OcrKind};
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
    match input {
        InputKind::DryRun => Ok(()),
        InputKind::Native => check_native(source, config),
        InputKind::Sendinput => check_sendinput(source, config),
    }
}

fn check_native(source: SourceKind, config: &Config) -> Result<()> {
    if source != SourceKind::Screen {
        bail!(
            "native input requires --source screen: input must land on the screen the agent \
             observes"
        );
    }
    check_consent("native", config)?;
    if !cfg!(feature = "native-input") {
        bail!("this build has no native input; rebuild with `--features native-input`");
    }
    Ok(())
}

fn check_sendinput(source: SourceKind, config: &Config) -> Result<()> {
    if source != SourceKind::Dxgi {
        bail!(
            "sendinput input requires --source dxgi: input must land on the screen the agent \
             observes"
        );
    }
    check_consent("sendinput", config)?;
    let apps = &config.safety.allowed_applications;
    if apps.is_empty() {
        bail!(
            "sendinput input requires a non-empty `[safety] allowed_applications`: the engine \
             only acts on a foreground application listed there"
        );
    }
    if let Some(app) = apps.iter().find(|a| **a != a.trim().to_lowercase()) {
        bail!(
            "[safety] allowed_applications entry {app:?} would never match: list lowercase \
             executable file names such as \"notepad.exe\""
        );
    }
    if !cfg!(all(windows, feature = "sendinput")) {
        bail!("this build has no SendInput; rebuild on Windows with `--features sendinput`");
    }
    Ok(())
}

fn check_consent(input: &str, config: &Config) -> Result<()> {
    if !config.safety.actuation_enabled {
        bail!(
            "{input} input requires `[safety] actuation_enabled = true` in the --config file; \
             the agent can never enable actuation"
        );
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
            let (detector, recognizer) = perception(config)?;
            (images(dir, &clock)?, detector, recognizer, dry_run())
        }
        SourceKind::Screen => {
            let (detector, recognizer) = perception(config)?;
            let (capturer, actuator) = screen_ports(&clock, input)?;
            (capturer, detector, recognizer, actuator)
        }
        SourceKind::Dxgi => {
            let (detector, recognizer) = perception(config)?;
            let (capturer, actuator) = dxgi_ports(&clock, input, args.output)?;
            (capturer, detector, recognizer, actuator)
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
    if args.source == SourceKind::Sim
        && (config.models.has_detector() || config.models.ocr != OcrKind::None)
    {
        notes.push(
            "[models] is not used with --source sim: the simulated desktop brings its own \
             detector and recogniser"
                .to_owned(),
        );
    }
    if live && input == InputKind::Native {
        notes.push(
            "--input native cannot attest the foreground application, so the engine refuses \
             every live action against [safety] allowed_applications; on Windows use \
             --source dxgi --input sendinput"
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
        "[cache] {total} curated icon prototypes, {in_context} for context {:?}; the \
         thresholds are the operator's, not calibrated by this build",
        icons.context
    )
}

/// The detector and recogniser `[models]` selects for a real source, loaded and verified
/// before any capture opens.
fn perception(config: &Config) -> Result<(Box<dyn VisionDetector>, Box<dyn TextRecognizer>)> {
    Ok((detector(config)?, recognizer(config)?))
}

fn detector(config: &Config) -> Result<Box<dyn VisionDetector>> {
    if !config.models.has_detector() {
        return Ok(Box::new(FlatRegionDetector::default()));
    }
    onnx_detector(config)
}

#[cfg(feature = "onnx")]
fn onnx_detector(config: &Config) -> Result<Box<dyn VisionDetector>> {
    let m = &config.models;
    let source = ommatix_adapter_vision::OnnxDetectorSource {
        manifest_path: config.resolve_path(&m.detector_manifest),
        manifest_sha256: m.detector_manifest_sha256.clone(),
        runtime_path: config.resolve_path(&m.onnxruntime_path),
        runtime_sha256: m.onnxruntime_sha256.clone(),
    };
    Ok(Box::new(
        ommatix_adapter_vision::OnnxVisionDetector::load(&source)
            .context("[models] the detector was refused")?,
    ))
}

#[cfg(not(feature = "onnx"))]
fn onnx_detector(_: &Config) -> Result<Box<dyn VisionDetector>> {
    bail!(
        "[models] detector_manifest is set but this build has no ONNX detector; rebuild with \
         `--features onnx`"
    )
}

fn recognizer(config: &Config) -> Result<Box<dyn TextRecognizer>> {
    match config.models.ocr {
        OcrKind::None => Ok(Box::new(NullTextRecognizer)),
        OcrKind::Winrt => winrt_recognizer(&config.models.ocr_language),
    }
}

#[cfg(all(windows, feature = "winrt-ocr"))]
fn winrt_recognizer(language: &str) -> Result<Box<dyn TextRecognizer>> {
    Ok(Box::new(
        ommatix_adapter_vision::WinRtTextRecognizer::new(language)
            .context("[models] ocr = \"winrt\" is unavailable")?,
    ))
}

#[cfg(not(all(windows, feature = "winrt-ocr")))]
fn winrt_recognizer(_: &str) -> Result<Box<dyn TextRecognizer>> {
    bail!(
        "[models] ocr = \"winrt\" but this build has no Windows OCR; rebuild on Windows with \
         `--features winrt-ocr`"
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
        InputKind::Sendinput => bail!("sendinput input requires --source dxgi"),
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

#[cfg(all(windows, feature = "dxgi"))]
fn dxgi_ports(
    clock: &Arc<dyn Clock>,
    input: InputKind,
    output: u32,
) -> Result<(Box<dyn FrameCapturer>, Box<dyn OsActuator>)> {
    use ommatix_adapter_capture::{DxgiCapturer, DxgiOptions};

    let options = DxgiOptions {
        output_index: output,
        ..DxgiOptions::default()
    };
    let dxgi = DxgiCapturer::open(options, clock.clone())
        .with_context(|| format!("cannot capture output {output} through DXGI"))?;
    let actuator = match input {
        InputKind::DryRun => dry_run(),
        InputKind::Sendinput => send_input(&dxgi)?,
        InputKind::Native => bail!("native input requires --source screen"),
    };
    Ok((Box::new(dxgi), actuator))
}

#[cfg(not(all(windows, feature = "dxgi")))]
fn dxgi_ports(
    _: &Arc<dyn Clock>,
    _: InputKind,
    _: u32,
) -> Result<(Box<dyn FrameCapturer>, Box<dyn OsActuator>)> {
    bail!("this build has no DXGI capture; rebuild on Windows with `--features dxgi`")
}

/// `SendInput` aimed through the DXGI capturer's live mapping, which fails closed while
/// the capture is lost or out of profile.
#[cfg(all(windows, feature = "sendinput"))]
fn send_input(dxgi: &ommatix_adapter_capture::DxgiCapturer) -> Result<Box<dyn OsActuator>> {
    Ok(Box::new(
        ommatix_adapter_input::SendInputActuator::with_mapping_source(dxgi.mapping_source())?,
    ))
}

#[cfg(all(windows, feature = "dxgi", not(feature = "sendinput")))]
fn send_input(_: &ommatix_adapter_capture::DxgiCapturer) -> Result<Box<dyn OsActuator>> {
    bail!("this build has no SendInput; rebuild on Windows with `--features sendinput`")
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

    fn allowlisted(apps: &str) -> Config {
        Config::parse(&format!(
            "[safety]\nactuation_enabled = true\nallowed_applications = [{apps}]\n"
        ))
        .unwrap()
    }

    fn check(source: SourceKind, input: InputKind, config: &Config) -> Result<(), String> {
        check_input(source, input, config).map_err(|e| format!("{e:#}"))
    }

    #[test]
    fn sendinput_needs_dxgi_consent_and_an_allowlist() {
        let ready = allowlisted("\"notepad.exe\"");
        for line in [
            "serve --input sendinput",
            "serve --source screen --input sendinput",
            "serve --source images --input sendinput",
        ] {
            assert!(
                error(line, &ready).contains("requires --source dxgi"),
                "{line}"
            );
        }
        let sendinput = |config: &Config| check(SourceKind::Dxgi, InputKind::Sendinput, config);
        assert!(
            sendinput(&Config::default())
                .unwrap_err()
                .contains("actuation_enabled = true")
        );
        assert!(
            sendinput(&actuation_enabled())
                .unwrap_err()
                .contains("non-empty `[safety] allowed_applications`")
        );
        assert!(
            sendinput(&allowlisted("\"Notepad.exe\""))
                .unwrap_err()
                .contains("would never match")
        );
        if cfg!(all(windows, feature = "sendinput")) {
            assert_eq!(sendinput(&ready), Ok(()));
        } else {
            assert!(
                sendinput(&ready)
                    .unwrap_err()
                    .contains("--features sendinput")
            );
        }
    }

    #[test]
    fn native_input_never_pairs_with_dxgi() {
        assert!(
            check(SourceKind::Dxgi, InputKind::Native, &actuation_enabled())
                .unwrap_err()
                .contains("requires --source screen")
        );
        if !cfg!(all(windows, feature = "dxgi")) {
            assert!(error("serve --source dxgi", &Config::default()).contains("--features dxgi"));
        }
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

    const DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn with_detector() -> Config {
        Config::parse(&format!(
            "[models]\ndetector_manifest = \"no/such/manifest.json\"\n\
             detector_manifest_sha256 = \"{DIGEST}\"\n\
             onnxruntime_path = \"no/such/onnxruntime.dll\"\nonnxruntime_sha256 = \"{DIGEST}\"\n"
        ))
        .unwrap()
    }

    #[test]
    fn a_configured_detector_that_cannot_be_verified_stops_the_engine() {
        // Refused before any capture opens (the image directory, the screen or the DXGI
        // output), and never replaced by the flat-region detector.
        let expected = if cfg!(feature = "onnx") {
            "the detector was refused"
        } else {
            "--features onnx"
        };
        for line in [
            "serve --source images --images no/such/dir",
            "serve --source screen",
            "serve --source dxgi --output 7",
        ] {
            let message = error(line, &with_detector());
            assert!(message.contains(expected), "{line}: {message}");
        }
    }

    #[test]
    fn a_configured_ocr_that_is_unavailable_stops_the_engine() {
        // A language that is never installed, so the outcome is the same on every build.
        let config = Config::parse("[models]\nocr = \"winrt\"\nocr_language = \"tlh\"\n").unwrap();
        let expected = if cfg!(all(windows, feature = "winrt-ocr")) {
            "unavailable"
        } else {
            "--features winrt-ocr"
        };
        for line in [
            "serve --source images --images no/such/dir",
            "serve --source dxgi --output 7",
        ] {
            let message = error(line, &config);
            assert!(message.contains(expected), "{line}: {message}");
        }
    }

    #[test]
    fn the_simulator_ignores_models_and_says_so() {
        let wired = build(&args("serve"), &with_detector()).unwrap();
        assert!(!wired.engine.engine_metrics().models_loaded);
        assert!(
            wired
                .notes
                .iter()
                .any(|n| n.contains("[models] is not used"))
        );
        // Notes are wrapped in source with `\` continuations, never with raw spaces.
        for note in &wired.notes {
            assert!(!note.contains("  "), "{note:?}");
        }
    }

    #[test]
    fn the_icon_note_reads_as_one_sentence() {
        use ommatix_domain::appearance::{CachePolicy, IconCache};

        let icons = IconResolverConfig {
            cache: IconCache::new(),
            policy: CachePolicy::new(0.1, 0.1).unwrap(),
            tau: 3,
            context: "notepad.exe".to_owned(),
        };
        for note in [icon_note(None), icon_note(Some(&icons))] {
            assert!(!note.contains("  "), "{note:?}");
        }
        assert_eq!(
            icon_note(Some(&icons)),
            "[cache] 0 curated icon prototypes, 0 for context \"notepad.exe\"; the thresholds \
             are the operator's, not calibrated by this build"
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
