//! ADR-0027 (INC-05): the candidate enumeration of Algorithm 2, the reference scan against the
//! experimental R*-tree, on generated scenes of the families the protocol names
//! (`docs/thesis/04_ALGORITMOS_CORE.md` §4.3 and `docs/thesis/INDICE_A2_RESULTADOS.md`).
//!
//! ```text
//! cargo run --release -p ommatix-acceptance --example candidate_index -- --out indice_a2.json
//! ```
//!
//! For every family and size it checks that both enumerators give the same fusion (G1 and G2 on
//! these scenes), then measures in interleaved repetitions: the whole geometric stage (`fuse`),
//! the construction of the index alone, the enumeration of candidates alone (the owner window of
//! every word and the corridor window of every labelled control), and what is left of the stage
//! (refinement, order and the result), by difference. It measures no screen and sends no input.
//! It refuses to run unoptimised. Percentiles are nearest-rank over the repetitions after a
//! declared warm-up; medians are the mean of the two middle values; the reduction of the median
//! has a 95 % bootstrap interval over the paired repetitions.

// The report is the output of the run: it prints to stdout, as core_latency does.
#![allow(clippy::print_stdout)]

#[path = "../../domain/tests/support/families.rs"]
mod families;

use std::hint::black_box;
use std::time::{Duration, Instant};

use families::{FAMILIES, Family, Scene, scene};
use ommatix_domain::bbox::BBox;
use ommatix_domain::candidates::{CandidateSearch, ControlSearch};
use ommatix_domain::fusion::{FusionParams, corridor, fuse, internal_owners_with};

/// The families whose queries are selective (a window meets few controls). Declared before the
/// first measurement: the target of the net-cost gate G4.
const TARGET: [Family; 2] = [Family::SparseGrid, Family::MultiColumnForm];
/// The scales of the protocol, in controls (`04_Protocolo_y_Gates.md`).
const SIZES: [usize; 5] = [32, 128, 512, 2048, 8192];
/// The reference's owner threshold and label margin (`FusionParams::default`).
const THRESHOLD: f64 = 0.75;
const MARGIN: f64 = 0.15;
/// Bootstrap resamples of the paired repetitions for the interval of the reduction.
const RESAMPLES: usize = 2000;
/// Gate G4: the reduction of the median geometric cost in a target family.
const G4_REDUCTION: f64 = 0.20;
/// Gate G5: a p95 regression is reviewed above the larger of this share and 0.2 ms.
const G5_SHARE: f64 = 0.01;
const G5_FLOOR_NS: f64 = 200_000.0;

/// SplitMix64, for the bootstrap resamples only: deterministic, with no dependency.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// The median: the mean of the two middle values when the count is even.
fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        f64::midpoint(v[n / 2 - 1], v[n / 2])
    }
}

/// The nearest-rank percentile `q` (0 < q <= 1) of `values`.
fn nearest_rank(values: &[f64], q: f64) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let rank = (q * v.len() as f64).ceil() as usize;
    v[rank.clamp(1, v.len()) - 1]
}

/// The point ratio `median(rstar) / median(scan)` and its 95 % bootstrap interval, resampling the
/// paired repetitions together.
fn ratio_interval(scan: &[f64], rstar: &[f64], seed: u64) -> (f64, f64, f64) {
    let n = scan.len();
    let mut rng = Rng(seed);
    let mut ratios: Vec<f64> = (0..RESAMPLES)
        .map(|_| {
            let picks: Vec<usize> = (0..n).map(|_| rng.below(n)).collect();
            let s: Vec<f64> = picks.iter().map(|&i| scan[i]).collect();
            let r: Vec<f64> = picks.iter().map(|&i| rstar[i]).collect();
            median(&r) / median(&s)
        })
        .collect();
    ratios.sort_by(f64::total_cmp);
    let point = median(rstar) / median(scan);
    (
        point,
        nearest_rank(&ratios, 0.025),
        nearest_rank(&ratios, 0.975),
    )
}

/// Algorithm 2's label cost, as the fusion computes it (`fusion::label_cost`, the reference).
fn label_cost(control: &BBox, word: &BBox) -> Option<f64> {
    let scale = control.height().max(word.height()).max(1.0);
    let dy = (word.center().1 - control.center().1).abs();
    let dx0 = (word.x0() - control.x0()).abs();
    let cost = if word.x1() <= control.x0() && dy <= 0.8 * scale {
        (control.x0() - word.x1() + 2.5 * dy) / scale
    } else if word.y1() <= control.y0() && dx0 <= scale {
        (control.y0() - word.y1() + 1.5 * dx0) / scale
    } else {
        return None;
    };
    (cost <= 6.0).then_some(cost)
}

