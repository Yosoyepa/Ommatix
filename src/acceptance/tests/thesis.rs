//! Step definitions for the thesis scenarios in `docs/thesis/features/`, run where the
//! dossier keeps them so its traceability matrix stays valid.
//!
//! Only the scenarios listed in [`IMPLEMENTED`] run; the others wait for the stage that
//! builds what they need (see `docs/thesis/matrices/gherkin_trazabilidad.csv`). The S1
//! and S2a scenarios exercise the domain core directly: the thesis contract that would let
//! them run through MCP arrives in S2b.

// cucumber passes captured step parameters by value.
#![allow(clippy::needless_pass_by_value)]

use std::collections::BTreeSet;

use cucumber::{World, given, then, when};
use ommatix_domain::appearance::{
    CachePolicy, DESCRIPTOR_VERSION, GRID_LEN, GradientDescriptor, IconCache, IconMatch, Prototype,
    PrototypeSource, SupportPolicy, UnknownReason, descriptor_distance, gradient_descriptor,
};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::{
    Actionability, Affordance, Control, FusionParams, LabelChoice, Role, Word,
    choose_external_label, fuse, internal_owners, reading_order,
};
use ommatix_domain::identity::{
    ActionRef, ActionRejection, AssociationPolicy, Changes, Clearance, Delivery, Observation,
    Registry, TargetId,
};
use ommatix_domain::incremental::{
    BgraFrame, Evidence, FullRefreshReason, PlanParams, RegionPlan, TileCoord, Work, plan_regions,
};
use ommatix_domain::snapshot::{CommitError, SnapshotStore, Staged, StoreState};

/// Scenarios with step definitions, by tag.
const IMPLEMENTED: &[&str] = &[
    "BDD-01", "BDD-02", "BDD-03", "BDD-04", "BDD-05", "BDD-06", "BDD-07", "BDD-08", "BDD-09",
    "BDD-10", "BDD-13", "BDD-14", "BDD-15", "BDD-16", "BDD-17", "BDD-18", "BDD-19", "BDD-20",
    "BDD-21",
];

#[derive(Debug, Default, World)]
struct Engine {
    store: SnapshotStore<Vec<String>>,
    current: Option<BgraFrame>,
    supports: Vec<BBox>,
    params: PlanParams,
    plan: Option<RegionPlan>,
    staged: Vec<Staged<Vec<String>>>,
    commits: Vec<Result<u64, CommitError>>,
    controls: Vec<BBox>,
    words: Vec<Word>,
    ioc: Option<f64>,
    owners: Vec<Vec<Word>>,
    ordered: Vec<Word>,
    expected_order: Vec<&'static str>,
    label: Option<LabelChoice>,
    registry: Option<Registry>,
    changes: Vec<Changes>,
    observations: Vec<Observation>,
    remembered: Vec<TargetId>,
    prepared: Option<(u64, TargetId, u64)>,
    verdict: Option<Result<Clearance, ActionRejection>>,
    crop: Vec<u8>,
    distance: Option<f64>,
    cache: IconCache,
    icon: Option<IconMatch>,
}

fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox {
    BBox::new(x0, y0, x1, y1).expect("valid box")
}

fn blank(w: u32, h: u32, padding: usize) -> BgraFrame {
    let stride = 4 * w as usize + padding;
    BgraFrame::new(w, h, stride, 1, vec![0u8; stride * h as usize]).expect("valid frame")
}

/// `frame` with the blue byte of pixel `(x, y)` set to 255.
fn with_pixel(frame: &BgraFrame, x: u32, y: u32) -> BgraFrame {
    let mut buf = frame.pixels().to_vec();
    buf[y as usize * frame.row_stride() + 4 * x as usize] = 255;
    BgraFrame::new(
        frame.width(),
        frame.height(),
        frame.row_stride(),
        frame.geometry_epoch(),
        buf,
    )
    .expect("same layout")
}

fn tiles(list: &[(u32, u32)]) -> BTreeSet<TileCoord> {
    list.iter().map(|&(c, r)| TileCoord::new(c, r)).collect()
}

