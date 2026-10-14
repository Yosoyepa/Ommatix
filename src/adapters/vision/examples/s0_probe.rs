//! S0 measurement probe for the ONNX detector: loads it exactly as the engine does (every
//! file verified against its pin), runs it on synthetic frames and prints, per frame, one
//! JSON line with the inference times and the candidates after NMS (input coordinates and
//! score). Comparing two runs (CPU and DirectML, or 1 and N threads) gives the box and
//! score differences of `docs/thesis/S0_RESULTADOS.md`.
//!
//! The frames are raw files written by `tools/s0/synthetic_frames.py`: tightly packed BGRA,
//! named `<name>_<width>x<height>.bgra`. They are synthetic drawings, never captures.
//!
//! ```text
//! cargo run --release -p ommatix-adapter-vision --features directml --example s0_probe -- \
//!   <cpu|directml> <threads> <runs> <manifest> <manifest_sha256> <onnxruntime.dll> \
//!   <onnxruntime_sha256> [<DirectML.dll> <directml_sha256>] -- <frame.bgra>...
//! ```

#![allow(clippy::print_stdout)] // the probe's output is its JSON lines

use std::path::PathBuf;
use std::time::Instant;

use ommatix_adapter_vision::model::manifest::ExecutionProvider;
use ommatix_adapter_vision::{OnnxDetectorSource, OnnxVisionDetector};
use ommatix_domain::incremental::BgraFrame;

fn usage() -> ! {
    eprintln!(
        "usage: s0_probe <cpu|directml> <threads> <runs> <manifest> <manifest_sha256> \
         <onnxruntime.dll> <onnxruntime_sha256> [<DirectML.dll> <directml_sha256>] -- \
         <frame.bgra>..."
    );
    std::process::exit(2)
}

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("{message}");
    std::process::exit(1)
}

fn frame(path: &PathBuf) -> BgraFrame {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let size = stem.rsplit('_').next().unwrap_or_default();
    let Some((w, h)) = size.split_once('x') else {
        usage()
    };
    let (Ok(width), Ok(height)) = (w.parse::<u32>(), h.parse::<u32>()) else {
        usage()
    };
    let pixels = std::fs::read(path)
        .unwrap_or_else(|e| fail(format!("cannot read {}: {e}", path.display())));
    BgraFrame::new(width, height, 4 * width as usize, 0, pixels)
        .unwrap_or_else(|e| fail(format!("{}: {e}", path.display())))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(split) = args.iter().position(|a| a == "--") else {
        usage()
    };
    let (head, frames) = (&args[..split], &args[split + 1..]);
    if !(head.len() == 7 || head.len() == 9) || frames.is_empty() {
        usage()
    }
    let Some(execution_provider) = ExecutionProvider::parse(&head[0]) else {
        usage()
    };
    let (Ok(intra_threads), Ok(runs)) = (head[1].parse::<usize>(), head[2].parse::<usize>()) else {
        usage()
    };
    let source = OnnxDetectorSource {
        manifest_path: PathBuf::from(&head[3]),
        manifest_sha256: head[4].clone(),
        runtime_path: PathBuf::from(&head[5]),
        runtime_sha256: head[6].clone(),
        execution_provider,
        directml: (head.len() == 9).then(|| (PathBuf::from(&head[7]), head[8].clone())),
        intra_threads,
    };
    let started = Instant::now();
    let mut detector = OnnxVisionDetector::load(&source)
        .unwrap_or_else(|e| fail(ommatix_application::error_chain(&e)));
    eprintln!(
        "loaded {} in {:.0} ms",
        execution_provider.name(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    for path in frames.iter().map(PathBuf::from) {
        let frame = frame(&path);
        let mut times = Vec::with_capacity(runs);
        let mut last = Vec::new();
        for _ in 0..runs.max(1) {
            let t = Instant::now();
            last = detector
                .frame_candidates(&frame)
                .unwrap_or_else(|e| fail(ommatix_application::error_chain(&e)));
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let candidates: Vec<String> = last
            .iter()
            .map(|c| {
                format!(
                    "[{:.4},{:.4},{:.4},{:.4},{:.7}]",
                    c.bbox[0], c.bbox[1], c.bbox[2], c.bbox[3], c.score
                )
            })
            .collect();
        let times: Vec<String> = times.iter().map(|t| format!("{t:.3}")).collect();
        println!(
            "{{\"frame\":{:?},\"provider\":{:?},\"threads\":{intra_threads},\"ms\":[{}],\"candidates\":[{}]}}",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default(),
            execution_provider.name(),
            times.join(","),
            candidates.join(",")
        );
    }
}
