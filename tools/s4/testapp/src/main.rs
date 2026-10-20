//! The controlled test application of the S4 reduced acceptance protocol.
//!
//! One maximized window with large Spanish labels and disposable state held only in memory
//! (nothing is read from or written to disk, the network or the clipboard):
//!
//! - a counter from 0 to 9: `Sumar`, `Restar` and `Reiniciar` buttons and the text
//!   `Valor: cero` .. `Valor: nueve`;
//! - a toggle: an `Activar` button and the text `Estado: apagado` / `Estado: encendido`;
//! - a pager: `Anterior` and `Siguiente` buttons and the text `Primera página` ..
//!   `Novena página`;
//! - a text field, named `Campo de texto`: a plain egui `TextEdit` whose contents are
//!   mirrored in the window title, so a `type` step is checked through the OS. Its accessible
//!   name is set on its AccessKit node and not drawn, so UI Automation reports an editable
//!   field and OCR reads no second copy of the name.
//!
//! Every effect of a click is visible text, so an end-to-end run can verify its end state
//! by OCR alone. OCR returns one element per word, so each state is spelt with a word that
//! appears nowhere else in the window (cardinals for the counter, ordinals for the pager):
//! digits would be ambiguous. Nothing moves on its own: the window only repaints after input,
//! except in the hidden measurement mode below.
//!
//! **Hidden `--animate <ms>` flag** (performance measurements, not used by the acceptance
//! protocol): one or two small `Tic nnnn` labels, far apart, whose number advances every
//! `<ms>` milliseconds without any input, so that incremental regions appear by themselves.
//! `--animate-labels <1|2>` picks how many (default 1). `--animate-flash` replaces the labels
//! with a dark veil over the whole window that comes and goes every `<ms>` milliseconds, so
//! that every observation is a full refresh. The flags are not listed anywhere user-visible.
//!
//! **Hidden `--icon-strip` flag** (the icon-cache soak, dossier S0 §19; AG-02 never passes it):
//! a row of eight bare text-less synthetic glyphs painted by code from `tools/s4/icon_spec.rs`, near
//! the bottom of the window, so that the detector has icon candidates for the icon cache.
//! They are drawn on a layer of the window and nothing else: they take no input and expose
//! nothing to UI Automation, and without the flag the window is exactly the same. With the flag
//! the ink of the glyphs advances by a grey level every 100 ms through a cycle of 37 levels, so
//! that between two observations the strip's region has always changed and the planner must
//! re-analyse it (the engine's incrementality is untouched; only the strip changes).

#![windows_subsystem = "windows"]

use std::time::{Duration, Instant};

use eframe::egui;

#[path = "../../icon_spec.rs"]
mod icon_spec;

/// The counter's values, spelt out.
const CARDINALS: [&str; 10] = [
    "cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve",
];
/// The pager's pages, spelt out (the first is page 1).
const ORDINALS: [&str; 9] = [
    "Primera", "Segunda", "Tercera", "Cuarta", "Quinta", "Sexta", "Séptima", "Octava", "Novena",
];
/// The last value of the counter.
const MAX_VALUE: u8 = 9;
/// The last page of the pager.
const LAST_PAGE: u8 = 9;

/// The disposable state of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    value: u8,
    on: bool,
    page: u8,
    text: String,
}

/// The longest text the field takes: a disposable string, far below the contract's limit.
const TEXT_LIMIT: usize = 64;

impl Default for State {
    fn default() -> Self {
        Self {
            value: 0,
            on: false,
            page: 1,
            text: String::new(),
        }
    }
}

/// What a button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Add,
    Subtract,
    Reset,
    Toggle,
    Previous,
    Next,
}

impl State {
    fn apply(&mut self, command: Command) {
        match command {
            Command::Add => self.value = (self.value + 1).min(MAX_VALUE),
            Command::Subtract => self.value = self.value.saturating_sub(1),
            Command::Reset => self.value = 0,
            Command::Toggle => self.on = !self.on,
            Command::Previous => self.page = self.page.saturating_sub(1).max(1),
            Command::Next => self.page = (self.page + 1).min(LAST_PAGE),
        }
    }

