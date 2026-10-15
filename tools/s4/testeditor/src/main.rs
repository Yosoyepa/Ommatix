//! The controlled editor of the dossier's E2E-02 ("editor: type and save a temporary file").
//!
//! One maximized window with large Spanish labels:
//!
//! - a multi-line text field, named `Editor de texto` (a plain egui `TextEdit` whose
//!   accessible name is set on its AccessKit node and not drawn, so UI Automation reports an
//!   editable field and OCR reads no second copy of the name);
//! - a `Guardar` button that writes the field's text, byte for byte, to the file `nota.txt`
//!   of the folder the client passed with `--folder`, and nowhere else;
//! - a status line: `Estado: pendiente`, `Estado: guardado` or `Estado: fallido`, each state
//!   a word that appears nowhere else in the window.
//!
//! **The save path is confined** ([`SaveFolder`]): the folder must be, once resolved,
//! exactly `<temporary directory>\ommatix-e2e\<run id>` (a run id of 1 to 64 letters, digits,
//! `.`, `_` or `-`), the client's own folder for the run. The file name is fixed, so the
//! application cannot be pointed at any other file, and it refuses to write when the target
//! is not a regular file (a link, a directory). With a folder that does not fit it exits with
//! code 2 before opening a window. Nothing else is read from or written to disk, the network
//! or the clipboard.
//!
//! The window title mirrors the state in a fixed form (`caracteres=N guardado=no|si`, the
//! number of characters in the field and whether the last save is current), so a run can
//! check the typing through the OS (`GetWindowTextW`). The independent check of a save is the
//! file itself, read by the client from the OS.

#![windows_subsystem = "windows"]

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};

use eframe::egui;

/// The only file the editor writes, in the client's folder.
const NOTE_FILE: &str = "nota.txt";
/// The client's directory for its runs, directly under the system's temporary directory.
const RUN_ROOT: &str = "ommatix-e2e";
/// The longest run id (the folder's name) accepted.
const RUN_ID_LIMIT: usize = 64;
/// The longest text the field takes: a disposable string, far below any real document.
const TEXT_LIMIT_CHARS: usize = 1024;
/// The largest file the editor writes (a character is at most 4 bytes of UTF-8).
const MAX_NOTE_BYTES: usize = 4 * TEXT_LIMIT_CHARS;

/// Why a folder is not a folder the editor saves into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderError {
    NotAbsolute,
    /// It, or the temporary directory, cannot be resolved or is not a directory.
    Unresolvable,
    /// Resolved, it lies outside the temporary directory.
    OutsideTemp,
    /// Resolved, it is not `<temp>\ommatix-e2e\<run id>`.
    WrongShape,
    BadRunId,
}

impl fmt::Display for FolderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotAbsolute => "the folder is not an absolute path",
            Self::Unresolvable => "the folder cannot be resolved or is not a directory",
            Self::OutsideTemp => "the folder is outside the temporary directory",
            Self::WrongShape => "the folder is not <temporary directory>/ommatix-e2e/<run id>",
            Self::BadRunId => "the run id is not 1 to 64 letters, digits, '.', '_' or '-'",
        })
    }
}

/// Why a save did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveError {
    TooLong,
    /// The folder is no longer the one that was validated.
    FolderChanged,
    /// The target exists and is not a regular file.
    NotAFile,
    Io,
}

/// Whether `run` is a run id: 1 to 64 letters, digits, `.`, `_` or `-`, and not all dots.
fn valid_run_id(run: &str) -> bool {
    (1..=RUN_ID_LIMIT).contains(&run.len())
        && run
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !run.chars().all(|c| c == '.')
}

/// The one folder the editor writes into, resolved and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SaveFolder {
    /// The resolved path (no links, no `..`).
    dir: PathBuf,
}

