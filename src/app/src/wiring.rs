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
//! Accessibility evidence (`[models] accessibility = "uia"`, ADR-0017) reads the
//! foreground application's UI Automation tree on the monitor the capturer captures, so
//! it needs a live source (`dxgi`, or `screen`) and a Windows build with the `uia`
//! feature; with `images` it is refused (the tree would not match the frames) and the
//! simulated desktop ignores it. Otherwise the engine gets no accessibility evidence. It also
//! reads which element has keyboard focus, so with `--input sendinput` the engine delivers
//! `type` in two phases and sends the keys only once the target field has focus (ADR-0018);
//! without it `type` keeps the actuator's window-level focus check.
//!
//! Live input needs all of: `--input native` with `--source screen` (or `[engine] mode =
//! "native"`) and the `native-input` build, or `--input sendinput` with `--source dxgi`
//! and the Windows `sendinput` build; plus `[safety] actuation_enabled = true`. Even then
//! the engine only delivers an action the agent explicitly asks for with `dry_run: false`,
//! after its own gate, which checks the foreground application against `[safety]
//! allowed_applications`: only `sendinput` can attest it, so `native` never acts live.
//!
//! The operator's stop (`[safety] stop_file`) is read on every action, at the gate and
//! right before delivery; while live input is possible, [`Wired::active_control`] is the
//! indicator `serve` prints on stderr (chapter 08 §8.2).

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
    AccessibilityProvider, Clock, Engine, EngineConfig, FrameCapturer, IconResolverConfig,
    NoAccessibility, OsActuator, Ports, TextRecognizer, VisionDetector,
};

use crate::cli::{InputKind, RunArgs, SourceKind, ThemeKind};
use crate::clock::SystemClock;
use ommatix::policy::ConfigPolicy;

use crate::config::{AccessibilityKind, Config, EngineMode, OcrKind};

/// The driven adapters chosen for a source.
type Adapters = (
    Box<dyn FrameCapturer>,
    Box<dyn VisionDetector>,
    Box<dyn TextRecognizer>,
    Box<dyn AccessibilityProvider>,
    Box<dyn OsActuator>,
);