    fn value_text(&self) -> String {
        format!("Valor: {}", CARDINALS[usize::from(self.value)])
    }

    fn toggle_text(&self) -> &'static str {
        if self.on {
            "Estado: encendido"
        } else {
            "Estado: apagado"
        }
    }

    fn page_text(&self) -> String {
        format!("{} página", ORDINALS[usize::from(self.page - 1)])
    }

    /// The window title: the whole state in a fixed form, so that an end-to-end run can check
    /// its end state through the OS (`GetWindowTextW`), independently of the engine's OCR.
    /// The text field is mirrored last, as the text the user typed.
    fn title(&self) -> String {
        format!(
            "Ommatix S4 prueba [valor={} estado={} pagina={} texto={}]",
            self.value,
            if self.on { "on" } else { "off" },
            self.page,
            self.text
        )
    }
}

const BUTTON_SIZE: [f32; 2] = [340.0, 110.0];
const BUTTON_TEXT: f32 = 52.0;
const VALUE_TEXT: f32 = 64.0;
const HEADING_TEXT: f32 = 40.0;
/// The vertical gaps of the tallest column at full size: the top margin, after the heading,
/// after the value and between the three buttons.
const GAPS: f32 = 60.0 + 30.0 + 50.0 + 2.0 * 40.0;
/// What the tallest column needs besides its gaps (heading, value and three buttons, with
/// egui's item spacing), rounded up so that the last button never touches the bottom edge.
const CONTENT: f32 = 500.0;

/// How much of each gap fits: 1 when the window is tall enough (100 % and 125 % on a
/// 1920x1080 monitor, so their layout does not change), less when it is short (150 %, where
/// the third button would fall under the taskbar), and never negative. Only the gaps shrink:
/// labels, text sizes and button sizes stay the same, so the engine sees the same controls.
fn gap_scale(available_height: f32) -> f32 {
    ((available_height - CONTENT) / GAPS).clamp(0.0, 1.0)
}

/// The hidden measurement mode: how often the animated labels advance and how many there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Animation {
    period: Duration,
    labels: u8,
    /// A veil over the whole window instead of the labels.
    flash: bool,
}

impl Animation {
    /// Reads `--animate <ms>` and `--animate-labels <1|2>`; `None` without `--animate` or with
    /// a value that is not a positive number of milliseconds.
    fn from_args(args: impl IntoIterator<Item = String>) -> Option<Self> {
        let mut args = args.into_iter();
        let (mut period, mut labels, mut flash) = (None, 1, false);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--animate" => period = args.next().and_then(|v| v.parse::<u64>().ok()),
                "--animate-labels" => {
                    labels = args
                        .next()
                        .and_then(|v| v.parse::<u8>().ok())
                        .unwrap_or(1)
                        .clamp(1, 2);
                }
                "--animate-flash" => flash = true,
                _ => {}
            }
        }
        period.filter(|ms| *ms > 0).map(|ms| Self {
            period: Duration::from_millis(ms),
            labels,
            flash,
        })
    }

    /// The tick number after `elapsed`.
    fn tick(&self, elapsed: Duration) -> u128 {
        elapsed.as_millis() / self.period.as_millis()
    }

    /// The text of the label with the given tick: the second label counts from a different
    /// origin so that the two never read alike.
    fn text(tick: u128, second: bool) -> String {
        format!("Tic {:04}", (tick + if second { 5000 } else { 0 }) % 10_000)
    }
}

struct TestApp {
    state: State,
    animation: Option<Animation>,
    icon_strip: bool,
    started: Instant,
}

impl TestApp {
    fn new(animation: Option<Animation>, icon_strip: bool) -> Self {
        Self {
            state: State::default(),
            animation,
            icon_strip,
            started: Instant::now(),
        }
    }

