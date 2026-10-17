//! Offline RNF-06 diagnostics, complementary to PERF-03; never a live soak.
//! Run in release with --out PATH [--observations N]. No stdout, screen or input.
use ommatix_adapter_sim::{FixtureDesktop, FixtureScene, confirmed};
use ommatix_application::contract::{ActionKind, ActionRequest, ExpectedEffect, ObserveRequest};
use ommatix_application::{
    CapturedFrame, Engine, EngineConfig, FrameCapturer, IconResolverConfig, LocalPolicy,
    ManualClock, NoAccessibility, PolicyStore, PortError, Ports, TextRecognizer, VisionDetector,
};
use ommatix_domain::appearance::{
    CachePolicy, DESCRIPTOR_VERSION, GRID_LEN, IconCache, Prototype, PrototypeSource,
    gradient_descriptor,
};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Control, Role, Word};
use ommatix_domain::incremental::BgraFrame;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

struct DenyAll;
impl PolicyStore for DenyAll {
    fn current(&self) -> LocalPolicy {
        LocalPolicy::default()
    }
}

// Avoid FixtureDetector's intentional, ever-growing test-call log in a resource measurement.
struct Detector(FixtureDesktop, Arc<AtomicU8>);
impl VisionDetector for Detector {
    fn name(&self) -> &'static str {
        "bounded-fixture-detector"
    }
    fn models_loaded(&self) -> bool {
        false
    }
    fn detect(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Control>, PortError> {
        if self.1.load(Ordering::Relaxed) == 1 {
            return Err(PortError::new(self.name(), "injected detector failure"));
        }
        Ok(self.0.scene().controls_in(regions))
    }
}
struct Recognizer(FixtureDesktop, Arc<AtomicU8>);
impl TextRecognizer for Recognizer {
    fn name(&self) -> &'static str {
        "bounded-fixture-recognizer"
    }
    fn recognize(&mut self, _: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        if self.1.load(Ordering::Relaxed) == 2 {
            return Err(PortError::new(self.name(), "injected recognizer failure"));
        }
        Ok(self.0.scene().words_in(regions))
    }
}

// Paint the generated prototype on each icon, preserving the fixture's other pixels.
struct Screen(Box<dyn FrameCapturer>, FixtureDesktop);
impl FrameCapturer for Screen {
    fn name(&self) -> &'static str {
        "textured-fixture"
    }
    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        let mut captured = self.0.capture()?;
        let mut pixels = captured.frame.pixels().to_vec();
        for (_, control) in self.1.scene().controls() {
            let x0 = control.bbox.x0() as usize;
            let y0 = control.bbox.y0() as usize;
            for y in 0..16 {
                for x in 0..18 {
                    let offset = ((y0 + y) * 128 + x0 + x) * 4;
                    let gray = if (x / 2).is_multiple_of(2) { 0 } else { 255 };
                    pixels[offset..offset + 3].fill(gray);
                }
            }
        }
        captured.frame = BgraFrame::new(128, 64, 512, 1, pixels)
            .map_err(|_| PortError::new(self.name(), "invalid generated frame"))?;
        Ok(captured)
    }
}

