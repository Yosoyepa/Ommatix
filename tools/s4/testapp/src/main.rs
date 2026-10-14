//! The controlled test application of the S4 reduced acceptance protocol.
//!
//! One maximized window with large Spanish labels and disposable state held only in memory
//! (nothing is read from or written to disk, the network or the clipboard):
//!
//! - a counter from 0 to 9: `Sumar`, `Restar` and `Reiniciar` buttons and the text
//!   `Valor: cero` .. `Valor: nueve`;
//! - a toggle: an `Activar` button and the text `Estado: apagado` / `Estado: encendido`;
//! - a pager: `Anterior` and `Siguiente` buttons and the text `Primera página` ..
//!   `Novena página`.
//!
//! Every effect of a click is visible text, so an end-to-end run can verify its end state
//! by OCR alone. OCR returns one element per word, so each state is spelt with a word that
//! appears nowhere else in the window (cardinals for the counter, ordinals for the pager):
//! digits would be ambiguous. The window has no text field, so the protocol needs no `type`. Nothing
//! moves on its own: the window only repaints after input.

#![windows_subsystem = "windows"]

use eframe::egui;

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
}

impl Default for State {
    fn default() -> Self {
        Self {
            value: 0,
            on: false,
            page: 1,
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

    /// The window title: the whole state in a fixed ASCII form, so that an end-to-end run
    /// can check its end state through the OS (`GetWindowTextW`), independently of the
    /// engine's OCR.
    fn title(&self) -> String {
        format!(
            "Ommatix S4 prueba [valor={} estado={} pagina={}]",
            self.value,
            if self.on { "on" } else { "off" },
            self.page
        )
    }
}

const BUTTON_SIZE: [f32; 2] = [340.0, 110.0];
const BUTTON_TEXT: f32 = 52.0;
const VALUE_TEXT: f32 = 64.0;
const HEADING_TEXT: f32 = 40.0;

#[derive(Debug, Default)]
struct TestApp {
    state: State,
}

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
) -> Option<Command> {
    let mut pressed = None;
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(heading).size(HEADING_TEXT));
        ui.add_space(30.0);
        ui.label(egui::RichText::new(value).size(VALUE_TEXT).strong());
        ui.add_space(50.0);
        for &(label, command) in buttons {
            if button(ui, label) {
                pressed = Some(command);
            }
            ui.add_space(40.0);
        }
    });
    pressed
}

impl eframe::App for TestApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(60.0);
            let mut pressed = None;
            ui.columns(3, |columns| {
                pressed = pressed.or(section(
                    &mut columns[0],
                    "Contador",
                    &self.state.value_text(),
                    &[
                        ("Sumar", Command::Add),
                        ("Restar", Command::Subtract),
                        ("Reiniciar", Command::Reset),
                    ],
                ));
                pressed = pressed.or(section(
                    &mut columns[1],
                    "Interruptor",
                    self.state.toggle_text(),
                    &[("Activar", Command::Toggle)],
                ));
                pressed = pressed.or(section(
                    &mut columns[2],
                    "Paginador",
                    &self.state.page_text(),
                    &[
                        ("Siguiente", Command::Next),
                        ("Anterior", Command::Previous),
                    ],
                ));
            });
            if let Some(command) = pressed {
                self.state.apply(command);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Title(self.state.title()));
            }
        });
    }
}

fn main() -> eframe::Result {
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
            Ok(Box::new(TestApp::default()))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(s.title(), "Ommatix S4 prueba [valor=0 estado=off pagina=1]");
        s.apply(Command::Add);
        s.apply(Command::Toggle);
        s.apply(Command::Next);
        assert_eq!(s.title(), "Ommatix S4 prueba [valor=1 estado=on pagina=2]");
    }
}
