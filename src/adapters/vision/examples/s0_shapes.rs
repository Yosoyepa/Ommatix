//! RNF-05 measurement: do the smaller detector inputs find what the 1280×1280 one finds?
//!
//! Loads the detector exactly as the engine does (every file verified against its pin),
//! then, on synthetic frames written by `tools/s0/synthetic_frames.py`, compares the
//! reference path (every region padded to the largest input, as before RNF-05) with the
//! path the engine takes (each region in the smallest input that holds it, and the whole
//! frame in the smallest input that does not reduce it more than the reference does).
//! Boxes are matched at IoU 0.5 in frame coordinates. Prints one JSON line per comparison
//! and a summary per kind of region.
//!
//! ```text
//! cargo run --release -p ommatix-adapter-vision --features directml --example s0_shapes -- \
//!   <cpu|directml> <threads> <manifest> <manifest_sha256> <onnxruntime.dll> \
//!   <onnxruntime_sha256> [<DirectML.dll> <directml_sha256>] -- <frame.bgra>...
//! ```
//!
//! The frames are drawings, never captures; nothing is saved.

#![allow(clippy::print_stdout)] // the tool's output is its JSON lines

use std::collections::BTreeMap;
use std::path::PathBuf;

use ommatix_adapter_vision::model::manifest::ExecutionProvider;
use ommatix_adapter_vision::{OnnxDetectorSource, OnnxVisionDetector};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Control;
use ommatix_domain::incremental::BgraFrame;

/// Region sizes in frame pixels: what the S4 test application's animated labels produce
/// (310×160, 229×152), a few neighbours, and larger ones that need the bigger inputs.
const REGION_SIZES: [(u32, u32); 8] = [
    (310, 160),
    (229, 152),
    (318, 160),
    (300, 300),
    (400, 200),
    (600, 300),
    (640, 640),
    (900, 500),
];