    /// Draws the animated labels at the top-left and the bottom-right of the window.
    fn animated_labels(&self, ctx: &egui::Context, animation: Animation) {
        let tick = animation.tick(self.started.elapsed());
        let screen = ctx.content_rect();
        if animation.flash {
            let veil = if tick.is_multiple_of(2) { 0 } else { 170 };
            ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("veil"),
            ))
            .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(veil));
        }
        let draw = |id: &str, pos: egui::Pos2, second: bool| {
            egui::Area::new(egui::Id::new(id))
                .fixed_pos(pos)
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(Animation::text(tick, second))
                            .size(HEADING_TEXT)
                            .strong(),
                    );
                });
        };
        if !animation.flash {
            draw("tick-a", screen.min + egui::vec2(24.0, 16.0), false);
        }
        if !animation.flash && animation.labels > 1 {
            draw("tick-b", screen.max - egui::vec2(220.0, 64.0), true);
        }
        // The window never asks the OS for the foreground itself: the toolkit's request sends a
        // synthetic Alt key. The client brings it to the front by its handle (window management,
        // no input) and the test below keeps the request out of this source.
        ctx.request_repaint_after(animation.period);
    }
}

// icon strip: begin
/// Whether the hidden `--icon-strip` flag was given.
fn icon_strip_requested(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|arg| arg == "--icon-strip")
}

/// Points above the window's bottom edge where the strip ends.
const STRIP_BOTTOM_MARGIN: f32 = 64.0;

/// How often the strip's ink advances, in milliseconds, and how many levels it cycles through:
/// one grey level per tick over a prime number of ticks, so that the observations of a soak
/// (every 5 s give or take a second) never see the same level twice in a row and the strip's
/// region is always among the changed ones.
const STRIP_TICK_MS: u128 = 100;
const STRIP_CYCLE: u128 = 37;

/// The ink of every glyph at `tick`: the nominal ink plus the tick's place in the cycle, so
/// between 40 and 76, inside the style range the calibration draws (20 to 90).
fn strip_ink(tick: u128) -> u8 {
    icon_spec::INK_GREY + u8::try_from(tick % STRIP_CYCLE).unwrap_or(0)
}

/// A design coordinate (the spec is in `f64`) as a point; the values are a few hundred, so the
/// narrowing is exact.
#[allow(clippy::cast_possible_truncation)]
fn pt(value: f64) -> f32 {
    value as f32
}

fn grey(level: u8) -> egui::Color32 {
    egui::Color32::from_gray(level)
}

/// The strip's rectangle in a window of `screen`: centred, on whole points, with its bottom
/// [`STRIP_BOTTOM_MARGIN`] above the bottom edge (below the controls, clear of both animated
/// labels).
fn strip_rect(screen: egui::Rect) -> egui::Rect {
    let size = egui::vec2(pt(icon_spec::strip_width()), pt(icon_spec::CELL));
    let x = (screen.center().x - size.x / 2.0).round();
    let y = (screen.max.y - size.y - STRIP_BOTTOM_MARGIN).round();
    egui::Rect::from_min_size(egui::pos2(x, y), size)
}

fn paint_shape(
    painter: &egui::Painter,
    origin: egui::Pos2,
    shape: &icon_spec::Shape,
    ink: egui::Color32,
) {
    let at = |p: (f64, f64)| egui::pos2(origin.x + pt(p.0), origin.y + pt(p.1));
    match *shape {
        icon_spec::Shape::Segment { a, b, width } => {
            painter.line_segment([at(a), at(b)], egui::Stroke::new(pt(width), ink));
            painter.circle_filled(at(a), pt(width / 2.0), ink);
            painter.circle_filled(at(b), pt(width / 2.0), ink);
        }
        icon_spec::Shape::Ring { c, r, width } => {
            painter.circle_stroke(
                at(c),
                pt(r - width / 2.0),
                egui::Stroke::new(pt(width), ink),
            );
        }
        icon_spec::Shape::Disc { c, r } => {
            painter.circle_filled(at(c), pt(r), ink);
        }
        icon_spec::Shape::Rect { x0, y0, x1, y1 } => {
            painter.rect_filled(
                egui::Rect::from_min_max(at((x0, y0)), at((x1, y1))),
                0.0,
                ink,
            );
        }
        icon_spec::Shape::Poly(points) => {
            painter.add(egui::Shape::convex_polygon(
                points.iter().map(|p| at(*p)).collect(),
                ink,
                egui::Stroke::NONE,
            ));
        }
    }
}

