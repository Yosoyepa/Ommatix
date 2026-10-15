//! The generated scenes of the families the protocol names (ADR-0027, `04_Protocolo_y_Gates.md`):
//! sparse grid, multi-column forms, many texts per control, nested containers, extreme overlap,
//! identical boxes, shared labels, support chains and fractional coordinates. Shared by the
//! domain's differential tests and by the release benchmark, so both measure the same scenes.
//! Deterministic: every scene comes from its seed, with a small generator and no dependency.

use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{Actionability, Affordance, Control, Role, Word};

/// SplitMix64: a small deterministic generator, so the scenes need no dependency.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    SparseGrid,
    MultiColumnForm,
    ManyTextsPerControl,
    NestedContainers,
    ExtremeOverlap,
    IdenticalBoxes,
    SharedLabels,
    SupportChains,
    FractionalCoordinates,
}

pub const FAMILIES: [Family; 9] = [
    Family::SparseGrid,
    Family::MultiColumnForm,
    Family::ManyTextsPerControl,
    Family::NestedContainers,
    Family::ExtremeOverlap,
    Family::IdenticalBoxes,
    Family::SharedLabels,
    Family::SupportChains,
    Family::FractionalCoordinates,
];

pub struct Scene {
    pub controls: Vec<Control>,
    pub words: Vec<Word>,
}

fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
    BBox::new(x0, y0, x1, y1).expect("generated boxes have positive extent")
}

fn confirmed() -> Affordance {
    Affordance {
        actionability: Actionability::Confirmed,
        enabled: true,
        visible: true,
    }
}

fn control(bbox: BBox, role: Role) -> Control {
    let affordance = if matches!(role, Role::Button | Role::IconButton) {
        confirmed()
    } else {
        Affordance {
            actionability: Actionability::Unknown,
            ..confirmed()
        }
    };
    Control {
        bbox,
        role,
        affordance,
    }
}

fn word(text: &str, bbox: BBox) -> Word {
    Word::new(text, bbox)
}

/// A scene of about `n` controls (and about as many words) of one family.
pub fn scene(family: Family, n: usize, seed: u64) -> Scene {
    let mut rng = Rng(seed ^ (n as u64).wrapping_mul(0x1000_0000_01B3));
    let mut scene = Scene {
        controls: Vec::new(),
        words: Vec::new(),
    };
    match family {
        Family::SparseGrid => sparse_grid(n, &mut rng, &mut scene),
        Family::MultiColumnForm => multi_column_form(n, &mut rng, &mut scene),
        Family::ManyTextsPerControl => many_texts_per_control(n, &mut rng, &mut scene),
        Family::NestedContainers => nested_containers(n, &mut scene),
        Family::ExtremeOverlap => extreme_overlap(n, &mut rng, &mut scene),
        Family::IdenticalBoxes => identical_boxes(n, &mut scene),
        Family::SharedLabels => shared_labels(n, &mut scene),
        Family::SupportChains => support_chains(n, &mut rng, &mut scene),
        Family::FractionalCoordinates => fractional_coordinates(n, &mut rng, &mut scene),
    }
    scene
}

/// Buttons on a grid with gaps between them; most hold one word, some sit in the gaps.
fn sparse_grid(n: usize, rng: &mut Rng, scene: &mut Scene) {
    let cols = (n as f64).sqrt().ceil() as usize;
    for i in 0..n {
        let (x, y) = ((i % cols) as f64 * 120.0, (i / cols) as f64 * 60.0);
        scene.controls.push(control(
            bx(x + 10.0, y + 10.0, x + 90.0, y + 40.0),
            Role::Button,
        ));
        if rng.chance(0.8) {
            scene
                .words
                .push(word("b", bx(x + 20.0, y + 15.0, x + 60.0, y + 30.0)));
        }
        if rng.chance(0.25) {
            scene
                .words
                .push(word("gap", bx(x + 95.0, y + 45.0, x + 110.0, y + 55.0)));
        }
    }
}

/// Input fields in three columns, each with a label to its left and sometimes one above.
fn multi_column_form(n: usize, rng: &mut Rng, scene: &mut Scene) {
    for i in 0..n {
        let (col, row) = (i % 3, i / 3);
        let x = 200.0 + col as f64 * 420.0;
        let y = 80.0 + row as f64 * 60.0;
        scene
            .controls
            .push(control(bx(x, y, x + 220.0, y + 30.0), Role::Input));
        let dx = rng.range(10.0, 40.0);
        scene
            .words
            .push(word("Campo", bx(x - dx - 90.0, y + 6.0, x - dx, y + 22.0)));
        if rng.chance(0.3) {
            scene
                .words
                .push(word("Sección", bx(x, y - 22.0, x + 80.0, y - 6.0)));
        }
        if rng.chance(0.2) {
            scene
                .words
                .push(word("valor", bx(x + 8.0, y + 8.0, x + 60.0, y + 22.0)));
        }
    }
}