/// A button whose actionability fusion confirmed.
fn button(bbox: BBox, label: &str) -> Observation {
    Observation {
        bbox,
        role: Role::Button,
        label: Some(label.to_owned()),
        discriminant: None,
        affordance: Affordance {
            actionability: Actionability::Confirmed,
            enabled: true,
            visible: true,
        },
    }
}

/// The oracle's striped icon (OR-29, OR-30): eight rows of `[0, 10, 10, 0, 0, 10, 10, 0, 0]`.
fn stripes() -> Vec<u8> {
    [0, 10, 10, 0, 0, 10, 10, 0, 0].repeat(8)
}

fn inverted(gray: &[u8]) -> Vec<u8> {
    gray.iter().map(|v| 255 - v).collect()
}

/// Descriptor with the oracle's gradient threshold.
fn descriptor(gray: &[u8]) -> GradientDescriptor {
    gradient_descriptor(gray, 3).expect("a 9x8 crop")
}

fn curated(label: &str, context: &str, gray: &[u8]) -> Prototype {
    Prototype {
        label: label.to_owned(),
        context: context.to_owned(),
        descriptor_version: DESCRIPTOR_VERSION,
        descriptor: descriptor(gray),
        source: PrototypeSource::Curated,
    }
}

/// The thresholds the unit tests use; the thesis calibrates them on the corpus (H4).
fn cache_policy() -> CachePolicy {
    CachePolicy::new(0.1, 0.1).expect("valid policy")
}

impl Engine {
    fn commit_base(&mut self, frame: BgraFrame) {
        let staged = self.store.stage(frame, vec!["e1".to_owned()]);
        self.store.commit(staged).expect("first commit");
    }

    fn base(&self) -> &BgraFrame {
        self.store.base().expect("a committed base")
    }

    fn plan(&mut self) -> &RegionPlan {
        let current = self.current.as_ref().expect("a current frame");
        let plan = plan_regions(
            self.store.base(),
            current,
            self.store.pending_dirty(),
            Evidence {
                supports: &self.supports,
                relations: &[],
            },
            &self.params,
        )
        .expect("valid plan input");
        self.plan.insert(plan)
    }

    fn planned(&self) -> &RegionPlan {
        self.plan.as_ref().expect("a plan")
    }

    fn registry(&self) -> &Registry {
        self.registry.as_ref().expect("a registry")
    }

    /// Observes `observations` in the current geometry epoch, starting a session if needed.
    fn observe(&mut self, observations: &[Observation]) -> &Changes {
        let current = self
            .registry
            .take()
            .unwrap_or_else(|| Registry::new("s", 1));
        let (next, changes) = current
            .observe(
                current.geometry_epoch(),
                observations,
                AssociationPolicy::default(),
            )
            .expect("valid policy");
        self.registry = Some(next);
        self.changes.push(changes);
        self.last_changes()
    }

    fn last_changes(&self) -> &Changes {
        self.changes.last().expect("an observation")
    }

    /// Checks a dry-run action on `target`, in this session and epoch, from `snapshot_id`.
    fn check(
        &self,
        snapshot_id: u64,
        target: TargetId,
        revision: u64,
    ) -> Result<Clearance, ActionRejection> {
        let registry = self.registry();
        let target = target.to_string();
        registry.check_action(
            &ActionRef {
                session_id: registry.session_id(),
                geometry_epoch: registry.geometry_epoch(),
                snapshot_id,
                target_id: &target,
                expected_revision: revision,
            },
            Delivery::DryRun,
        )
    }
}

// ---- 01_incrementalidad.feature ----

#[given("un frame base confirmado con ancho impar")]
fn odd_width_base(e: &mut Engine) {
    e.commit_base(blank(641, 321, 0));
}

#[when("cambia un único píxel de color en una fila impar")]
fn one_pixel_on_odd_row(e: &mut Engine) {
    e.current = Some(with_pixel(e.base(), 10, 1));
    e.plan();
}