/// Whether the reference gives the word an owner: a claim at the threshold whose area is not
/// (nearly) tied with the next smallest one.
fn owned(boxes: &[BBox], word: &BBox) -> bool {
    let (mut best, mut second, mut any) = (f64::INFINITY, f64::INFINITY, false);
    for c in boxes {
        if c.ioc(word) >= THRESHOLD {
            any = true;
            let a = c.area();
            if a < best {
                second = best;
                best = a;
            } else if a < second {
                second = a;
            }
        }
    }
    let tied = second.is_finite() && (best - second).abs() <= 1e-9 * best.abs().max(second.abs());
    any && !tied
}

/// The queries one scene issues, as the fusion issues them.
struct Plan {
    /// The owner window of every word (the reference asks every word).
    words: Vec<BBox>,
    /// The corridor window of every labelled control and every free word whose label cost is in
    /// range.
    gaps: Vec<BBox>,
}

fn plan(s: &Scene) -> Plan {
    let boxes: Vec<BBox> = s.controls.iter().map(|c| c.bbox).collect();
    let free: Vec<BBox> = s
        .words
        .iter()
        .map(|w| w.bbox)
        .filter(|w| !owned(&boxes, w))
        .collect();
    let mut gaps = Vec::new();
    for c in &s.controls {
        if !c.role.takes_external_label() {
            continue;
        }
        for w in &free {
            let Some(gap) = corridor(w, &c.bbox) else {
                continue;
            };
            if label_cost(&c.bbox, w).is_some() {
                gaps.push(gap);
            }
        }
    }
    Plan {
        words: s.words.iter().map(|w| w.bbox).collect(),
        gaps,
    }
}

/// Every query of the plan against `search`: the owner windows, then the corridor windows.
/// Each candidate is visited and counted, so the work cannot be optimised away.
fn enumerate(search: &ControlSearch<'_>, plan: &Plan) -> usize {
    let mut sink = 0usize;
    for w in &plan.words {
        search.owner_candidates(w, THRESHOLD, |i| sink = sink.wrapping_add(i));
    }
    for g in &plan.gaps {
        search.any_overlapping(g, |i| {
            sink = sink.wrapping_add(i);
            false
        });
    }
    sink
}

/// The fused result of one scene under one enumerator, with the reference's margin and threshold.
fn fused(s: &Scene, threshold: f64, search: CandidateSearch) -> ommatix_domain::fusion::Fusion {
    fuse(
        &s.controls,
        &s.words,
        FusionParams {
            ioc_threshold: threshold,
            label_margin: MARGIN,
            candidate_search: search,
            ..FusionParams::default()
        },
    )
    .expect("valid parameters")
}

/// G1 and G2 on one scene: the fusion of both enumerators, at the reference threshold and at a
/// zero and a full threshold, and the owners alone. Returns the number of checks that differ.
fn differences(s: &Scene) -> usize {
    let boxes: Vec<BBox> = s.controls.iter().map(|c| c.bbox).collect();
    let mut differing = 0;
    for threshold in [THRESHOLD, 0.0, 1.0] {
        let scan = fused(s, threshold, CandidateSearch::Scan);
        let tree = fused(s, threshold, CandidateSearch::RStar);
        differing += usize::from(scan != tree);
    }
    let scan = internal_owners_with(&boxes, &s.words, THRESHOLD, CandidateSearch::Scan)
        .expect("valid threshold");
    let tree = internal_owners_with(&boxes, &s.words, THRESHOLD, CandidateSearch::RStar)
        .expect("valid threshold");
    differing + usize::from(scan != tree)
}

/// One variant's timings for one repetition, in nanoseconds.
#[derive(Clone, Copy)]
struct Sample {
    total: f64,
    build: f64,
    query: f64,
}

impl Sample {
    /// What the stage does besides building and enumerating: refinement, order and the result.
    fn refine(self) -> f64 {
        (self.total - self.build - self.query).max(0.0)
    }
}

fn nanos(d: Duration) -> f64 {
    d.as_nanos() as f64
}