impl SaveFolder {
    /// Accepts `dir` only when it resolves to `<temp_root>/ommatix-e2e/<run id>`, a directory
    /// that exists. Everything is checked on the resolved paths, so `..` and links (symbolic
    /// links, junctions) that lead elsewhere are refused.
    fn new(dir: &Path, temp_root: &Path) -> Result<Self, FolderError> {
        if !dir.is_absolute() {
            return Err(FolderError::NotAbsolute);
        }
        let root = std::fs::canonicalize(temp_root).map_err(|_| FolderError::Unresolvable)?;
        let resolved = std::fs::canonicalize(dir).map_err(|_| FolderError::Unresolvable)?;
        if !resolved.is_dir() {
            return Err(FolderError::Unresolvable);
        }
        let relative = resolved
            .strip_prefix(&root)
            .map_err(|_| FolderError::OutsideTemp)?;
        let names: Option<Vec<&OsStr>> = relative
            .components()
            .map(|c| match c {
                Component::Normal(name) => Some(name),
                _ => None,
            })
            .collect();
        let Some([root_dir, run]) = names.as_deref() else {
            return Err(FolderError::WrongShape);
        };
        if *root_dir != OsStr::new(RUN_ROOT) {
            return Err(FolderError::WrongShape);
        }
        if !run.to_str().is_some_and(valid_run_id) {
            return Err(FolderError::BadRunId);
        }
        Ok(Self { dir: resolved })
    }

    /// The file the editor writes: the fixed name in the folder, nothing else.
    fn note_path(&self) -> PathBuf {
        self.dir.join(NOTE_FILE)
    }

    /// Writes `text` to the note file, byte for byte (UTF-8, no byte order mark, no line
    /// ending conversion), replacing what a previous save wrote. Returns the bytes written.
    fn save(&self, text: &str) -> Result<usize, SaveError> {
        if text.len() > MAX_NOTE_BYTES {
            return Err(SaveError::TooLong);
        }
        // The folder must still be the one that was validated, not a link made since.
        if std::fs::canonicalize(&self.dir).ok().as_deref() != Some(self.dir.as_path()) {
            return Err(SaveError::FolderChanged);
        }
        let path = self.note_path();
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if !meta.is_file() => return Err(SaveError::NotAFile),
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(_) => return Err(SaveError::Io),
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .map_err(|_| SaveError::Io)?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|_| SaveError::Io)?;
        Ok(text.len())
    }
}

/// The folder of `--folder <path>`, when the arguments are exactly that.
fn folder_arg(args: impl IntoIterator<Item = OsString>) -> Option<PathBuf> {
    let mut args = args.into_iter();
    let flag = args.next()?;
    let folder = args.next()?;
    (flag == "--folder" && args.next().is_none()).then(|| PathBuf::from(folder))
}

/// What the status line says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Pending,
    Saved,
    Failed,
}

/// The editor's state: the field's text and whether it was saved.
#[derive(Debug)]
struct Editor {
    folder: SaveFolder,
    text: String,
    status: Status,
}

impl Editor {
    fn new(folder: SaveFolder) -> Self {
        Self {
            folder,
            text: String::new(),
            status: Status::Pending,
        }
    }

    /// The field changed: what was saved is no longer what the field holds.
    fn edited(&mut self) {
        self.status = Status::Pending;
    }

    fn save(&mut self) {
        self.status = match self.folder.save(&self.text) {
            Ok(_) => Status::Saved,
            Err(_) => Status::Failed,
        };
    }

    /// Each state is a word that appears nowhere else in the window (OCR returns a word per
    /// element, and `Guardar` is the button).
    fn status_text(&self) -> &'static str {
        match self.status {
            Status::Pending => "Estado: pendiente",
            Status::Saved => "Estado: guardado",
            Status::Failed => "Estado: fallido",
        }
    }

    /// The window title: the typed length and whether the last save is current.
    fn title(&self) -> String {
        format!(
            "Ommatix S4 editor [caracteres={} guardado={}]",
            self.text.chars().count(),
            if self.status == Status::Saved {
                "si"
            } else {
                "no"
            }
        )
    }
}