#[then("el tile correspondiente queda sucio y no se descarta como caret")]
fn its_tile_is_dirty(e: &mut Engine) {
    let plan = e.planned();
    assert_eq!(plan.dirty, tiles(&[(0, 0)]));
    let Work::Regions(regions) = &plan.work else {
        panic!("expected regions, got {:?}", plan.work);
    };
    assert!(
        regions
            .iter()
            .any(|r| r.contains(&bx(10.0, 1.0, 11.0, 2.0)))
    );
}

#[given("un framebuffer con padding al final de cada fila")]
fn padded_base(e: &mut Engine) {
    e.commit_base(blank(65, 65, 12));
}

#[when("cambia un píxel real sin cambiar el padding")]
fn real_pixel_behind_padding(e: &mut Engine) {
    e.current = Some(with_pixel(e.base(), 4, 63));
    e.plan();
}

#[then("se detecta el cambio usando el stride del frame")]
fn detected_with_stride(e: &mut Engine) {
    assert_eq!(e.planned().dirty, tiles(&[(0, 0)]));

    // The padding alone is not a change.
    let base = e.base().clone();
    let mut buf = base.pixels().to_vec();
    buf[4 * 65] = 1;
    e.current = Some(BgraFrame::new(65, 65, base.row_stride(), 1, buf).expect("same layout"));
    assert_eq!(e.plan().work, Work::Nothing);
}

#[given("una región sucia que alcanza un control conectado a otro soporte")]
fn region_reaching_chained_supports(e: &mut Engine) {
    e.commit_base(blank(640, 320, 0));
    e.current = Some(with_pixel(e.base(), 10, 10));
    // The halo of tile (0, 0) reaches x = 96; the control reaches the second support.
    e.supports = vec![bx(90.0, 10.0, 200.0, 40.0), bx(190.0, 10.0, 300.0, 40.0)];
}

#[when("se calcula el cierre contextual")]
fn compute_closure(e: &mut Engine) {
    e.plan();
}

#[then("la región incluye ambos soportes completos")]
fn region_includes_both_supports(e: &mut Engine) {
    let plan = e.planned();
    let Work::Regions(regions) = &plan.work else {
        panic!("expected regions, got {:?}", plan.work);
    };
    assert_eq!(regions.len(), 1);
    for support in &e.supports {
        assert!(
            regions[0].contains(support),
            "{regions:?} misses {support:?}"
        );
    }
}

#[given("una región pendiente y un snapshot confirmado")]
fn pending_region_and_snapshot(e: &mut Engine) {
    e.commit_base(blank(640, 320, 0));
    e.current = Some(with_pixel(e.base(), 300, 200));
    e.plan();
    assert_eq!(e.store.state(), StoreState::Ready);
}

#[when("la inferencia de la región falla")]
fn inference_fails(e: &mut Engine) {
    let dirty = e.planned().dirty.clone();
    e.store.record_failure(&dirty);
}

#[then("se conserva la base confirmada y se bloquea la actuación con evidencia obsoleta")]
fn base_kept_and_actions_blocked(e: &mut Engine) {
    assert_eq!(e.store.base(), Some(&blank(640, 320, 0)));
    assert_eq!(e.store.published(), Some(&vec!["e1".to_owned()]));
    assert_eq!(e.store.state(), StoreState::Degraded);
    assert!(!e.store.actions_allowed());

    // The next plan still covers the failed region, even if the screen reverted.
    let dirty = e.planned().dirty.clone();
    e.current = Some(blank(640, 320, 0));
    assert_eq!(e.plan().dirty, dirty);
}

#[given("dos candidatos construidos desde la misma revisión")]
fn two_candidates(e: &mut Engine) {
    e.commit_base(blank(65, 65, 0));
    let first = with_pixel(e.base(), 1, 1);
    let second = with_pixel(e.base(), 2, 2);
    e.staged = vec![
        e.store.stage(first, vec!["primero".to_owned()]),
        e.store.stage(second, vec!["segundo".to_owned()]),
    ];
}

#[when("se publica el primero y después termina el segundo")]
fn commit_both(e: &mut Engine) {
    for staged in std::mem::take(&mut e.staged) {
        let result = e.store.commit(staged);
        e.commits.push(result);
    }
}