/// One variant on one scene: the construction, then the enumeration over a built search, then
/// the whole stage (which builds its own search for the index).
fn sample(s: &Scene, plan: &Plan, variant: CandidateSearch) -> Sample {
    let boxes: Vec<BBox> = s.controls.iter().map(|c| c.bbox).collect();
    let start = Instant::now();
    let search = black_box(ControlSearch::new(&boxes, variant));
    let build = nanos(start.elapsed());
    let start = Instant::now();
    black_box(enumerate(&search, plan));
    let query = nanos(start.elapsed());
    drop(search);
    let start = Instant::now();
    black_box(fused(s, THRESHOLD, variant));
    let total = nanos(start.elapsed());
    Sample {
        total,
        build,
        query,
    }
}

/// Everything measured for one family and size.
struct Cell {
    family: Family,
    size: usize,
    controls: usize,
    words: usize,
    gaps: usize,
    checks: usize,
    differing: usize,
    scan: Vec<Sample>,
    tree: Vec<Sample>,
}

fn measure(family: Family, size: usize, reps: usize) -> Cell {
    let seed = 0x2026_1015 ^ size as u64;
    let s = scene(family, size, seed);
    let plan = plan(&s);
    let differing = differences(&s);
    // Warm-up: two repetitions per variant, not recorded.
    for _ in 0..2 {
        sample(&s, &plan, CandidateSearch::Scan);
        sample(&s, &plan, CandidateSearch::RStar);
    }
    let (mut scan, mut tree) = (Vec::with_capacity(reps), Vec::with_capacity(reps));
    for rep in 0..reps {
        // Alternate the order within each pair, so neither variant always runs second.
        if rep % 2 == 0 {
            scan.push(sample(&s, &plan, CandidateSearch::Scan));
            tree.push(sample(&s, &plan, CandidateSearch::RStar));
        } else {
            tree.push(sample(&s, &plan, CandidateSearch::RStar));
            scan.push(sample(&s, &plan, CandidateSearch::Scan));
        }
    }
    Cell {
        family,
        size,
        controls: s.controls.len(),
        words: s.words.len(),
        gaps: plan.gaps.len(),
        checks: 4,
        differing,
        scan,
        tree,
    }
}

fn column(samples: &[Sample], pick: impl Fn(&Sample) -> f64) -> Vec<f64> {
    samples.iter().map(pick).collect()
}

/// One row of the report: the medians and p95 of each quantity of one variant.
fn summary(samples: &[Sample]) -> serde_json::Value {
    let total = column(samples, |s| s.total);
    let build = column(samples, |s| s.build);
    let query = column(samples, |s| s.query);
    let refine = column(samples, |s| s.refine());
    let stat =
        |v: &[f64]| serde_json::json!({ "median_ns": median(v), "p95_ns": nearest_rank(v, 0.95) });
    serde_json::json!({
        "total": stat(&total),
        "build": stat(&build),
        "query": stat(&query),
        "refine_and_order": stat(&refine),
    })
}

fn report(cell: &Cell) -> (serde_json::Value, String) {
    let scan_total = column(&cell.scan, |s| s.total);
    let tree_total = column(&cell.tree, |s| s.total);
    let (point, lo, hi) = ratio_interval(&scan_total, &tree_total, 0x5EED ^ cell.size as u64);
    let reduction = 1.0 - point;
    let (reduction_lo, reduction_hi) = (1.0 - hi, 1.0 - lo);
    let p95_scan = nearest_rank(&scan_total, 0.95);
    let p95_tree = nearest_rank(&tree_total, 0.95);
    let p95_delta = p95_tree - p95_scan;
    let p95_limit = (G5_SHARE * p95_scan).max(G5_FLOOR_NS);
    let target = TARGET.contains(&cell.family);
    let g4_pass = target && cell.size >= 512 && reduction >= G4_REDUCTION && reduction_lo > 0.0;
    let line = format!(
        "{:<22} {:>5} {:>10.3} {:>10.3} {:>8.1}% [{:>6.1}, {:>6.1}] {:>9.3} {:>9.3} {}",
        format!("{:?}", cell.family),
        cell.size,
        median(&scan_total) / 1e6,
        median(&tree_total) / 1e6,
        100.0 * reduction,
        100.0 * reduction_lo,
        100.0 * reduction_hi,
        p95_delta / 1e6,
        p95_limit / 1e6,
        if cell.differing > 0 {
            "DIFFERS"
        } else if g4_pass {
            "G4 met"
        } else {
            ""
        },
    );
    let json = serde_json::json!({
        "family": format!("{:?}", cell.family),
        "size_requested": cell.size,
        "target_family": target,
        "controls": cell.controls,
        "words": cell.words,
        "corridor_queries": cell.gaps,
        "repetitions": cell.scan.len(),
        "equivalence": { "checks": cell.checks, "differences": cell.differing },
        "scan": summary(&cell.scan),
        "rstar": summary(&cell.tree),
        "reduction_median": reduction,
        "reduction_ci95": [reduction_lo, reduction_hi],
        "p95_delta_ns": p95_delta,
        "p95_review_limit_ns": p95_limit,
        "g4_pass": g4_pass,
        "g5_review": p95_delta > p95_limit && cell.size <= 128,
    });
    (json, line)
}