fn bbox(x: u32, y: u32, w: u32, h: u32) -> BBox {
    BBox::new(
        f64::from(x),
        f64::from(y),
        f64::from(x + w),
        f64::from(y + h),
    )
    .expect("generated boxes fit the fixture")
}
fn icons() -> IconResolverConfig {
    let grid: Vec<u8> = (0..GRID_LEN)
        .map(|i| if (i % 9).is_multiple_of(2) { 0 } else { 255 })
        .collect();
    let mut cache = IconCache::new();
    for _ in 0..64 {
        cache.insert(Prototype {
            label: "stripe".into(),
            context: "synthetic".into(),
            descriptor_version: DESCRIPTOR_VERSION,
            descriptor: gradient_descriptor(&grid, 3)
                .expect("generated grid has the descriptor shape"),
            source: PrototypeSource::Curated,
        });
    }
    IconResolverConfig {
        cache,
        policy: CachePolicy::new(0.1, 0.1).expect("explicit synthetic thresholds are valid"),
        tau: 3,
        context: "synthetic".into(),
    }
}
fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn measure(n: u32) -> Result<Value, Box<dyn std::error::Error>> {
    let clock = Arc::new(ManualClock::new());
    let desktop = FixtureDesktop::new(FixtureScene::new(128, 64), clock.clone());
    let failure = Arc::new(AtomicU8::new(0));
    let mut config = EngineConfig::new("offline-state-bounds");
    config.concurrent_stages = false;
    config.icons = Some(icons());
    let mut engine = Engine::new(
        config,
        Ports {
            capturer: Box::new(Screen(Box::new(desktop.capturer()), desktop.clone())),
            detector: Box::new(Detector(desktop.clone(), failure.clone())),
            recognizer: Box::new(Recognizer(desktop.clone(), failure.clone())),
            accessibility: Box::new(NoAccessibility),
            actuator: Box::new(desktop.actuator()),
            policy: Arc::new(DenyAll),
            clock: clock.clone(),
        },
    );
    let mut samples = Vec::new();
    let mut maxima = BTreeMap::new();
    let mut failed = 0;
    for i in 1..=n {
        desktop.update(|scene| {
            let ids: Vec<_> = scene
                .controls()
                .map(|(id, _)| id)
                .chain(scene.words().map(|(id, _)| id))
                .collect();
            for id in ids {
                scene.remove(id);
            }
            if !i.is_multiple_of(3) {
                scene.add_control(confirmed(Role::IconButton, bbox(i % 2 * 64, 0, 18, 16)));
            }
            scene.add_word(format!("item-{i}"), bbox(100, 40, 20, 10));
        });
        // Ten consecutive changing failed observations per block; alternate the failed port.
        let fail = i % 100 < 10;
        failure.store(
            if fail {
                if (i / 100).is_multiple_of(2) { 1 } else { 2 }
            } else {
                0
            },
            Ordering::Relaxed,
        );
        let result = engine.observe_screen(&ObserveRequest::default());
        if fail {
            failed += 1;
            if result.is_ok() {
                return Err("injected failure was not observed".into());
            }
        } else {
            result?;
        }
        // No actuation: missing target is rejected and exercises the bounded journal.
        engine.execute_action(&ActionRequest {
            request_id: format!("r{i}"),
            session_id: engine.session_id().into(),
            snapshot_id: "s0".into(),
            geometry_epoch: 1,
            target_id: "e1".into(),
            expected_revision: 1,
            action: ActionKind::Click,
            dry_run: true,
            expected_effect: ExpectedEffect::None,
        });
        let sizes = engine.retained_state_sizes();
        for (&key, &value) in &sizes {
            maxima
                .entry(key)
                .and_modify(|v: &mut usize| *v = (*v).max(value))
                .or_insert(value);
        }
        if i.is_multiple_of(1000) || i == n {
            samples.push(
                json!({"attempts": i, "commits": engine.engine_metrics().observations,
                "failures": failed, "collections": sizes, "working_set_bytes": null}),
            );
        }
        clock.advance(Duration::from_millis(1));
    }
    Ok(
        json!({"schema": 1, "synthetic_offline": true, "observations_attempted": n,
        "failed_observations": failed, "commit": command_output("git", &["rev-parse", "HEAD"]),
        "working_tree_dirty": command_output("git", &["status", "--porcelain"]).map(|s| !s.is_empty()),
        "binary_sha256": std::env::current_exe().ok().and_then(|p| std::fs::read(p).ok()).map(|b| Sha256::digest(b).iter().fold(String::with_capacity(64), |mut hex, byte| { let _ = write!(hex, "{byte:02x}"); hex })),
        "rustc": command_output("rustc", &["--version"]), "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "machine": std::env::var("COMPUTERNAME").ok(), "processor": std::env::var("PROCESSOR_IDENTIFIER").ok(),
        "frame": [128, 64], "tile_size": 64, "live_capture": false, "input_delivered": false,
        "accessibility": "NoAccessibility", "concurrent_stages": false,
        "working_set": "not measured: sysinfo absent from Cargo.lock; no dependency or unsafe added",
        "storage_bytes": "typed storage plus string capacities; excludes allocator/tree overhead, RSS and peak/transient memory",
        "maxima": maxima, "samples": samples}),
    )
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("state_bounds requires --release".into());
    }
    let mut args = std::env::args().skip(1);
    let mut out = None;
    let mut n = 20_000_u32;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = Some(args.next().ok_or("--out needs a path")?),
            "--observations" => n = args.next().ok_or("--observations needs a count")?.parse()?,
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if n == 0 {
        return Err("--observations must be positive".into());
    }
    let out = out.ok_or("--out PATH is required")?;
    std::fs::write(out, serde_json::to_vec_pretty(&measure(n)?)?)?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("state_bounds: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rnf_06_measurement_injects_both_failure_ports_and_exercises_resolved_icons() {
        let report = measure(200).unwrap();
        assert_eq!(report["failed_observations"], 20);
        assert_eq!(report["maxima"]["registry_entries"], 2);
        assert_eq!(report["maxima"]["base_pixel_bytes"], 32768);
        assert_eq!(report["maxima"]["icon_prototypes"], 64);
        assert_eq!(report["maxima"]["icon_labels"], 1);
        assert_eq!(report["maxima"]["pending_tiles"], 2);
        assert_eq!(report["samples"][0]["commits"], 180);
        assert!(report["samples"][0]["working_set_bytes"].is_null());
    }
}