/// The capturer, the accessibility provider on its monitor and the actuator of a live
/// source.
type LivePorts = (
    Box<dyn FrameCapturer>,
    Box<dyn AccessibilityProvider>,
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
    /// The active-control indicator, for stderr: `Some` while the operator's policy lets
    /// a live actuator act.
    pub active_control: Option<String>,
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

/// Refuses accessibility evidence on a source it would not describe: the desktop's tree
/// matches live frames only (`dxgi`, `screen`). The simulated desktop ignores it, with a
/// note.
pub fn check_accessibility(source: SourceKind, config: &Config) -> Result<()> {
    if source == SourceKind::Images && config.models.accessibility == AccessibilityKind::Uia {
        bail!(
            "[models] accessibility = \"uia\" needs a live source (--source dxgi or screen): \
             the desktop's accessibility tree does not describe recorded images"
        );
    }
    // Refuse before any live source is opened (a headless host cannot open one).
    if matches!(source, SourceKind::Dxgi | SourceKind::Screen)
        && config.models.accessibility == AccessibilityKind::Uia
        && !cfg!(all(windows, feature = "uia"))
    {
        bail!(
            "[models] accessibility = \"uia\" but this build has no UI Automation; rebuild on \
             Windows with `--features uia`"
        );
    }
    Ok(())
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
    check_accessibility(args.source, config)?;
    let icons = config.icon_resolver()?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());

    let mut sim = None;
    let access = config.models.accessibility;
    let (capturer, detector, recognizer, accessibility, actuator): Adapters = match args.source {
        SourceKind::Sim => {
            let desktop = SimulatedDesktop::with_theme(theme(args.theme)).with_clock(clock.clone());
            let ports = (
                Box::new(desktop.capturer()) as Box<dyn FrameCapturer>,
                Box::new(desktop.detector()) as Box<dyn VisionDetector>,
                Box::new(desktop.recognizer()) as Box<dyn TextRecognizer>,
                Box::new(NoAccessibility) as Box<dyn AccessibilityProvider>,
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
            if access == AccessibilityKind::Uia {
                bail!(
                    "[models] accessibility = \"uia\" needs a live source (--source dxgi or \
                     screen): the desktop's accessibility tree does not describe recorded \
                     images"
                );
            }
            let (detector, recognizer) = perception(config)?;
            let none: Box<dyn AccessibilityProvider> = Box::new(NoAccessibility);
            (images(dir, &clock)?, detector, recognizer, none, dry_run())
        }
        SourceKind::Screen => {
            let (detector, recognizer) = perception(config)?;
            let (capturer, accessibility, actuator) = screen_ports(&clock, input, access)?;
            (capturer, detector, recognizer, accessibility, actuator)
        }
        SourceKind::Dxgi => {
            let (detector, recognizer) = perception(config)?;
            let (capturer, accessibility, actuator) =
                dxgi_ports(&clock, input, args.output, access)?;
            (capturer, detector, recognizer, accessibility, actuator)
        }
    };

    let live = actuator.is_live();
    let summary = format!(
        "source={} detector={} recognizer={} accessibility={} input={}{}",
        capturer.name(),
        detector.name(),
        recognizer.name(),
        accessibility.name(),
        actuator.name(),
        if live { " (live)" } else { " (never live)" },
    );
    let notes = startup_notes(args.source, config, icons.as_ref(), input, live);
    let active_control = (live && config.safety.actuation_enabled).then(|| active_control(config));

    let mut engine_config = EngineConfig::new(session_id);
    engine_config.plan = config.plan_params();
    engine_config.verify = config.verify_params();
    engine_config.concurrent_stages = config.engine.concurrent_stages;
    engine_config.icons = icons;
    let engine = Engine::new(
        engine_config,
        Ports {
            capturer,
            detector,
            recognizer,
            accessibility,
            actuator,
            policy: Arc::new(operator_policy(config)),
            clock,
        },
    );
    Ok(Wired {
        engine,
        sim,
        summary,
        notes,
        active_control,
    })
}

/// What the operator should know at startup, for stderr.
fn startup_notes(
    source: SourceKind,
    config: &Config,
    icons: Option<&IconResolverConfig>,
    input: InputKind,
    live: bool,
) -> Vec<String> {
    let access = config.models.accessibility;
    let mut notes = config.notes();
    notes.push(icon_note(icons));
    if source != SourceKind::Sim && config.models.has_detector() {
        let m = &config.models;
        notes.push(format!(
            "[models] detector: intra-op threads {}",
            if m.detector_threads == 0 {
                "ONNX Runtime default (physical cores)".to_owned()
            } else {
                m.detector_threads.to_string()
            }
        ));
    }
    if source == SourceKind::Sim
        && (config.models.has_detector()
            || config.models.ocr != OcrKind::None
            || access != AccessibilityKind::None)
    {
        notes.push(
            "[models] is not used with --source sim: the simulated desktop brings its own \
             detector and recogniser"
                .to_owned(),
        );
    }
    notes.extend(input_notes(config, input, live));
    notes
}

/// What the operator should know about live input, for stderr.
fn input_notes(config: &Config, input: InputKind, live: bool) -> Vec<String> {
    let mut notes = Vec::new();
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
    if live && config.safety.actuation_enabled && config.stop_file().is_none() {
        notes.push(
            "[safety] stop_file is not set: the only local stop is ending the process \
             (Ctrl+C)"
                .to_owned(),
        );
    }
    if live && config.safety.actuation_enabled {
        // explorer.exe also owns the desktop and the taskbar (ADR-0020).
        let scoped = &config.safety.allowed_window_classes;
        if config.safety.window_pin_file.is_empty()
            && config
                .safety
                .allowed_applications
                .iter()
                .any(|app| app == "explorer.exe" && !scoped.contains_key(app))
        {
            notes.push(
                "[safety] allowed_applications lists explorer.exe without allowed_window_classes: \
                 live input can then reach the desktop and the taskbar, which explorer.exe also \
                 owns (ADR-0020)"
                    .to_owned(),
            );
        }
    }
    notes
}

/// The operator's policy, stop file and window pin file, reporting the stop on stderr.
fn operator_policy(config: &Config) -> ConfigPolicy {
    let mut policy = ConfigPolicy::new(config.local_policy()).with_stderr_notices();
    if let Some(path) = config.stop_file() {
        policy = policy.with_stop_file(path);
    }
    match config.window_pin_file() {
        Some(path) => policy.with_window_pin_file(path),
        None => policy,
    }
}

/// The active-control indicator (chapter 08 §8.2): what may receive live input and how
/// the operator stops it.
fn active_control(config: &Config) -> String {
    let stop = match config.stop_file() {
        Some(path) => format!("create {} or press Ctrl+C", path.display()),
        None => "press Ctrl+C".to_owned(),
    };
    let mut scope: Vec<String> = config
        .safety
        .allowed_window_classes
        .iter()
        .map(|(app, classes)| format!(" (only windows of class {} of {app})", classes.join(", ")))
        .collect();
    if let Some(pin) = config.window_pin_file() {
        scope.push(format!(
            " (only the window whose handle {} holds)",
            pin.display()
        ));
    }
    let scope = scope.concat();
    format!(
        "ACTIVE CONTROL: live input is enabled for {}{scope}; the agent acts only with \
         dry_run: false; to stop, {stop}",
        config.safety.allowed_applications.join(", ")
    )
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
    use ommatix_adapter_vision::model::manifest::ExecutionProvider;

    use crate::config::ProviderKind;

    let m = &config.models;
    let (execution_provider, directml) = match m.detector_execution_provider {
        ProviderKind::Cpu => (ExecutionProvider::Cpu, None),
        ProviderKind::Directml => (
            ExecutionProvider::DirectMl,
            Some((
                config.resolve_path(&m.directml_path),
                m.directml_sha256.clone(),
            )),
        ),
    };
    let source = ommatix_adapter_vision::OnnxDetectorSource {
        manifest_path: config.resolve_path(&m.detector_manifest),
        manifest_sha256: m.detector_manifest_sha256.clone(),
        runtime_path: config.resolve_path(&m.onnxruntime_path),
        runtime_sha256: m.onnxruntime_sha256.clone(),
        execution_provider,
        directml,
        intra_threads: m.detector_threads as usize,
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
    access: AccessibilityKind,
) -> Result<LivePorts> {
    let screen = ommatix_adapter_capture::ScreenSource::primary(clock.clone())?;
    let actuator = match input {
        InputKind::DryRun => dry_run(),
        InputKind::Native => native_input(&screen)?,
        InputKind::Sendinput => bail!("sendinput input requires --source dxgi"),
    };
    let accessibility = accessibility(access, screen.mapping_source())?;
    Ok((Box::new(screen), accessibility, actuator))
}

#[cfg(not(feature = "screen"))]
fn screen_ports(_: &Arc<dyn Clock>, _: InputKind, _: AccessibilityKind) -> Result<LivePorts> {
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
    access: AccessibilityKind,
) -> Result<LivePorts> {
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
    let accessibility = accessibility(access, dxgi.mapping_source())?;
    Ok((Box::new(dxgi), accessibility, actuator))
}

#[cfg(not(all(windows, feature = "dxgi")))]
fn dxgi_ports(_: &Arc<dyn Clock>, _: InputKind, _: u32, _: AccessibilityKind) -> Result<LivePorts> {
    bail!("this build has no DXGI capture; rebuild on Windows with `--features dxgi`")
}

/// The capturer's live mapping from frame pixels to desktop coordinates.
#[cfg(any(feature = "screen", all(windows, feature = "dxgi")))]
trait MappingSource:
    Fn() -> Result<ommatix_domain::DisplayMapping, ommatix_application::PortError>
    + Send
    + Sync
    + 'static
{
}

#[cfg(any(feature = "screen", all(windows, feature = "dxgi")))]
impl<F> MappingSource for F where
    F: Fn() -> Result<ommatix_domain::DisplayMapping, ommatix_application::PortError>
        + Send
        + Sync
        + 'static
{
}

/// The accessibility provider `[models] accessibility` selects, reading the monitor
/// `mapping` describes.
#[cfg(any(feature = "screen", all(windows, feature = "dxgi")))]
fn accessibility(
    access: AccessibilityKind,
    mapping: impl MappingSource,
) -> Result<Box<dyn AccessibilityProvider>> {
    match access {
        AccessibilityKind::None => Ok(Box::new(NoAccessibility)),
        AccessibilityKind::Uia => uia(mapping),
    }
}

#[cfg(all(windows, feature = "uia", any(feature = "screen", feature = "dxgi")))]
fn uia(mapping: impl MappingSource) -> Result<Box<dyn AccessibilityProvider>> {
    use ommatix_adapter_accessibility::{Budget, UiaProvider};

    Ok(Box::new(
        UiaProvider::new(mapping, Budget::default())
            .context("[models] accessibility = \"uia\" is unavailable")?,
    ))
}

#[cfg(all(
    not(all(windows, feature = "uia")),
    any(feature = "screen", all(windows, feature = "dxgi"))
))]
fn uia(_: impl MappingSource) -> Result<Box<dyn AccessibilityProvider>> {
    bail!(
        "[models] accessibility = \"uia\" but this build has no UI Automation; rebuild on \
         Windows with `--features uia`"
    )
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
    fn accessibility_evidence_needs_a_live_source() {
        let uia = Config::parse(
            "[models]
accessibility = \"uia\"
",
        )
        .unwrap();
        assert!(
            error("serve --source images --images frames", &uia).contains("needs a live source")
        );
        assert!(check_accessibility(SourceKind::Images, &Config::default()).is_ok());
        // A live source takes it only when this build has UI Automation.
        let built = cfg!(all(windows, feature = "uia"));
        assert_eq!(check_accessibility(SourceKind::Dxgi, &uia).is_ok(), built);
        assert_eq!(check_accessibility(SourceKind::Screen, &uia).is_ok(), built);
        // The simulated desktop ignores it, with a note.
        let wired = build(&args("serve"), &uia).expect("the simulator ignores it");
        assert!(wired.summary.contains("accessibility=none"));
        assert!(
            wired
                .notes
                .iter()
                .any(|n| n.contains("not used with --source sim"))
        );
        // Without the Windows `uia` build, a live source refuses it before opening anything.
        if !built {
            let refusal = check_accessibility(SourceKind::Screen, &uia).unwrap_err();
            assert!(refusal.to_string().contains("--features uia"));
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
             recognizer=simulated-desktop accessibility=none input=simulated-desktop (never live)"
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
    fn the_active_control_indicator_names_the_applications_and_the_stop() {
        let mut config = allowlisted("\"notepad.exe\", \"calc.exe\"");
        assert_eq!(
            active_control(&config),
            "ACTIVE CONTROL: live input is enabled for notepad.exe, calc.exe; the agent acts \
             only with dry_run: false; to stop, press Ctrl+C"
        );
        config.safety.stop_file = "STOP".to_owned();
        assert!(active_control(&config).ends_with("to stop, create STOP or press Ctrl+C"));
        // Never shown while no live actuator exists, whatever the policy says.
        let wired = build(&args("serve"), &config).unwrap();
        assert_eq!(wired.active_control, None);
    }

    #[test]
    fn the_active_control_indicator_names_the_window_scope() {
        let config = Config::parse(
            "[safety]\nallowed_applications = [\"explorer.exe\"]\n\
             allowed_window_classes = { \"explorer.exe\" = [\"CabinetWClass\"] }\n\
             window_pin_file = \"window.pin\"\n",
        )
        .unwrap();
        let line = active_control(&config);
        assert!(
            line.starts_with(
                "ACTIVE CONTROL: live input is enabled for explorer.exe (only windows of class \
                 CabinetWClass of explorer.exe) (only the window whose handle window.pin holds); "
            ),
            "{line}"
        );
    }

    #[test]
    fn explorer_without_a_window_scope_is_flagged() {
        let note = |toml: &str| {
            let config = Config::parse(toml).unwrap();
            input_notes(&config, InputKind::Sendinput, true)
                .iter()
                .any(|n| n.contains("desktop and the taskbar"))
        };
        let live =
            "[safety]\nactuation_enabled = true\nallowed_applications = [\"explorer.exe\"]\n";
        assert!(note(live), "explorer.exe on its own is flagged");
        assert!(!note(&format!(
            "{live}allowed_window_classes = {{ \"explorer.exe\" = [\"CabinetWClass\"] }}\n"
        )));
        assert!(!note(&format!("{live}window_pin_file = \"window.pin\"\n")));
        assert!(!note(
            "[safety]\nactuation_enabled = true\nallowed_applications = [\"notepad.exe\"]\n"
        ));
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