const BUTTON_SIZE: [f32; 2] = [340.0, 110.0];
const BUTTON_TEXT: f32 = 52.0;
const VALUE_TEXT: f32 = 64.0;
const HEADING_TEXT: f32 = 40.0;
const FIELD_WIDTH: f32 = 900.0;
const FIELD_ROWS: usize = 6;

struct EditorApp {
    editor: Editor,
}

impl eframe::App for EditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let before = self.editor.title();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Editor").size(HEADING_TEXT));
                ui.add_space(30.0);
                let field = ui.add(
                    egui::TextEdit::multiline(&mut self.editor.text)
                        .char_limit(TEXT_LIMIT_CHARS)
                        .desired_rows(FIELD_ROWS)
                        .desired_width(FIELD_WIDTH)
                        .font(egui::FontId::proportional(BUTTON_TEXT)),
                );
                ui.ctx()
                    .accesskit_node_builder(field.id, |node| node.set_label("Editor de texto"));
                if field.changed() {
                    self.editor.edited();
                }
                ui.add_space(40.0);
                let save = ui.add_sized(
                    BUTTON_SIZE,
                    egui::Button::new(egui::RichText::new("Guardar").size(BUTTON_TEXT).strong()),
                );
                if save.clicked() {
                    self.editor.save();
                }
                ui.add_space(40.0);
                ui.label(
                    egui::RichText::new(self.editor.status_text())
                        .size(VALUE_TEXT)
                        .strong(),
                );
            });
        });
        let after = self.editor.title();
        if after != before {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(after));
        }
    }
}