/// Large buttons with sixteen words each, and one word across each right edge.
fn many_texts_per_control(n: usize, rng: &mut Rng, scene: &mut Scene) {
    let per_control = 16;
    for i in 0..n.div_ceil(per_control) {
        let (x, y) = ((i % 8) as f64 * 520.0, (i / 8) as f64 * 140.0);
        scene
            .controls
            .push(control(bx(x, y, x + 480.0, y + 120.0), Role::Button));
        for k in 0..per_control {
            let (cx, cy) = ((k % 4) as f64 * 110.0, (k / 4) as f64 * 26.0);
            let wx = x + 12.0 + cx + rng.range(0.0, 8.0);
            scene
                .words
                .push(word("t", bx(wx, y + 10.0 + cy, wx + 50.0, y + 24.0 + cy)));
        }
        scene
            .words
            .push(word("edge", bx(x + 470.0, y + 60.0, x + 520.0, y + 74.0)));
    }
}

/// A panel, three buttons inside it, an icon inside each button, and the panel's title.
fn nested_containers(n: usize, scene: &mut Scene) {
    for i in 0..n.div_ceil(7) {
        let (x, y) = ((i % 6) as f64 * 650.0, (i / 6) as f64 * 420.0);
        scene
            .controls
            .push(control(bx(x, y, x + 600.0, y + 400.0), Role::Unknown));
        scene
            .words
            .push(word("Panel", bx(x + 20.0, y + 10.0, x + 120.0, y + 30.0)));
        for b in 0..3_u32 {
            let bx0 = x + 40.0 + f64::from(b) * 180.0;
            scene.controls.push(control(
                bx(bx0, y + 80.0, bx0 + 150.0, y + 120.0),
                Role::Button,
            ));
            scene.words.push(word(
                "Acción",
                bx(bx0 + 45.0, y + 92.0, bx0 + 105.0, y + 108.0),
            ));
            scene.controls.push(control(
                bx(bx0 + 4.0, y + 84.0, bx0 + 24.0, y + 104.0),
                Role::IconButton,
            ));
        }
    }
}

/// Controls of random sizes around one point, and small words around it: heavy overlap.
fn extreme_overlap(n: usize, rng: &mut Rng, scene: &mut Scene) {
    for _ in 0..n {
        let half_w = rng.range(5.0, 300.0);
        let half_h = rng.range(5.0, 200.0);
        scene.controls.push(control(
            bx(
                400.0 - half_w,
                300.0 - half_h,
                400.0 + half_w,
                300.0 + half_h,
            ),
            Role::Button,
        ));
    }
    for _ in 0..n {
        let (x, y) = (rng.range(300.0, 480.0), rng.range(250.0, 340.0));
        scene.words.push(word("x", bx(x, y, x + 20.0, y + 12.0)));
    }
}

/// Four distinct boxes, each repeated: identical controls and words that match them exactly.
fn identical_boxes(n: usize, scene: &mut Scene) {
    let distinct = [
        bx(0.0, 0.0, 100.0, 40.0),
        bx(0.0, 60.0, 100.0, 100.0),
        bx(120.0, 0.0, 220.0, 40.0),
        bx(120.0, 60.0, 220.0, 100.0),
    ];
    for i in 0..n {
        scene.controls.push(control(distinct[i % 4], Role::Input));
    }
    for (i, b) in distinct.iter().enumerate() {
        for _ in 0..(n / 8).max(1) {
            scene
                .words
                .push(word(if i % 2 == 0 { "same" } else { "inner" }, *b));
        }
    }
    scene.words.push(word("inside", bx(10.0, 10.0, 30.0, 25.0)));
}

/// Fields in one column, each with two candidate labels that are about equally plausible.
fn shared_labels(n: usize, scene: &mut Scene) {
    for i in 0..n {
        let y = i as f64 * 40.0;
        scene
            .controls
            .push(control(bx(300.0, y, 500.0, y + 30.0), Role::Input));
        scene
            .words
            .push(word("izq", bx(240.0, y + 6.0, 290.0, y + 20.0)));
        scene
            .words
            .push(word("arr", bx(300.0, y - 16.0, 360.0, y - 3.0)));
    }
}

/// Controls that overlap their neighbours in a row, and words between them.
fn support_chains(n: usize, rng: &mut Rng, scene: &mut Scene) {
    for i in 0..n {
        let x = i as f64 * 90.0;
        let role = if i % 2 == 0 {
            Role::Input
        } else {
            Role::Button
        };
        scene
            .controls
            .push(control(bx(x, 100.0, x + 100.0, 130.0), role));
        scene
            .words
            .push(word("c", bx(x + 10.0, 108.0, x + 40.0, 122.0)));
        if rng.chance(0.5) {
            scene
                .words
                .push(word("s", bx(x + 88.0, 40.0, x + 112.0, 60.0)));
        }
    }
}

/// The form of the multi-column family at a 1.25 scale and with fractional offsets.
fn fractional_coordinates(n: usize, rng: &mut Rng, scene: &mut Scene) {
    for i in 0..n {
        let (col, row) = (i % 3, i / 3);
        let s = 1.25;
        let x = (200.0 + col as f64 * 420.0 + 0.37) * s;
        let y = (80.0 + row as f64 * 60.0 + 0.61) * s;
        let w = rng.range(200.0, 260.0);
        scene
            .controls
            .push(control(bx(x, y, x + w, y + 30.5 * s), Role::Input));
        let dx = rng.range(7.5, 37.5);
        scene.words.push(word(
            "Nombre",
            bx(x - dx - 85.25, y + 5.9, x - dx, y + 21.7),
        ));
        if rng.chance(0.3) {
            scene
                .words
                .push(word("Dirección", bx(x + 0.3, y - 21.3, x + 79.9, y - 6.1)));
        }
    }
}