/// Paints the eight glyphs on a foreground layer of the window, with no tile or border, as the
/// calibration tool rasterises them but with the ink of `tick`, and asks for the next tick.
fn paint_icon_strip(ctx: &egui::Context, tick: u128) {
    let ink = grey(strip_ink(tick));
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("icon-strip"),
    ));
    let strip = strip_rect(ctx.content_rect());
    let count = u8::try_from(icon_spec::CLASSES).unwrap_or(0);
    for class in 0..count {
        let cell = egui::Rect::from_min_size(
            egui::pos2(
                strip.min.x + f32::from(class) * pt(icon_spec::CELL + icon_spec::GAP),
                strip.min.y,
            ),
            egui::vec2(pt(icon_spec::CELL), pt(icon_spec::CELL)),
        );
        for shape in icon_spec::glyph(usize::from(class)) {
            paint_shape(&painter, cell.min, shape, ink);
        }
    }
    ctx.request_repaint_after(Duration::from_millis(100));
}
// icon strip: end

fn button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add_sized(
        BUTTON_SIZE,
        egui::Button::new(egui::RichText::new(label).size(BUTTON_TEXT).strong()),
    )
    .clicked()
}

fn section(
    ui: &mut egui::Ui,
    heading: &str,
    value: &str,
    buttons: &[(&str, Command)],
    gap: f32,
) -> Option<Command> {
    let mut pressed = None;
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(heading).size(HEADING_TEXT));
        ui.add_space(30.0 * gap);
        ui.label(egui::RichText::new(value).size(VALUE_TEXT).strong());
        ui.add_space(50.0 * gap);
        for (i, &(label, command)) in buttons.iter().enumerate() {
            if i > 0 {
                ui.add_space(40.0 * gap);
            }
            if button(ui, label) {
                pressed = Some(command);
            }
        }
    });
    pressed
}

/// The text field: a plain `TextEdit`. Its accessible name is set on its AccessKit node, so
/// UI Automation reports an editable field named `Campo de texto`, and nothing is drawn for
/// OCR. Returns whether its contents changed.
fn text_field(ui: &mut egui::Ui, text: &mut String, gap: f32) -> bool {
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new("Texto").size(HEADING_TEXT));
        ui.add_space(30.0 * gap);
        let edit = ui.add(
            egui::TextEdit::singleline(text)
                .char_limit(TEXT_LIMIT)
                .desired_width(BUTTON_SIZE[0])
                .font(egui::FontId::proportional(BUTTON_TEXT)),
        );
        ui.ctx()
            .accesskit_node_builder(edit.id, |node| node.set_label("Campo de texto"));
        edit.changed()
    })
    .inner
}