fn main() -> eframe::Result {
    // Exit code 2, before any window, for anything but `--folder <the client's folder>`.
    let Some(dir) = folder_arg(std::env::args_os().skip(1)) else {
        std::process::exit(2);
    };
    let Ok(folder) = SaveFolder::new(&dir, &std::env::temp_dir()) else {
        std::process::exit(2);
    };
    let editor = Editor::new(folder);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(editor.title())
            .with_maximized(true),
        ..Default::default()
    };
    eframe::run_native(
        "ommatix-testeditor",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_theme(egui::Theme::Light);
            Ok(Box::new(EditorApp { editor }))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh directory standing for the system's temporary directory.
    fn temp_root() -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "ommatix-testeditor-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// `<root>/ommatix-e2e/<run>`, created.
    fn run_dir(root: &Path, run: &str) -> PathBuf {
        let dir = root.join(RUN_ROOT).join(run);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Every file under `root`, as `a/b/c` relative to it, sorted.
    fn files_under(root: &Path) -> Vec<String> {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else {
                    let relative = path.strip_prefix(root).unwrap();
                    let parts: Vec<_> = relative.iter().map(|p| p.to_string_lossy()).collect();
                    out.push(parts.join("/"));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out.sort();
        out
    }

    fn folder(root: &Path, run: &str) -> SaveFolder {
        SaveFolder::new(&run_dir(root, run), root).unwrap()
    }

    #[test]
    fn the_folder_argument_is_exactly_folder_and_a_path() {
        let args = |list: &[&str]| list.iter().map(OsString::from).collect::<Vec<_>>();
        assert_eq!(
            folder_arg(args(&["--folder", "C:\\t"])),
            Some(PathBuf::from("C:\\t"))
        );
        for bad in [
            &[][..],
            &["--folder"][..],
            &["C:\\t"][..],
            &["--dir", "C:\\t"][..],
            &["--folder", "C:\\t", "--folder", "D:\\u"][..],
            &["--folder", "C:\\t", "extra"][..],
        ] {
            assert_eq!(folder_arg(args(bad)), None, "{bad:?}");
        }
    }

    #[test]
    fn the_clients_run_folder_is_accepted() {
        let root = temp_root();
        let dir = run_dir(&root, "s4-e2e02-20261014-1830");
        let accepted = SaveFolder::new(&dir, &root).unwrap();
        assert!(
            accepted.note_path().ends_with(
                Path::new(RUN_ROOT)
                    .join("s4-e2e02-20261014-1830")
                    .join(NOTE_FILE)
            )
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_outside_the_clients_run_root_is_refused() {
        let root = temp_root();
        let _ = run_dir(&root, "run1");
        std::fs::create_dir_all(root.join("other").join("run1")).unwrap();
        std::fs::create_dir_all(root.join(RUN_ROOT).join("run1").join("deeper")).unwrap();
        let outside = temp_root();
        let cases: Vec<(PathBuf, FolderError)> = vec![
            (root.join("other").join("run1"), FolderError::WrongShape),
            (root.clone(), FolderError::WrongShape),
            (root.join(RUN_ROOT), FolderError::WrongShape),
            (
                root.join(RUN_ROOT).join("run1").join("deeper"),
                FolderError::WrongShape,
            ),
            (outside.clone(), FolderError::OutsideTemp),
            (PathBuf::from("run1"), FolderError::NotAbsolute),
            (
                root.join(RUN_ROOT).join("missing"),
                FolderError::Unresolvable,
            ),
        ];
        for (dir, expected) in cases {
            assert_eq!(SaveFolder::new(&dir, &root), Err(expected), "{dir:?}");
        }
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn dot_dot_cannot_climb_out_of_the_run_folder() {
        let root = temp_root();
        let dir = run_dir(&root, "run1");
        std::fs::create_dir_all(root.join("victim")).unwrap();
        let climbing = dir.join("..").join("..").join("victim");
        assert_eq!(
            SaveFolder::new(&climbing, &root),
            Err(FolderError::WrongShape)
        );
        let beyond = dir.join("..").join("..").join("..");
        assert!(SaveFolder::new(&beyond, &root).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_is_not_a_folder() {
        let root = temp_root();
        let dir = run_dir(&root, "run1");
        let file = root.join(RUN_ROOT).join("run2");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(
            SaveFolder::new(&file, &root),
            Err(FolderError::Unresolvable)
        );
        drop(dir);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn run_ids_are_plain_names() {
        for ok in ["a", "run-1", "s4_e2e02.20261014", &"x".repeat(RUN_ID_LIMIT)] {
            assert!(valid_run_id(ok), "{ok}");
        }
        for bad in [
            "",
            ".",
            "..",
            "a b",
            "a/b",
            "a\\b",
            "é",
            "a:b",
            &"x".repeat(RUN_ID_LIMIT + 1),
        ] {
            assert!(!valid_run_id(bad), "{bad:?}");
        }
        let root = temp_root();
        let odd = root.join(RUN_ROOT).join("a b");
        std::fs::create_dir_all(&odd).unwrap();
        assert_eq!(SaveFolder::new(&odd, &root), Err(FolderError::BadRunId));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The editor writes exactly what was typed, to exactly one file of its folder.
    #[test]
    fn a_save_writes_the_typed_bytes_to_the_note_file_and_nothing_else() {
        let root = temp_root();
        let target = folder(&root, "run1");
        let bystander = root.join(RUN_ROOT).join("run1").join("keep.txt");
        std::fs::write(&bystander, b"keep").unwrap();
        let sibling = run_dir(&root, "run2");
        std::fs::write(sibling.join(NOTE_FILE), b"sibling").unwrap();
        let before = files_under(&root);

        assert_eq!(target.save("prueba 123"), Ok(10));

        let note = target.note_path();
        assert_eq!(std::fs::read(&note).unwrap(), b"prueba 123");
        let mut expected = before;
        expected.push("ommatix-e2e/run1/nota.txt".to_owned());
        expected.sort();
        assert_eq!(files_under(&root), expected, "one file was added");
        assert_eq!(std::fs::read(&bystander).unwrap(), b"keep");
        assert_eq!(std::fs::read(sibling.join(NOTE_FILE)).unwrap(), b"sibling");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_bytes_are_utf8_without_a_bom_and_line_endings_are_kept() {
        let root = temp_root();
        let target = folder(&root, "run1");
        let text = "ñandú\nsegunda línea\r\ntercera";
        assert_eq!(target.save(text), Ok(text.len()));
        let bytes = std::fs::read(target.note_path()).unwrap();
        assert_eq!(bytes, text.as_bytes());
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        assert_eq!(bytes.split(|b| *b == b'\n').count(), 3, "two line feeds");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_second_save_replaces_the_first_whole() {
        let root = temp_root();
        let target = folder(&root, "run1");
        target.save("una línea larga de texto").unwrap();
        target.save("corta").unwrap();
        assert_eq!(std::fs::read(target.note_path()).unwrap(), b"corta");
        target.save("").unwrap();
        assert_eq!(std::fs::read(target.note_path()).unwrap(), b"");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_text_over_the_limit_is_not_written() {
        let root = temp_root();
        let target = folder(&root, "run1");
        assert_eq!(
            target.save(&"x".repeat(MAX_NOTE_BYTES + 1)),
            Err(SaveError::TooLong)
        );
        assert!(!target.note_path().exists(), "nothing was created");
        assert_eq!(target.save(&"x".repeat(MAX_NOTE_BYTES)), Ok(MAX_NOTE_BYTES));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_target_that_is_not_a_regular_file_is_not_written() {
        let root = temp_root();
        let target = folder(&root, "run1");
        std::fs::create_dir(target.note_path()).unwrap();
        assert_eq!(target.save("hola"), Err(SaveError::NotAFile));
        assert!(target.note_path().is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A link to a file elsewhere is not followed (creating one needs a privilege on
    /// Windows, so the test is skipped where it is missing).
    #[test]
    fn a_link_at_the_note_path_is_not_followed() {
        let root = temp_root();
        let target = folder(&root, "run1");
        let victim = root.join("victim.txt");
        std::fs::write(&victim, b"untouched").unwrap();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&victim, target.note_path());
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&victim, target.note_path());
        if linked.is_ok() {
            assert_eq!(target.save("hola"), Err(SaveError::NotAFile));
            assert_eq!(std::fs::read(&victim).unwrap(), b"untouched");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_replaced_after_validation_is_not_written() {
        let root = temp_root();
        let target = folder(&root, "run1");
        std::fs::remove_dir_all(&target.dir).unwrap();
        assert_eq!(target.save("hola"), Err(SaveError::FolderChanged));
        assert!(!target.dir.exists(), "the folder was not recreated");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_status_and_the_title_follow_the_state() {
        let root = temp_root();
        let mut editor = Editor::new(folder(&root, "run1"));
        assert_eq!(editor.status_text(), "Estado: pendiente");
        assert_eq!(
            editor.title(),
            "Ommatix S4 editor [caracteres=0 guardado=no]"
        );
        editor.text = "prueba 123".to_owned();
        editor.edited();
        assert_eq!(
            editor.title(),
            "Ommatix S4 editor [caracteres=10 guardado=no]"
        );
        editor.save();
        assert_eq!(editor.status_text(), "Estado: guardado");
        assert_eq!(
            editor.title(),
            "Ommatix S4 editor [caracteres=10 guardado=si]"
        );
        editor.text.push('!');
        editor.edited();
        assert_eq!(editor.status_text(), "Estado: pendiente");
        assert!(editor.title().ends_with("guardado=no]"));
        std::fs::remove_dir_all(&editor.folder.dir).unwrap();
        editor.save();
        assert_eq!(editor.status_text(), "Estado: fallido");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_state_word_is_distinct_from_the_button() {
        let words = ["pendiente", "guardado", "fallido", "Guardar", "Editor"];
        let mut folded: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
        let n = folded.len();
        folded.sort();
        folded.dedup();
        assert_eq!(folded.len(), n);
    }
}
