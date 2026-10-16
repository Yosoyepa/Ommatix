//! Operator-only M1 composition: DXGI plus a reader that cannot deliver input.

use anyhow::Result;

use crate::cli::FreshnessProbeArgs;

#[cfg(not(all(windows, feature = "sendinput")))]
pub fn run(_args: &FreshnessProbeArgs) -> Result<()> {
    anyhow::bail!("freshness-probe needs Windows and --features sendinput (window reads only)")
}

#[cfg(all(windows, feature = "sendinput"))]
pub fn run(args: &FreshnessProbeArgs) -> Result<()> {
    use std::sync::Arc;
    use std::time::Duration;

    use anyhow::{Context, ensure};
    use ommatix_adapter_capture::{DxgiCapturer, DxgiOptions};
    use ommatix_adapter_input::WindowReader;
    use ommatix_application::{Clock, FrameCapturer, FreshnessProbeGroup, PolicyStore};
    use serde_json::json;

    use crate::clock::SystemClock;
    use crate::config::Config;

    let config = Config::load(Some(&args.config))?;
    let target = target_box(args, &config)?;
    let mut policy = ommatix::policy::ConfigPolicy::new(config.local_policy());
    if let Some(path) = config.stop_file() {
        policy = policy.with_stop_file(path);
    }
    if let Some(path) = config.window_pin_file() {
        policy = policy.with_window_pin_file(path);
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
    let mut capture = DxgiCapturer::open(
        DxgiOptions {
            output_index: args.output,
            ..DxgiOptions::default()
        },
        clock.clone(),
    )
    .map_err(|_| anyhow::anyhow!("M1 capture unavailable"))?;
    let reader = WindowReader::with_mapping_source(capture.mapping_source())
        .map_err(|_| anyhow::anyhow!("M1 window reader unavailable"))?;
    check_scope(&reader, &policy.current(), &args.window_class, None)?;
    // Initialise the capture's mapping, discarding this frame. Nothing is persisted.
    let _ = capture
        .capture()
        .map_err(|_| anyhow::anyhow!("M1 capture unavailable"))?;
    let mut records = Vec::new();
    let mut outside = 0;
    for _ in 0..args.pairs {
        let mut read = || -> Result<_> {
            check_scope(&reader, &policy.current(), &args.window_class, Some(target))?;
            let group = FreshnessProbeGroup::read(&reader, target)
                .context("M1 window group cannot be attested")?;
            let frame = capture
                .capture()
                .map_err(|_| anyhow::anyhow!("M1 capture unavailable"))?;
            ensure!(
                FreshnessProbeGroup::read(&reader, target).as_ref() == Some(&group),
                "M1 window group changed during capture"
            );
            check_scope(&reader, &policy.current(), &args.window_class, Some(target))?;
            Ok((group, frame))
        };
        let (first, base) = read()?;
        clock.sleep(Duration::from_millis(args.interval_ms));
        let (second, frame) = read()?;
        ensure!(first == second, "M1 window group changed between frames");
        let counts = first
            .measure(
                &base.frame,
                &frame.frame,
                config.engine.action_freshness_margin_px,
            )
            .context("M1 geometry or target changed; no valid pair")?;
        outside += u32::from(counts.total_tiles > counts.group_tiles);
        records.push(json!({"capture_gap_ms": frame.captured_at.saturating_sub(base.captured_at).as_secs_f64() * 1000.0,
            "total_tiles": counts.total_tiles, "group_tiles": counts.group_tiles,
            "touches_target": counts.touches_target, "group_window_count": counts.group_window_count,
            "group_frame_fraction": counts.group_frame_fraction,
            "group_covers_whole_frame": counts.group_covers_whole_frame}));
    }
    let mut report = json!({"pairs": records, "interval_ms": args.interval_ms,
        "margin_px": config.engine.action_freshness_margin_px, "outside_pairs": outside,
        "descriptive": args.descriptive});
    if let Some(minimum) = args.min_outside_pairs {
        report["min_outside_pairs"] = json!(minimum);
        report["criterion_met"] = json!(outside >= minimum);
    }
    // The product writes no files (SEC-04). The operator-side Python driver validates
    // these counts and persists them outside the checkout; stdout remains untouched.
    serde_json::to_writer(std::io::stderr().lock(), &report)
        .context("cannot write the measurement counts")
}

#[cfg(any(test, all(windows, feature = "sendinput")))]
fn check_scope(
    reader: &dyn ommatix_application::OsActuator,
    policy: &ommatix_application::LocalPolicy,
    class: &str,
    target: Option<ommatix_domain::bbox::BBox>,
) -> Result<()> {
    use anyhow::{Context, ensure};
    let application = reader
        .foreground_application()
        .context("M1 foreground unavailable")?;
    let foreground = reader
        .foreground_window()
        .context("M1 foreground window unavailable")?;
    ensure!(
        !policy.actuation_enabled && !policy.stopped,
        "M1 policy refuses the measurement"
    );
    ensure!(
        policy.allowed_applications.contains(&application)
            && application != "applicationframehost.exe"
            && policy
                .window_classes
                .get(&application)
                .is_some_and(|cs| cs.iter().any(|c| c == class))
            && foreground.class == class,
        "M1 foreground is outside the operator's scope"
    );
    if let Some(target) = target {
        ensure!(
            ommatix_application::window_scope_problem(
                policy,
                &application,
                Some(&foreground),
                reader.window_at(target.center()).as_ref()
            )
            .is_none(),
            "M1 target is outside the operator's scope"
        );
    }
    Ok(())
}

#[cfg(any(test, all(windows, feature = "sendinput")))]
fn target_box(
    args: &FreshnessProbeArgs,
    config: &crate::config::Config,
) -> Result<ommatix_domain::bbox::BBox> {
    use anyhow::ensure;
    ensure!(
        !config.safety.actuation_enabled && config.engine.mode == crate::config::EngineMode::DryRun,
        "M1 requires actuation_enabled = false and mode = dry_run"
    );
    ensure!(
        args.pairs > 0
            && args
                .min_outside_pairs
                .is_none_or(|minimum| minimum <= args.pairs),
        "the criterion exceeds the number of pairs or there are no pairs"
    );
    Ok(ommatix_domain::bbox::BBox::new(
        f64::from(args.target_x),
        f64::from(args.target_y),
        f64::from(args.target_x) + f64::from(args.target_width),
        f64::from(args.target_y) + f64::from(args.target_height),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command};
    use crate::config::Config;
    use clap::Parser;
    use ommatix_application::{LocalPolicy, OsActuator, TopLevelWindow, WindowPin};

    fn argv() -> Vec<&'static str> {
        vec![
            "ommatix",
            "freshness-probe",
            "--config",
            "operator.toml",
            "--pairs",
            "2",
            "--min-outside-pairs",
            "1",
            "--window-class",
            "TestClass",
            "--target-x",
            "10",
            "--target-y",
            "10",
            "--target-width",
            "4",
            "--target-height",
            "4",
        ]
    }
    fn args() -> FreshnessProbeArgs {
        let Command::FreshnessProbe(args) = Cli::parse_from(argv()).command else {
            panic!("probe args")
        };
        args
    }
    #[test]
    fn the_owner_must_choose_descriptive_or_a_measurement_criterion() {
        let mut missing = argv();
        missing.drain(6..8);
        assert!(Cli::try_parse_from(missing).is_err());
        assert_eq!(args().interval_ms, 150);
    }
    #[test]
    fn descriptive_measurement_has_no_default_pass_threshold() {
        let mut descriptive = argv();
        descriptive.drain(6..8);
        descriptive.push("--descriptive");
        let Command::FreshnessProbe(args) = Cli::parse_from(descriptive.clone()).command else {
            panic!("probe args")
        };
        assert!(args.descriptive);
        assert_eq!(args.min_outside_pairs, None);
        descriptive.extend(["--min-outside-pairs", "0"]);
        assert!(Cli::try_parse_from(descriptive).is_err());
    }
    #[test]
    fn an_enabled_or_native_configuration_is_refused_before_any_capture() {
        let args = args();
        let mut config = Config::default();
        assert!(target_box(&args, &config).is_ok());
        config.safety.actuation_enabled = true;
        assert!(target_box(&args, &config).is_err());
        config.safety.actuation_enabled = false;
        config.engine.mode = crate::config::EngineMode::Native;
        assert!(target_box(&args, &config).is_err());
    }
    struct Reader;
    impl OsActuator for Reader {
        fn name(&self) -> &'static str {
            "test"
        }
        fn is_live(&self) -> bool {
            false
        }
        fn foreground_application(&self) -> Option<String> {
            Some("test.exe".into())
        }
        fn foreground_window(&self) -> Option<TopLevelWindow> {
            Some(TopLevelWindow {
                handle: 1,
                class: "TestClass".into(),
                z_order: 0,
                rectangle: None,
            })
        }
        fn window_at(&self, _: (f64, f64)) -> Option<TopLevelWindow> {
            self.foreground_window()
        }
        fn deliver(
            &mut self,
            _: &ommatix_application::contract::ValidatedAction,
        ) -> Result<ommatix_application::DeliveryReceipt, ommatix_application::PortError> {
            panic!("no input in a measurement")
        }
    }
    #[test]
    fn the_probe_requires_the_operator_class_application_stop_and_pin_scope() {
        let mut policy = LocalPolicy {
            allowed_applications: vec!["test.exe".into()],
            window_classes: [("test.exe".into(), vec!["TestClass".into()])].into(),
            ..LocalPolicy::default()
        };
        let target = target_box(&args(), &Config::default()).unwrap();
        assert!(check_scope(&Reader, &policy, "TestClass", Some(target)).is_ok());
        assert!(check_scope(&Reader, &policy, "OtherClass", Some(target)).is_err());
        policy.window_pin = Some(WindowPin::Window(2));
        assert!(check_scope(&Reader, &policy, "TestClass", Some(target)).is_err());
        policy.window_pin = None;
        policy.stopped = true;
        assert!(check_scope(&Reader, &policy, "TestClass", Some(target)).is_err());
        policy.stopped = false;
        policy.window_classes.clear();
        assert!(check_scope(&Reader, &policy, "TestClass", Some(target)).is_err());
        policy.allowed_applications.clear();
        assert!(check_scope(&Reader, &policy, "TestClass", Some(target)).is_err());
    }
}