#[then("se rechaza el commit del segundo por obsoleto")]
fn second_is_stale(e: &mut Engine) {
    assert_eq!(e.commits[0], Ok(2));
    assert_eq!(
        e.commits[1],
        Err(CommitError::Stale {
            staged_on: 1,
            current: 2
        })
    );
    assert_eq!(e.store.published(), Some(&vec!["primero".to_owned()]));
}

#[given("regiones cuyo cierre supera el presupuesto configurado")]
fn closure_over_budget(e: &mut Engine) {
    e.commit_base(blank(1280, 640, 0));
    e.current = Some(with_pixel(&with_pixel(e.base(), 5, 5), 900, 500));
    // A chain of supports, listed so that each closure round reaches one more link: longer
    // than the closure may follow.
    e.supports = (0..10)
        .rev()
        .map(|i| {
            let x = f64::from(i) * 50.0;
            bx(x, 20.0, x + 60.0, 40.0)
        })
        .collect();
    e.params = PlanParams {
        max_closure_rounds: 2,
        ..PlanParams::default()
    };
}

#[when("se selecciona el plan de percepción")]
fn select_plan(e: &mut Engine) {
    e.plan();
}

#[then("se elige refresco completo sin omitir cambios")]
fn full_refresh_without_dropping(e: &mut Engine) {
    let plan = e.planned();
    assert_eq!(
        plan.work,
        Work::FullRefresh(FullRefreshReason::ClosureBudget)
    );
    assert_eq!(plan.dirty, tiles(&[(0, 0), (14, 7)]));
}

// ---- 02_fusion.feature ----

#[given(expr = "texto de {int} por {int} píxeles dentro de un botón de {int} por {int} píxeles")]
fn text_inside_button(e: &mut Engine, tw: u32, th: u32, bw: u32, bh: u32) {
    let button = bx(100.0, 100.0, 100.0 + f64::from(bw), 100.0 + f64::from(bh));
    let (cx, cy) = button.center();
    let (hw, hh) = (f64::from(tw) / 2.0, f64::from(th) / 2.0);
    e.controls = vec![button];
    e.words = vec![Word::new("Guardar", bx(cx - hw, cy - hh, cx + hw, cy + hh))];
}

#[when("se calcula la contención del texto")]
fn compute_containment(e: &mut Engine) {
    e.ioc = Some(e.controls[0].ioc(&e.words[0].bbox));
}

#[then("la IoC es 1 y el texto puede asociarse al botón")]
fn ioc_is_one(e: &mut Engine) {
    assert_eq!(e.ioc, Some(1.0));
    let button = Control {
        bbox: e.controls[0],
        role: Role::Button,
        affordance: Affordance {
            actionability: Actionability::Confirmed,
            enabled: true,
            visible: true,
        },
    };
    let fusion = fuse(&[button], &e.words, FusionParams::default()).expect("valid params");
    assert_eq!(fusion.controls[0].inner_text(), "Guardar");
    assert_eq!(fusion.text_nodes, Vec::<Word>::new());
}

#[given("un panel y un botón interno que contienen la misma caja OCR")]
fn panel_and_button(e: &mut Engine) {
    e.controls = vec![bx(0.0, 0.0, 400.0, 300.0), bx(20.0, 20.0, 140.0, 50.0)];
    e.words = vec![Word::new("Aceptar", bx(40.0, 25.0, 110.0, 45.0))];
}

#[when("se asigna propietario al texto")]
fn assign_owner(e: &mut Engine) {
    e.owners = internal_owners(&e.controls, &e.words, 0.75).expect("valid threshold");
}

#[then("se prefiere el control específico sin depender del orden de entrada")]
fn specific_control_wins(e: &mut Engine) {
    assert_eq!(e.owners, vec![vec![], e.words.clone()]);
    let reversed = [e.controls[1], e.controls[0]];
    assert_eq!(
        internal_owners(&reversed, &e.words, 0.75).expect("valid threshold"),
        vec![e.words.clone(), vec![]]
    );
}