impl eframe::App for TestApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            let gap = gap_scale(ui.available_height());
            ui.add_space(60.0 * gap);
            let value = self.state.value_text();
            let toggle = self.state.toggle_text();
            let page = self.state.page_text();
            let mut pressed = None;
            let mut text_changed = false;
            ui.columns(4, |columns| {
                pressed = pressed.or(section(
                    &mut columns[0],
                    "Contador",
                    &value,
                    &[
                        ("Sumar", Command::Add),
                        ("Restar", Command::Subtract),
                        ("Reiniciar", Command::Reset),
                    ],
                    gap,
                ));
                pressed = pressed.or(section(
                    &mut columns[1],
                    "Interruptor",
                    toggle,
                    &[("Activar", Command::Toggle)],
                    gap,
                ));
                pressed = pressed.or(section(
                    &mut columns[2],
                    "Paginador",
                    &page,
                    &[
                        ("Siguiente", Command::Next),
                        ("Anterior", Command::Previous),
                    ],
                    gap,
                ));
                text_changed = text_field(&mut columns[3], &mut self.state.text, gap);
            });
            if let Some(command) = pressed {
                self.state.apply(command);
            }
            if pressed.is_some() || text_changed {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Title(self.state.title()));
            }
        });
        if let Some(animation) = self.animation {
            self.animated_labels(ui.ctx(), animation);
        }
        if self.icon_strip {
            let tick = self.started.elapsed().as_millis() / STRIP_TICK_MS;
            paint_icon_strip(ui.ctx(), tick);
        }
    }
}