struct Options {
    out: Option<String>,
    reps: usize,
    sizes: Vec<usize>,
    commit: String,
    allow_debug: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        out: None,
        reps: 9,
        sizes: SIZES.to_vec(),
        commit: "unknown".to_owned(),
        allow_debug: false,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--out" => options.out = Some(value("--out")?),
            "--commit" => options.commit = value("--commit")?,
            "--reps" => {
                options.reps = value("--reps")?
                    .parse()
                    .map_err(|_| "--reps is a number".to_owned())?;
            }
            "--sizes" => {
                options.sizes = value("--sizes")?
                    .split(',')
                    .map(|s| {
                        s.trim()
                            .parse::<usize>()
                            .map_err(|_| format!("bad size {s:?}"))
                    })
                    .collect::<Result<_, _>>()?;
            }
            "--allow-debug" => options.allow_debug = true,
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if options.reps < 3 {
        return Err("--reps must be at least 3 for an interval".to_owned());
    }
    Ok(options)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("candidate_index: {message}");
            std::process::exit(2);
        }
    };
    if cfg!(debug_assertions) && !options.allow_debug {
        eprintln!(
            "candidate_index: this is not an optimised build; use --release (or --allow-debug)"
        );
        std::process::exit(2);
    }
    println!(
        "machine: {} {} / {} logical CPUs / {}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(0, usize::from),
        std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown CPU".to_owned()),
    );
    println!(
        "{:<22} {:>5} {:>10} {:>10} {:>9} {:>17} {:>9} {:>9}",
        "family", "size", "scan ms", "rstar ms", "reduction", "95% CI (%)", "p95 Δ ms", "limit ms"
    );
    let mut cells = Vec::new();
    for family in FAMILIES {
        for &size in &options.sizes {
            eprintln!("candidate_index: {family:?} {size} ...");
            let cell = measure(family, size, options.reps);
            let (json, line) = report(&cell);
            println!("{line}");
            cells.push(json);
        }
    }
    let differing: usize = cells
        .iter()
        .map(|c| c["equivalence"]["differences"].as_u64().unwrap_or(0))
        .sum::<u64>() as usize;
    let document = serde_json::json!({
        "experiment": "ADR-0027 candidate enumeration: scan against the R*-tree",
        "commit": options.commit,
        "build": "release",
        "repetitions": options.reps,
        "warm_up_repetitions": 2,
        "sizes": options.sizes,
        "target_families": TARGET.iter().map(|f| format!("{f:?}")).collect::<Vec<_>>(),
        "threshold": THRESHOLD,
        "label_margin": MARGIN,
        "resamples": RESAMPLES,
        "equivalence_differences": differing,
        "cells": cells,
    });
    if let Some(path) = &options.out {
        std::fs::write(path, serde_json::to_string_pretty(&document).expect("json"))
            .expect("the report can be written");
        eprintln!("candidate_index: report written to {path}");
    }
    if differing > 0 {
        eprintln!(
            "candidate_index: {differing} equivalence checks differ: the index is not equivalent"
        );
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nearest-rank p95 of 20 values is the 19th: the 95 % rank of 20 is 19.
    #[test]
    fn p95_is_the_nearest_rank() {
        let values: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(nearest_rank(&values, 0.95), 19.0);
        assert_eq!(median(&values), 10.5);
    }

    /// The owner test of the plan agrees with the reference on a tie and on a clear owner.
    #[test]
    fn owned_abstains_on_a_tie_like_the_reference() {
        let b = BBox::new(0.0, 0.0, 100.0, 100.0).unwrap();
        let word = BBox::new(10.0, 10.0, 30.0, 20.0).unwrap();
        assert!(!owned(&[b, b], &word));
        let small = BBox::new(0.0, 0.0, 50.0, 50.0).unwrap();
        assert!(owned(&[b, small], &word));
    }
}