#[given("palabras OCR de varias líneas en orden arbitrario")]
fn multiline_words(e: &mut Engine) {
    // Two lines; the second line starts left of where the first ends, and the words of the
    // first line are slightly misaligned vertically.
    e.words = vec![
        Word::new("olvidada", bx(90.0, 40.0, 160.0, 56.0)),
        Word::new("usuario", bx(140.0, 2.0, 210.0, 18.0)),
        Word::new("Nombre", bx(0.0, 0.0, 70.0, 16.0)),
        Word::new("Contraseña", bx(0.0, 40.0, 85.0, 56.0)),
        Word::new("de", bx(75.0, 1.0, 95.0, 17.0)),
    ];
    e.expected_order = vec!["Nombre", "de", "usuario", "Contraseña", "olvidada"];
}

#[when("se determina el orden de lectura")]
fn determine_reading_order(e: &mut Engine) {
    e.ordered = reading_order(&e.words);
}

#[then("se agrupan líneas antes de ordenar horizontalmente")]
fn lines_before_columns(e: &mut Engine) {
    let texts: Vec<&str> = e.ordered.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, e.expected_order);
    // Same result for every rotation of the input.
    for k in 1..e.words.len() {
        let mut rotated = e.words.clone();
        rotated.rotate_left(k);
        assert_eq!(reading_order(&rotated), e.ordered);
    }
}

#[given("dos etiquetas igualmente plausibles para un campo")]
fn two_equal_labels(e: &mut Engine) {
    let field = bx(100.0, 100.0, 300.0, 135.0);
    e.controls = vec![field];
    // Same distance and alignment above the field.
    e.words = vec![
        Word::new("Correo", bx(100.0, 72.0, 170.0, 92.0)),
        Word::new("Teléfono", bx(100.0, 72.0, 190.0, 92.0)),
    ];
}

#[when("se resuelve la asociación externa")]
fn resolve_external_label(e: &mut Engine) {
    e.label = Some(
        choose_external_label(&e.controls[0], &e.words, &e.controls, 0.15).expect("valid margin"),
    );
}

#[then("se informa ambigüedad en vez de seleccionar la primera")]
fn reports_ambiguity(e: &mut Engine) {
    assert_eq!(e.label, Some(LabelChoice::Ambiguous));
}

// ---- 03_identidad.feature ----

#[given("un track asociado inequívocamente a un botón en el límite de un tile")]
fn track_on_tile_edge(e: &mut Engine) {
    // The button ends exactly on the edge of the first 64 px tile.
    e.remembered = e
        .observe(&[button(bx(58.0, 100.0, 64.0, 110.0), "Guardar")])
        .created
        .clone();
    assert_eq!(e.remembered.len(), 1);
}

#[when("el botón se desplaza un píxel al tile vecino")]
fn button_moves_one_pixel(e: &mut Engine) {
    e.observe(&[button(bx(59.0, 100.0, 65.0, 110.0), "Guardar")]);
}

#[then("el registro conserva el ID y aumenta su revisión")]
fn id_kept_revision_bumped(e: &mut Engine) {
    let id = e.remembered[0];
    let changes = e.last_changes();
    assert_eq!(changes.created, vec![]);
    assert_eq!(changes.retired, vec![]);
    assert_eq!(changes.updated, vec![id]);
    let track = e.registry().get(id).expect("the track lives on");
    assert_eq!(track.revision, 2);
    assert_eq!(track.observation.bbox, bx(59.0, 100.0, 65.0, 110.0));
}

#[given("dos controles con el mismo rol y label dentro de una celda")]
fn two_repeated_controls(e: &mut Engine) {
    // Both inside the same 64 px cell, where the dossier's original ID collides (OR-35).
    e.observations = vec![
        button(bx(10.0, 10.0, 30.0, 20.0), "Eliminar"),
        button(bx(20.0, 30.0, 40.0, 40.0), "Eliminar"),
    ];
}

#[when("se crean sus tracks")]
fn create_tracks(e: &mut Engine) {
    let observations = e.observations.clone();
    e.remembered = e.observe(&observations).created.clone();
}