fn main() -> eframe::Result {
    let animation = Animation::from_args(std::env::args().skip(1));
    let icon_strip = icon_strip_requested(std::env::args().skip(1));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(State::default().title())
            .with_maximized(true),
        ..Default::default()
    };
    eframe::run_native(
        "ommatix-testapp",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_theme(egui::Theme::Light);
            Ok(Box::new(TestApp::new(animation, icon_strip)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::gap_scale;

    #[test]
    fn the_layout_keeps_its_full_gaps_when_the_window_is_tall_enough() {
        // 100 % and 125 % on a 1920x1080 monitor leave about 1000 and 770 points.
        assert!((gap_scale(1000.0) - 1.0).abs() < f32::EPSILON);
        assert!((gap_scale(770.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_gaps_shrink_so_the_third_button_fits_at_150_percent() {
        // 150 %: about 625 points of client area; the column must fit in it.
        let available = 625.0;
        let gap = gap_scale(available);
        assert!(gap < 1.0);
        assert!(super::CONTENT + super::GAPS * gap <= available + 0.01);
    }

    #[test]
    fn the_gaps_never_go_negative() {
        assert!(gap_scale(100.0).abs() < f32::EPSILON);
    }
    use super::*;

    /// The foreground and the keyboard focus are never requested through the toolkit: its window
    /// focus command sends a synthetic Alt key through `SendInput` when the window is not in
    /// front, and so does creating a window in the toolkit's exclusive-screen mode. That is input
    /// outside the engine's gate (AGENTS.md rule 6). The forbidden names are built from pieces so
    /// that this test does not contain them.
    #[test]
    fn the_source_never_asks_the_toolkit_for_the_foreground() {
        let source = include_str!("main.rs");
        for forbidden in [
            ["ViewportCommand", "::Focus"].concat(),
            ["focus", "_window"].concat(),
            ["Full", "screen"].concat(),
            ["full", "screen"].concat(),
        ] {
            assert!(
                !source.contains(&forbidden),
                "the source contains {forbidden}"
            );
        }
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn nothing_animates_without_the_hidden_flag() {
        assert_eq!(Animation::from_args(args(&[])), None);
        assert_eq!(Animation::from_args(args(&["--animate-labels", "2"])), None);
        assert_eq!(Animation::from_args(args(&["--animate", "0"])), None);
        assert_eq!(Animation::from_args(args(&["--animate", "x"])), None);
    }

    #[test]
    fn the_animation_reads_its_period_and_label_count() {
        let a = Animation::from_args(args(&["--animate", "80"])).unwrap();
        assert_eq!((a.period, a.labels), (Duration::from_millis(80), 1));
        let b = Animation::from_args(args(&["--animate", "40", "--animate-labels", "2"])).unwrap();
        assert_eq!((b.period, b.labels), (Duration::from_millis(40), 2));
        let c = Animation::from_args(args(&["--animate", "40", "--animate-labels", "9"])).unwrap();
        assert_eq!(c.labels, 2);
    }

    #[test]
    fn the_icon_strip_is_off_without_its_hidden_flag() {
        assert!(!icon_strip_requested(args(&[])));
        assert!(!icon_strip_requested(args(&["--animate", "100"])));
        assert!(!icon_strip_requested(args(&["--icon-strips"])));
        assert!(icon_strip_requested(args(&[
            "--animate",
            "100",
            "--icon-strip"
        ])));
    }

    #[test]
    fn the_icon_strip_has_one_cell_per_class_clear_of_the_controls_and_the_labels() {
        // The window of the 100 % profile: 1920 points wide and about 1000 high.
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1920.0, 1000.0));
        let strip = strip_rect(screen);
        assert!(screen.contains_rect(strip));
        assert!((strip.width() - pt(icon_spec::strip_width())).abs() < f32::EPSILON);
        assert!((strip.center().x - screen.center().x).abs() <= 0.5);
        // Whole points, so that the tiles start on whole pixels at 100 %.
        assert!((strip.min.x - strip.min.x.round()).abs() < f32::EPSILON);
        assert!((strip.min.y - strip.min.y.round()).abs() < f32::EPSILON);
        // The tallest column of controls ends near 560 points; the animated labels sit at the
        // top left and the bottom right.
        assert!(strip.min.y > super::CONTENT + 60.0 + 100.0);
        let tick_a =
            egui::Rect::from_min_size(screen.min + egui::vec2(24.0, 16.0), egui::vec2(220.0, 60.0));
        let tick_b = egui::Rect::from_min_size(
            screen.max - egui::vec2(220.0, 64.0),
            egui::vec2(200.0, 60.0),
        );
        assert!(!strip.intersects(tick_a) && !strip.intersects(tick_b));
        assert_eq!(icon_spec::CLASSES, 8);
        for class in 0..icon_spec::CLASSES {
            assert!(!icon_spec::glyph(class).is_empty(), "class {class}");
        }
    }

    /// The strip is painted on a layer and nothing else: no widget, no input, no accessibility
    /// node. The forbidden names are built from pieces so that this test does not contain them.
    /// R105-2: the strip changes between two consecutive observations of a soak, so that the
    /// planner always has to re-analyse its region and the resolver re-describes every glyph;
    /// a static strip would be crossed once and then kept as it was. The ink advances by one
    /// grey level every tick through a cycle of 37 ticks (a prime), and the soak observes every
    /// 5 seconds give or take a second, so no spacing between 4.0 and 6.5 s lands on the same
    /// level.
    #[test]
    fn the_icon_strip_changes_between_consecutive_observations() {
        for tick in 0..STRIP_CYCLE {
            assert!(strip_ink(tick) >= icon_spec::INK_GREY);
            assert!(
                strip_ink(tick) <= 90,
                "inside the calibrated style range (20 to 90)"
            );
        }
        for spacing_ms in (4000..=6500).step_by(100) {
            for start in 0..STRIP_CYCLE {
                assert_ne!(
                    strip_ink(start),
                    strip_ink(start + spacing_ms / STRIP_TICK_MS),
                    "start tick {start}, spacing {spacing_ms} ms"
                );
            }
        }
        // The ink never reaches the 255 levels of contrast that the measurement configuration
        // would need for a valid gradient against the window.
        assert!(
            u16::from(icon_spec::WINDOW_GREY) - u16::from(strip_ink(0)) < 255,
            "a glyph stays far from pure black on pure white"
        );
    }

    #[test]
    fn the_icon_strip_takes_no_input_and_exposes_nothing() {
        let source = include_str!("main.rs");
        let begin = source.find("// icon strip: begin").unwrap();
        let end = source.find("// icon strip: end").unwrap();
        let section = &source[begin..end];
        assert!(section.contains("layer_painter"));
        for forbidden in [
            ["Sen", "se"].concat(),
            ["Bu", "tton"].concat(),
            ["access", "kit"].concat(),
            ["inter", "act"].concat(),
            ["clic", "k"].concat(),
            ["Ui", "::"].concat(),
        ] {
            assert!(
                !section.contains(&forbidden),
                "the icon strip section contains {forbidden}"
            );
        }
    }

    #[test]
    fn the_flash_mode_is_a_separate_hidden_flag() {
        let a = Animation::from_args(args(&["--animate", "250", "--animate-flash"])).unwrap();
        assert!(a.flash);
        let b = Animation::from_args(args(&["--animate", "250"])).unwrap();
        assert!(!b.flash);
        assert_eq!(Animation::from_args(args(&["--animate-flash"])), None);
    }

    #[test]
    fn the_animated_labels_advance_with_time_and_never_read_alike() {
        let a = Animation::from_args(args(&["--animate", "50"])).unwrap();
        assert_eq!(a.tick(Duration::from_millis(49)), 0);
        assert_eq!(a.tick(Duration::from_millis(50)), 1);
        assert_ne!(Animation::text(3, false), Animation::text(4, false));
        assert_ne!(Animation::text(3, false), Animation::text(3, true));
        assert_eq!(Animation::text(9_999, false), "Tic 9999");
        assert_eq!(Animation::text(10_000, false), "Tic 0000");
    }

    #[test]
    fn the_counter_adds_subtracts_and_resets() {
        let mut s = State::default();
        assert_eq!(s.value_text(), "Valor: cero");
        s.apply(Command::Subtract);
        assert_eq!(s.value_text(), "Valor: cero");
        for _ in 0..3 {
            s.apply(Command::Add);
        }
        assert_eq!(s.value_text(), "Valor: tres");
        s.apply(Command::Subtract);
        assert_eq!(s.value_text(), "Valor: dos");
        for _ in 0..20 {
            s.apply(Command::Add);
        }
        assert_eq!(s.value_text(), "Valor: nueve");
        s.apply(Command::Reset);
        assert_eq!(s.value_text(), "Valor: cero");
    }

    #[test]
    fn the_toggle_flips_its_visible_state() {
        let mut s = State::default();
        assert_eq!(s.toggle_text(), "Estado: apagado");
        s.apply(Command::Toggle);
        assert_eq!(s.toggle_text(), "Estado: encendido");
        s.apply(Command::Toggle);
        assert_eq!(s.toggle_text(), "Estado: apagado");
    }

    #[test]
    fn the_pager_stays_between_the_first_and_the_last_page() {
        let mut s = State::default();
        assert_eq!(s.page_text(), "Primera página");
        s.apply(Command::Previous);
        assert_eq!(s.page_text(), "Primera página");
        s.apply(Command::Next);
        assert_eq!(s.page_text(), "Segunda página");
        for _ in 0..20 {
            s.apply(Command::Next);
        }
        assert_eq!(s.page_text(), "Novena página");
    }

    #[test]
    fn every_state_word_is_distinct() {
        let mut words: Vec<String> = CARDINALS
            .iter()
            .chain(ORDINALS.iter())
            .chain(["apagado", "encendido"].iter())
            .map(|w| w.to_lowercase())
            .collect();
        let n = words.len();
        words.sort();
        words.dedup();
        assert_eq!(words.len(), n);
    }

    #[test]
    fn the_title_carries_the_whole_state() {
        let mut s = State::default();
        assert_eq!(
            s.title(),
            "Ommatix S4 prueba [valor=0 estado=off pagina=1 texto=]"
        );
        s.apply(Command::Add);
        s.apply(Command::Toggle);
        s.apply(Command::Next);
        assert_eq!(
            s.title(),
            "Ommatix S4 prueba [valor=1 estado=on pagina=2 texto=]"
        );
    }

    #[test]
    fn the_title_mirrors_the_text_field() {
        let s = State {
            text: "prueba 123".to_owned(),
            ..State::default()
        };
        assert_eq!(
            s.title(),
            "Ommatix S4 prueba [valor=0 estado=off pagina=1 texto=prueba 123]"
        );
    }
}