fn usage() -> ! {
    eprintln!(
        "usage: s0_shapes <cpu|directml> <threads> <manifest> <manifest_sha256> \
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

fn iou(a: &BBox, b: &BBox) -> f64 {
    let (w, h) = (
        (a.x1().min(b.x1()) - a.x0().max(b.x0())).max(0.0),
        (a.y1().min(b.y1()) - a.y0().max(b.y0())).max(0.0),
    );
    let inter = w * h;
    let union =
        (a.x1() - a.x0()) * (a.y1() - a.y0()) + (b.x1() - b.x0()) * (b.y1() - b.y0()) - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// One comparison of two detections of the same region.
#[derive(Debug, Default, Clone)]
struct Compared {
    reference: usize,
    candidate: usize,
    matched: usize,
    ious: Vec<f64>,
    max_corner_diff: f64,
    max_score_diff: f64,
    /// Scores of the reference boxes the candidate lacks, and of the candidate boxes the
    /// reference lacks.
    missed: Vec<f32>,
    extra: Vec<f32>,
}

fn compare(reference: &[(Control, f32)], candidate: &[(Control, f32)]) -> Compared {
    let mut taken = vec![false; candidate.len()];
    let mut out = Compared {
        reference: reference.len(),
        candidate: candidate.len(),
        ..Compared::default()
    };
    for (r, rs) in reference {
        let best = candidate
            .iter()
            .enumerate()
            .filter(|(i, _)| !taken[*i])
            .map(|(i, (c, cs))| (i, iou(&r.bbox, &c.bbox), c, *cs))
            .filter(|(_, v, _, _)| *v >= 0.5)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if best.is_none() {
            out.missed.push(*rs);
        }
        if let Some((i, v, c, cs)) = best {
            taken[i] = true;
            out.matched += 1;
            out.ious.push(v);
            let corners = [
                (r.bbox.x0() - c.bbox.x0()).abs(),
                (r.bbox.y0() - c.bbox.y0()).abs(),
                (r.bbox.x1() - c.bbox.x1()).abs(),
                (r.bbox.y1() - c.bbox.y1()).abs(),
            ];
            out.max_corner_diff = out
                .max_corner_diff
                .max(corners.into_iter().fold(0.0, f64::max));
            out.max_score_diff = out.max_score_diff.max(f64::from((rs - cs).abs()));
        }
    }
    out.extra = candidate
        .iter()
        .zip(&taken)
        .filter(|(_, taken)| !**taken)
        .map(|((_, score), _)| *score)
        .collect();
    out
}

#[derive(Default)]
struct Summary {
    comparisons: usize,
    reference: usize,
    candidate: usize,
    matched: usize,
    iou_sum: f64,
    iou_min: f64,
    max_corner_diff: f64,
    max_score_diff: f64,
    exact: usize,
}

impl Summary {
    fn add(&mut self, c: &Compared) {
        if self.comparisons == 0 {
            self.iou_min = 1.0;
        }
        self.comparisons += 1;
        self.reference += c.reference;
        self.candidate += c.candidate;
        self.matched += c.matched;
        self.iou_sum += c.ious.iter().sum::<f64>();
        self.iou_min = c.ious.iter().copied().fold(self.iou_min, f64::min);
        self.max_corner_diff = self.max_corner_diff.max(c.max_corner_diff);
        self.max_score_diff = self.max_score_diff.max(c.max_score_diff);
        self.exact += usize::from(c.reference == c.candidate && c.matched == c.reference);
    }

    fn line(&self, kind: &str) -> String {
        let recall = if self.reference == 0 {
            1.0
        } else {
            self.matched as f64 / self.reference as f64
        };
        let precision = if self.candidate == 0 {
            1.0
        } else {
            self.matched as f64 / self.candidate as f64
        };
        let mean_iou = if self.matched == 0 {
            1.0
        } else {
            self.iou_sum / self.matched as f64
        };
        format!(
            "{{\"kind\":{kind:?},\"comparisons\":{},\"reference_boxes\":{},\"candidate_boxes\":{},\
             \"matched_iou50\":{},\"recall\":{recall:.4},\"precision\":{precision:.4},\
             \"mean_iou\":{mean_iou:.5},\"min_iou\":{:.5},\"max_corner_diff_px\":{:.3},\
             \"max_score_diff\":{:.6},\"identical_sets\":{}}}",
            self.comparisons,
            self.reference,
            self.candidate,
            self.matched,
            self.iou_min,
            self.max_corner_diff,
            self.max_score_diff,
            self.exact,
        )
    }
}

/// The detector the reference detections come from, the one the candidate detections come
/// from (the same one at another input size, or another export of the network), and how a
/// region is routed.
struct Harness {
    reference: OnnxVisionDetector,
    candidate: Option<OnnxVisionDetector>,
    /// `S0_FORCE=640x640`: every region goes through that input of the candidate instead of
    /// its best fit; a region too big for it is skipped.
    forced: Option<usize>,
}

impl Harness {
    fn candidate(&self) -> &OnnxVisionDetector {
        self.candidate.as_ref().unwrap_or(&self.reference)
    }

    /// Detects `region` the reference way (largest input) and the candidate way, and
    /// compares them. `None` when the forced input cannot hold the region.
    fn compare(&mut self, frame: &BgraFrame, region: &BBox) -> Option<((u32, u32), Compared)> {
        let mut chosen = self
            .candidate()
            .input_for(frame, region)
            .unwrap_or_else(|| fail("empty region"));
        if let Some(forced) = self.forced {
            let (width, height) = self.candidate().input_sizes()[forced];
            let rect = (region.x1() - region.x0(), region.y1() - region.y0());
            if rect.0 > f64::from(width) || rect.1 > f64::from(height) {
                return None;
            }
            chosen = forced;
        }
        let reference = self.reference.input_sizes().len() - 1;
        let slow = self
            .reference
            .scored_at(frame, region, reference)
            .unwrap_or_else(|e| fail(ommatix_application::error_chain(&e)));
        let fast = self
            .candidate
            .as_mut()
            .unwrap_or(&mut self.reference)
            .scored_at(frame, region, chosen)
            .unwrap_or_else(|e| fail(ommatix_application::error_chain(&e)));
        Some((
            self.candidate().input_sizes()[chosen],
            compare(&slow, &fast),
        ))
    }
}

fn load(source: &OnnxDetectorSource) -> OnnxVisionDetector {
    OnnxVisionDetector::load(source).unwrap_or_else(|e| fail(ommatix_application::error_chain(&e)))
}

fn harness(source: &OnnxDetectorSource) -> Harness {
    let started = std::time::Instant::now();
    let reference = load(source);
    eprintln!(
        "loaded {} input sizes {:?} in {:.0} ms",
        reference.input_sizes().len(),
        reference.input_sizes(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    // `S0_CANDIDATE_MANIFEST=<path>` and `S0_CANDIDATE_SHA256=<hex>`: take the candidate
    // detections from another export of the network, loaded with the same runtime, instead
    // of from the reference model's other inputs.
    let candidate = std::env::var("S0_CANDIDATE_MANIFEST").ok().map(|path| {
        load(&OnnxDetectorSource {
            manifest_path: PathBuf::from(path),
            manifest_sha256: std::env::var("S0_CANDIDATE_SHA256")
                .unwrap_or_else(|_| fail("S0_CANDIDATE_SHA256 is required")),
            ..source.clone()
        })
    });
    let mut harness = Harness {
        reference,
        candidate,
        forced: None,
    };
    harness.forced = std::env::var("S0_FORCE").ok().map(|v| {
        let (w, h) = v.split_once('x').unwrap_or_else(|| usage());
        let size = (w.parse::<u32>().unwrap_or(0), h.parse::<u32>().unwrap_or(0));
        harness
            .candidate()
            .input_sizes()
            .iter()
            .position(|s| *s == size)
            .unwrap_or_else(|| fail("S0_FORCE is not one of the input sizes"))
    });
    harness
}

fn source(head: &[String]) -> OnnxDetectorSource {
    let Some(execution_provider) = ExecutionProvider::parse(&head[0]) else {
        usage()
    };
    let Ok(intra_threads) = head[1].parse::<usize>() else {
        usage()
    };
    OnnxDetectorSource {
        manifest_path: PathBuf::from(&head[2]),
        manifest_sha256: head[3].clone(),
        runtime_path: PathBuf::from(&head[4]),
        runtime_sha256: head[5].clone(),
        execution_provider,
        directml: (head.len() == 8).then(|| (PathBuf::from(&head[6]), head[7].clone())),
        intra_threads,
    }
}

/// One frame: the whole frame, then regions of the sizes the engine plans, swept over it.
fn sweep(harness: &mut Harness, path: &PathBuf, summaries: &mut BTreeMap<String, Summary>) {
    let frame = frame(path);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if let Some((input, c)) = harness.compare(&frame, &frame.bounds()) {
        println!(
            "{{\"frame\":{name:?},\"region\":\"full\",\"input\":{input:?},\"reference\":{},\
             \"candidate\":{},\"matched\":{},\"max_corner_diff_px\":{:.3}}}",
            c.reference, c.candidate, c.matched, c.max_corner_diff
        );
        summaries.entry("full frame".into()).or_default().add(&c);
    }
    for (w, h) in REGION_SIZES {
        let (step_x, step_y) = ((w * 3 / 2).max(1), (h * 3 / 2).max(1));
        for y in (0..)
            .step_by(step_y as usize)
            .take_while(|y| y + h <= frame.height())
        {
            for x in (0..)
                .step_by(step_x as usize)
                .take_while(|x| x + w <= frame.width())
            {
                let region = BBox::new(
                    f64::from(x),
                    f64::from(y),
                    f64::from(x + w),
                    f64::from(y + h),
                )
                .unwrap_or_else(|_| fail("bad region"));
                let Some((input, c)) = harness.compare(&frame, &region) else {
                    continue;
                };
                let kind = format!("{w}x{h} regions in {}x{}", input.0, input.1);
                summaries.entry(kind).or_default().add(&c);
                if c.reference != c.candidate || c.matched != c.reference {
                    println!(
                        "{{\"frame\":{name:?},\"region\":[{x},{y},{w},{h}],\"input\":{input:?},\
                         \"reference\":{},\"candidate\":{},\"matched\":{},\
                         \"missed_scores\":{:?},\"extra_scores\":{:?}}}",
                        c.reference, c.candidate, c.matched, c.missed, c.extra
                    );
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(split) = args.iter().position(|a| a == "--") else {
        usage()
    };
    let (head, frames) = (&args[..split], &args[split + 1..]);
    if !(head.len() == 6 || head.len() == 8) || frames.is_empty() {
        usage()
    }
    let mut harness = harness(&source(head));
    let mut summaries: BTreeMap<String, Summary> = BTreeMap::new();
    for path in frames.iter().map(PathBuf::from) {
        sweep(&mut harness, &path, &mut summaries);
    }
    for (kind, summary) in &summaries {
        println!("{}", summary.line(kind));
    }
}