#[then("se asignan IDs distintos y no se sobrescribe ninguno")]
fn distinct_ids(e: &mut Engine) {
    assert_eq!(e.remembered.len(), 2);
    assert_ne!(e.remembered[0], e.remembered[1]);
    assert_eq!(e.registry().tracks().count(), 2);
    for (id, observation) in e.remembered.iter().zip(&e.observations) {
        assert_eq!(
            &e.registry().get(*id).expect("live").observation,
            observation
        );
    }
    // Observed again, in the other order, each keeps its own ID.
    let mut again = e.observations.clone();
    again.reverse();
    assert_eq!(e.observe(&again), &Changes::default());
}

/// A virtualised list row that shows `file`.
fn row(file: &str) -> Observation {
    Observation {
        discriminant: Some(file.to_owned()),
        ..button(bx(0.0, 100.0, 300.0, 120.0), "Abrir")
    }
}

#[given("una fila visible que representa el archivo A")]
fn row_shows_a(e: &mut Engine) {
    e.remembered = e.observe(&[row("informe-A.pdf")]).created.clone();
}

#[when("la misma posición pasa a representar el archivo B")]
fn row_shows_b(e: &mut Engine) {
    e.observe(&[row("informe-B.pdf")]);
}

#[then("no se conserva ciegamente la identidad accionable del archivo A")]
fn identity_of_a_not_kept(e: &mut Engine) {
    let a = e.remembered[0];
    let changes = e.last_changes();
    assert_eq!(changes.retired, vec![a]);
    assert_eq!(changes.created.len(), 1);
    assert_ne!(changes.created[0], a);
    assert!(e.registry().is_retired(a));
    let snapshot = e.registry().snapshot_id();
    assert_eq!(
        e.check(snapshot, a, 1),
        Err(ActionRejection::TargetNotFound)
    );
}

#[given("una acción preparada con una observación anterior")]
fn action_from_older_observation(e: &mut Engine) {
    let id = e.observe(&[button(bx(0.0, 0.0, 10.0, 10.0), "A")]).created[0];
    let snapshot = e.registry().snapshot_id();
    e.prepared = Some((snapshot, id, 1));
}

#[when("el agente solicita ejecutarla tras un nuevo snapshot")]
fn execute_after_new_snapshot(e: &mut Engine) {
    // Same element, plus a new one: the target and its revision still exist (OR-39).
    e.observe(&[
        button(bx(0.0, 0.0, 10.0, 10.0), "A"),
        button(bx(20.0, 20.0, 30.0, 30.0), "B"),
    ]);
    let (snapshot, id, revision) = e.prepared.expect("a prepared action");
    e.verdict = Some(e.check(snapshot, id, revision));
}

#[then("se devuelve STALE_SNAPSHOT sin reanclaje silencioso")]
fn stale_snapshot(e: &mut Engine) {
    let (snapshot, id, revision) = e.prepared.expect("a prepared action");
    let rejection = e.verdict.expect("a verdict").expect_err("rejected");
    assert_eq!(rejection, ActionRejection::StaleSnapshot);
    assert!(rejection.to_string().contains("[STALE_SNAPSHOT]"));
    // The target is alive and unchanged: only the snapshot made the difference.
    let current = e.registry().snapshot_id();
    assert!(current > snapshot);
    assert_eq!(e.registry().get(id).expect("live").revision, revision);
    assert_eq!(
        e.check(current, id, revision),
        Ok(Clearance::Simulate { target_id: id })
    );
}

#[given("un ID retirado durante la sesión")]
fn retired_id(e: &mut Engine) {
    e.remembered = e
        .observe(&[button(bx(0.0, 0.0, 10.0, 10.0), "A")])
        .created
        .clone();
    let retired = e.observe(&[]).retired.clone();
    assert_eq!(retired, e.remembered);
}

#[when("aparece otro control en el mismo lugar")]
fn another_control_same_place(e: &mut Engine) {
    // Identical to the retired one: even so, it is another element (OR-38).
    e.observe(&[button(bx(0.0, 0.0, 10.0, 10.0), "A")]);
}

#[then("no se reutiliza el ID retirado")]
fn retired_id_not_reused(e: &mut Engine) {
    let old = e.remembered[0];
    let changes = e.last_changes();
    assert_eq!(changes.created.len(), 1);
    assert_ne!(changes.created[0], old);
    assert!(e.registry().is_retired(old));
    assert!(e.registry().get(old).is_none());
}

// ---- 04_iconos.feature ----

#[given("un descriptor con suficientes gradientes válidos")]
fn descriptor_with_support(e: &mut Engine) {
    e.crop = stripes();
    assert!(descriptor(&e.crop).support() >= SupportPolicy::default().min_support);
}

#[when("se compara con su inversión ideal de luminancia")]
fn compare_with_inversion(e: &mut Engine) {
    // `None` would mean "too little shared support to compare".
    e.distance = descriptor_distance(
        &descriptor(&e.crop),
        &descriptor(&inverted(&e.crop)),
        SupportPolicy::default(),
    )
    .expect("valid policy");
}

#[then("la comparación enmascarada admite distancia cero")]
fn masked_distance_zero(e: &mut Engine) {
    assert_eq!(e.distance, Some(0.0));
}

#[given("un crop sin gradientes informativos")]
fn flat_crop(e: &mut Engine) {
    e.crop = vec![0; GRID_LEN];
    e.cache.insert(curated("Guardar", "editor", &stripes()));
}

#[when("se intenta resolver un icono")]
fn resolve_icon(e: &mut Engine) {
    e.icon = Some(
        e.cache
            .lookup(&descriptor(&e.crop), "editor", cache_policy())
            .expect("valid policy"),
    );
}

#[then("se devuelve unknown por soporte insuficiente")]
fn unknown_insufficient_support(e: &mut Engine) {
    assert_eq!(
        e.icon,
        Some(IconMatch::Unknown(UnknownReason::InsufficientSupport))
    );
    let flat = descriptor(&e.crop);
    assert_eq!(
        descriptor_distance(&flat, &flat, SupportPolicy::default()),
        Ok(None)
    );
}

#[given("dos prototipos semánticos con similitud indistinguible")]
fn two_indistinguishable_prototypes(e: &mut Engine) {
    // Two different meanings curated for the same appearance in the same context.
    e.crop = stripes();
    e.cache.insert(curated("Guardar", "editor", &e.crop));
    e.cache.insert(curated("Eliminar", "editor", &e.crop));
}

#[when("se consulta el caché")]
fn query_cache(e: &mut Engine) {
    resolve_icon(e);
}

#[then("se rechaza la clasificación automática")]
fn classification_rejected(e: &mut Engine) {
    assert_eq!(e.icon, Some(IconMatch::Unknown(UnknownReason::Ambiguous)));
    // Neither prototype overwrote the other.
    assert_eq!(e.cache.prototypes().len(), 2);
}

#[given("un símbolo curado para una aplicación diferente")]
fn symbol_curated_elsewhere(e: &mut Engine) {
    e.crop = stripes();
    e.cache.insert(curated("Guardar", "editor", &e.crop));
}

#[when("se observa en un contexto no validado")]
fn observe_in_other_context(e: &mut Engine) {
    e.icon = Some(
        e.cache
            .lookup(&descriptor(&e.crop), "navegador", cache_policy())
            .expect("valid policy"),
    );
}

#[then("no se asigna su significado como certeza")]
fn meaning_not_assigned(e: &mut Engine) {
    assert_eq!(
        e.icon,
        Some(IconMatch::Unknown(UnknownReason::NoPrototypeForContext))
    );
    // In the context it was curated for, the same crop is known: only the context differs.
    assert_eq!(
        e.cache
            .lookup(&descriptor(&e.crop), "editor", cache_policy()),
        Ok(IconMatch::Known {
            label: "Guardar".to_owned(),
            distance: 0.0
        })
    );
}

fn main() {
    let features = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/thesis/features");
    futures::executor::block_on(Engine::cucumber().fail_on_skipped().filter_run_and_exit(
        features,
        |_, _, scenario| {
            scenario
                .tags
                .iter()
                .any(|tag| IMPLEMENTED.contains(&tag.as_str()))
        },
    ));
}
