#!/usr/bin/env python3
"""S4 supervised runs of the dossier's E2E-02 (editor) and E2E-03 (Explorer).

``matrices/plan_pruebas.csv`` defines E2E-02 as an editor that types and saves a temporary
file and E2E-03 as a file explorer that navigates test folders without destroying anything
(the reduced protocol of ``e2e_driver.py`` used those labels for other applications; its
results say ``E2E-02-plan`` and ``E2E-03-plan`` here). This client runs those two flows
through the same minimal MCP client, the same gate and the same grounding as
``e2e_driver.py``, whose ``Protocol`` it extends: this file adds the two applications, the
temporary tree each run creates, and the OS-side checks. ``e2e_driver.py`` is imported
unchanged.

``--app editor``    ``tools/s4/testeditor`` (``ommatix-testeditor.exe``), a separate binary so
                    that RF-18 counts distinct applications. The client creates
                    ``%TEMP%\\ommatix-e2e\\<run id>``, starts the editor with ``--folder`` on it,
                    ``type``s the synthetic text ``prueba 123`` into ``Editor de texto`` (the
                    engine's two-phase ``type``, ADR-0018, makes the focusing click itself) and
                    clicks ``Guardar``. The OS-side checks are the window title after the typing
                    (``caracteres=10 guardado=no``, no file yet) and, after the save, the folder:
                    exactly one file, ``nota.txt``, whose bytes equal the typed text.
``--app explorer``  ``explorer.exe``, but only in the one window this client opens. The client
                    creates ``%TEMP%\\ommatix-e2e\\<run id>`` with ``carpeta-a\\carpeta-b`` and
                    dummy files, opens ONE File Explorer window directly on ``carpeta-b``,
                    records its handle and writes it to the operator's pin file
                    (``[safety] window_pin_file``, ADR-0020). Three single clicks, all in the
                    window's address bar and toolbar: the breadcrumb ``carpeta-a`` (up), the
                    breadcrumb of the root folder (up), then the Back button (``Atrás``), which
                    returns to ``carpeta-a``. Targets are told apart by their UI Automation
                    control type (breadcrumb Button/SplitButton, Back Button) and the engine's
                    role for it, and identified by RuntimeId; the engine's zone is only recorded
                    (it was ``unknown`` for the navigation targets in the ninth round). No double
                    click, delete, rename, drag, context menu, shortcut or typing exists in the
                    flow. After every click the window's current folder
                    (``Shell.Application``, read-only) must be the expected one; at the end the
                    tree must hold the same files and sizes. The window is closed by its handle (``WM_CLOSE``, never by
                    killing ``explorer.exe``) and only the tree this run created is deleted.
``--app decoy``     BDD-35 on the real actuator: ``ommatix-testeditor.exe`` started with
                    ``--decoy-focus``, whose ``Campo auxiliar`` hands the keyboard focus to ``Campo de
                    destino`` when clicked. One ``type`` of the synthetic text into ``Campo auxiliar``
                    goes through the gate. A ``STALE_SNAPSHOT`` refusal (nothing delivered) is
                    observed again and sent as a new request, at most three attempts. Before each
                    live request the editor's window must be in front, hold the keyboard focus and not
                    be minimized (read only); otherwise nothing is sent and the operator is asked, a
                    bounded number of times, to give it the focus by hand. The engine
                    must report ``indeterminate`` (the focusing click
                    delivered, ``verified`` false, ``INDETERMINATE_EFFECT``) with no key sent; the same
                    request_id repeated must return that stored result with no new delivery; and UI
                    Automation must place the keyboard focus on ``Campo de destino`` (compared by
                    RuntimeId in memory), so that a focus never moved cannot pass the case. The OS-side
                    checks are the three lengths in the title (all 0) and the empty folder. A key that
                    reached a field is the safety event ``keys_without_focus``: the invocation ends.
                    The precondition is read before anything else: UI Automation must report the
                    keyboard focus inside the editor's process (``--focus-wait`` seconds, read-only).
                    Without it nothing is sent, because no element of the window can hold the focus
                    then; ``--operator-focus`` lets the operator give the window the focus by hand.
                    At launch only, this opt-in case grants foreground rights to the started PID
                    (AllowSetForegroundWindow) and requests activation by its main-window handle
                    (SetForegroundWindow); Windows can refuse either. Its activation path never
                    attaches input queues, synthesizes input or changes user settings. The two
                    startup API outcomes are recorded separately from the actual UIA focus reads.

Safety (AGENTS.md rules 6 and 7, ADR-0007, ADR-0012, ADR-0020):

* Live input goes only through the engine's gate (``execute_action``, ``dry_run: false``);
  nothing here injects input. Run mode needs a configuration that allows exactly what the flow
  needs and nothing more (``config_problems``).
* Before any click the client checks, read-only, what the engine checks again: the foreground
  window is the recorded one, and the window under the click point is too.
* The client deletes only what it created, one recorded path at a time (``os.remove`` and
  ``os.rmdir``, never a recursive delete), and never a path outside its run folder.
* Nothing from the screen is saved: counts, statuses, codes, timings and booleans. The only
  texts are the synthetic ones this client wrote itself.
"""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import importlib.util
import json
import os
import re
import subprocess
import sys
import time
import tomllib
from collections import Counter
from ctypes import wintypes
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("s4_e2e_driver", HERE / "e2e_driver.py")
driver = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(driver)  # the reduced protocol's client: Protocol, flows, UI Automation reads

Abort = driver.Abort
NOT_BY_OCR = driver.NOT_BY_OCR
s3 = driver.s3
user32 = driver.user32
user32.AllowSetForegroundWindow.argtypes = [wintypes.DWORD]
user32.AllowSetForegroundWindow.restype = wintypes.BOOL

DOSSIER_PROBE_SESSION_ID = "s4d"


def dossier_serve_argv(args, live: bool) -> list:
    """The engine command of a dossier run: the reduced protocol's rule (driver.serve_argv), with
    the dossier's own probe identifier. A live run passes no fixed session id."""
    return driver.serve_argv(args.exe, args.output, args.config, live, args.session_id,
                             DOSSIER_PROBE_SESSION_ID)


RUN_ROOT = "ommatix-e2e"
RUN_ID_LIMIT = 64
E2E_ID = {"editor": "E2E-02-plan", "explorer": "E2E-03-plan", "decoy": "BDD-35-live-plan"}
# The applications that run the editor (the same binary, the same folder and checks).
EDITOR_APPS = ("editor", "decoy")

EDITOR_EXE = "ommatix-testeditor.exe"
EDITOR_FIELD = "Editor de texto"
EDITOR_SAVE = "Guardar"
EDITOR_NOTE = "nota.txt"
EDITOR_TEXT = driver.TYPED_TEXT  # synthetic: "prueba 123"
# The decoy (BDD-35, real actuator side): the editor's opt-in flag, and its two fields. A click on
# the first one hands the keyboard focus to the second (testeditor/src/main.rs, decoy_fields).
DECOY_FLAG = "--decoy-focus"
DECOY_FIELD = "Campo auxiliar"
DECOY_TARGET = "Campo de destino"
# A STALE_SNAPSHOT refusal (nothing delivered) is observed again and sent as a new request, at most
# DECOY_ATTEMPTS times; the repetition of the final request is one live request more.
DECOY_ATTEMPTS = 3
DECOY_LIVE_REQUESTS = DECOY_ATTEMPTS + 1
# Before each live request the editor's window must be the foreground window, hold the keyboard
# focus and not be minimized (read only). A failed guard sends nothing and waits for the operator;
# the waits are capped per case, and a case that cannot wait ends without spending the ledger.
DECOY_GUARD_WAITS = 3

EXPLORER_EXE = "explorer.exe"
EXPLORER_CLASS = "CabinetWClass"
# The Back button's UI Automation name on the reference machine (Spanish Windows 11); another
# language or a name with the shortcut appended is given with --explorer-back-name.
EXPLORER_BACK = "Atrás"
FOLDER_A, FOLDER_B = "carpeta-a", "carpeta-b"
ROOT_FILE = ("leeme.txt", b"Ommatix E2E: archivo de prueba de la carpeta raiz.\n")
DEEP_FILE = ("dummy.txt", b"Ommatix E2E: archivo de prueba.\n")
# Never allowed in a configuration of this client, for any application.
FORBIDDEN_APPLICATIONS = ("applicationframehost.exe",)

WM_CLOSE = 0x0010
user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user32.PostMessageW.restype = wintypes.BOOL
user32.IsWindow.argtypes = [wintypes.HWND]
user32.IsWindow.restype = wintypes.BOOL

# ---- the temporary tree of a run -------------------------------------------------------------


def valid_run_id(run_id: str) -> bool:
    """1 to 64 letters, digits, '.', '_' or '-' (the editor accepts the same names)."""
    return (1 <= len(run_id) <= RUN_ID_LIMIT
            and all(c.isascii() and (c.isalnum() or c in "._-") for c in run_id)
            and set(run_id) != {"."})


def new_run_id(prefix: str, now=time.gmtime) -> str:
    return f"{prefix}-{time.strftime('%Y%m%d-%H%M%S', now())}"


def windows_temp_dir() -> Path:
    """The temporary directory as Win32 reports it (``GetTempPathW``), which is what the
    editor's own ``std::env::temp_dir()`` resolves to, so both name the same folder."""
    buffer = ctypes.create_unicode_buffer(32768)
    length = driver.kernel32.GetTempPathW(len(buffer), buffer)
    if not 0 < length < len(buffer):
        raise Abort("the temporary directory cannot be read")
    return Path(buffer.value)


class RunTree:
    """The folder a run creates, ``<temp>/ommatix-e2e/<run id>``, and nothing else.

    ``create`` makes the run folder exclusively (it refuses one that exists, so a folder the
    client did not create is never adopted). Every directory and file the client adds is
    recorded; ``cleanup`` removes exactly the recorded paths (files, then directories, deepest
    first, with ``os.remove`` and ``os.rmdir``: no recursive delete exists here), plus the
    named files the application under test wrote in the run folder. An unexpected entry is
    left alone and reported. ``<temp>/ommatix-e2e`` is removed only if this client created it
    and it is empty."""

    def __init__(self, temp_root: Path, run_id: str):
        if not valid_run_id(run_id):
            raise Abort("the run id is not a plain name")
        self.base = Path(temp_root) / RUN_ROOT
        self.root = self.base / run_id
        self.dirs: list[Path] = []
        self.files: list[Path] = []
        self.created_base = False

    def create(self) -> Path:
        if not self.base.is_dir():
            try:
                self.base.mkdir()
                self.created_base = True
            except FileExistsError:
                pass  # created meanwhile by someone else: not ours
        try:
            self.root.mkdir()
        except FileExistsError as error:
            raise Abort("the run folder already exists: not adopted") from error
        self.dirs.append(self.root)
        return self.root

    def add_dir(self, relative: str) -> Path:
        path = self.inside(self.root / relative)
        path.mkdir()
        self.dirs.append(path)
        return path

    def add_file(self, relative: str, data: bytes) -> Path:
        path = self.inside(self.root / relative)
        with open(path, "xb") as handle:  # exclusive: never overwrites
            handle.write(data)
        self.files.append(path)
        return path

    def inside(self, path: Path) -> Path:
        """`path` when it lies in the run folder by its names (no '..', no other root)."""
        parts = path.parts
        if (".." in parts or path.parent == path
                or parts[:len(self.root.parts)] != self.root.parts):
            raise Abort("a path outside the run folder")
        return path

    def snapshot(self) -> dict[str, int]:
        """Every directory (size 0, a trailing '/') and file (its size) below the run folder."""
        found: dict[str, int] = {}
        for folder, subdirs, names in os.walk(self.root):
            here = Path(folder)
            for name in subdirs:
                found[(here / name).relative_to(self.root).as_posix() + "/"] = 0
            for name in names:
                found[(here / name).relative_to(self.root).as_posix()] = (here / name).stat().st_size
        return found

    def cleanup(self, adopt: tuple[str, ...] = ()) -> dict:
        """Removes what this run created. `adopt` names files the application under test wrote
        directly in the run folder (the editor's note); each is removed only if it is a regular
        file there. Returns the counts and, by relative name, whatever was left."""
        outcome = {"deleted_files": 0, "deleted_dirs": 0, "left": []}
        files = list(self.files)
        for name in adopt:
            path = self.root / name
            if (path.parent == self.root and path.is_file() and not path.is_symlink()
                    and path not in files):
                files.append(path)
        for path in reversed(files):
            self._remove(path, outcome, is_dir=False)
        for path in reversed(self.dirs):
            self._remove(path, outcome, is_dir=True)
        if self.created_base:
            try:
                self.base.rmdir()  # only if empty
            except OSError:
                outcome["left"].append(RUN_ROOT)
        return outcome

    def _remove(self, path: Path, outcome: dict, *, is_dir: bool) -> None:
        try:
            self.inside(path)
            if is_dir:
                path.rmdir()
                outcome["deleted_dirs"] += 1
            else:
                path.unlink()
                outcome["deleted_files"] += 1
        except (OSError, Abort):
            try:
                name = path.relative_to(self.base).as_posix()
            except ValueError:
                name = path.name
            outcome["left"].append(name)


def relative_parts(root: Path | str, location: str | None) -> tuple[str, ...] | None:
    """The folders of `location` below `root`, case-insensitively as Windows compares them;
    None when it is not inside `root` (or unknown)."""
    if not location:
        return None
    base = os.path.normcase(os.path.normpath(str(root)))
    here = os.path.normcase(os.path.normpath(location))
    if here == base:
        return ()
    if not here.startswith(base + os.sep):
        return None
    return tuple(p for p in here[len(base) + 1:].split(os.sep) if p)


# ---- configuration the flows need -----------------------------------------------------------


def config_problems(app: str, config: dict) -> list[str]:
    """What a live run of `app` needs from the operator's configuration, exactly: the editor
    needs its own binary allowed and keyboard focus read (UI Automation); Explorer needs
    ``explorer.exe`` allowed ONLY for ``CabinetWClass`` windows and a pin file, and never the
    frame host. Anything else makes the run refuse to start."""
    safety = config.get("safety", {})
    allowed = safety.get("allowed_applications", [])
    problems = []
    if not safety.get("actuation_enabled"):
        problems.append("[safety] actuation_enabled is not true")
    if not safety.get("stop_file"):
        problems.append("[safety] stop_file is not set")
    if config.get("models", {}).get("accessibility") != "uia":
        problems.append("[models] accessibility is not \"uia\" (roles, names and keyboard focus)")
    for forbidden in FORBIDDEN_APPLICATIONS:
        if forbidden in allowed:
            problems.append(f"[safety] allowed_applications lists {forbidden}")
    wanted = EDITOR_EXE if app in EDITOR_APPS else EXPLORER_EXE
    if wanted not in allowed:
        problems.append(f"[safety] allowed_applications does not list {wanted}")
    classes = safety.get("allowed_window_classes", {})
    if app in EDITOR_APPS:
        if safety.get("window_pin_file"):
            problems.append("[safety] window_pin_file is set: a pin would deny the editor")
        if EXPLORER_EXE in allowed:
            problems.append("[safety] allowed_applications lists explorer.exe in the editor run")
    else:
        if classes.get(EXPLORER_EXE) != [EXPLORER_CLASS]:
            problems.append(
                f"[safety] allowed_window_classes must be {{ \"{EXPLORER_EXE}\" = [\"{EXPLORER_CLASS}\"] }}")
        if not safety.get("window_pin_file"):
            problems.append("[safety] window_pin_file is not set")
        if set(allowed) - {EXPLORER_EXE}:
            problems.append("[safety] allowed_applications lists more than explorer.exe")
    return problems


def pin_file_path(config_path: Path, config: dict) -> Path | None:
    """The pin file as the engine resolves it: relative to the configuration file."""
    name = config.get("safety", {}).get("window_pin_file") or ""
    if not name:
        return None
    path = Path(name)
    return path if path.is_absolute() else config_path.parent / path


# ---- flows ------------------------------------------------------------------------------------


def editor_title(chars: int, saved: bool, decoy: tuple[int, int] | None = None) -> str:
    """The title the editor sets (testeditor's `Editor::title`): the field's length and whether the
    last save is current. With the decoy, `decoy` = (length of Campo auxiliar, length of Campo de
    destino) follows; lengths only, never text."""
    base = f"Ommatix S4 editor [caracteres={chars} guardado={'si' if saved else 'no'}"
    if decoy is None:
        return base + "]"
    return f"{base} auxiliar={decoy[0]} destino={decoy[1]}]"


TITLE_COUNTS = re.compile(r"caracteres=(\d+) guardado=(?:si|no) auxiliar=(\d+) destino=(\d+)\]")


def decoy_counts(title: str) -> dict | None:
    """The three lengths the decoy title shows (the main field, Campo auxiliar, Campo de destino),
    or None when the title is not the decoy's. Numbers only: no text can be read from it."""
    match = TITLE_COUNTS.search(title)
    if match is None:
        return None
    return {"caracteres": int(match.group(1)), "auxiliar": int(match.group(2)),
            "destino": int(match.group(3))}


def keys_reached_a_field(counts: dict) -> bool:
    """Whether any key reached one of the three fields (their lengths are not zero)."""
    return bool(counts["caracteres"] or counts["auxiliar"] or counts["destino"])


def focus_placement(focus: dict | None, editor_pid: int | None, decoy_id: str | None,
                    target_id: str | None) -> str:
    """Where UI Automation reports the keyboard focus: ``target`` (Campo de destino, by RuntimeId),
    ``decoy`` (Campo auxiliar), ``editor`` (another element of the editor's process: its window
    itself when no widget holds the focus), ``outside`` (another process: the desktop, when the
    window has no operating system focus) or ``unknown`` (no readable focus). The RuntimeIds are
    compared in memory and never recorded."""
    if not focus or not focus.get("runtime_id"):
        return "unknown"
    if editor_pid is None or focus["pid"] != editor_pid:
        return "outside"
    if focus["runtime_id"] == target_id:
        return "target"
    if focus["runtime_id"] == decoy_id:
        return "decoy"
    return "editor"


def editor_holds_focus(focus: dict | None, editor_pid: int | None) -> bool:
    """Whether UI Automation reports the keyboard focus inside the editor's process: the
    precondition of the case. Without the operating system's focus in the window, no element of it
    can hold the focus, so a result would say nothing about the redirect."""
    return bool(focus) and editor_pid is not None and focus["pid"] == editor_pid


def focus_summary(focus: dict | None, editor_pid: int | None) -> dict:
    """What a record keeps of a focus read: whether it is inside the editor, and the control type.
    Never a RuntimeId, a name or a value."""
    return {"in_editor": editor_holds_focus(focus, editor_pid),
            "control_type": focus.get("control_type") if focus else None}


def guard_message(state: dict, bound: float) -> str:
    """What the operator reads on stderr while the guard waits. A minimized window is said apart:
    its taskbar button has to be clicked once, and a second click would minimize it again."""
    if state["minimized"]:
        return (f"the editor's window is minimized: click its taskbar button ONCE; do not type. "
                f"Waiting up to {bound:.0f} s; nothing is sent meanwhile.")
    return ("OPERATOR: the editor's window does not have the foreground and the keyboard focus: "
            "give it the focus by hand (click its taskbar button once; do not type). Waiting up to "
            f"{bound:.0f} s; nothing is sent meanwhile.")


PLACEMENT_PROBLEM = {
    "outside":"UI Automation reports the keyboard focus outside the editor (the desktop or another "
               "application): the window has no operating system focus; the case is not exercised",
    "editor": "UI Automation reports the keyboard focus inside the editor but not on Campo de "
              "destino: the case is not exercised",
    "decoy": "UI Automation reports the keyboard focus on Campo auxiliar: the redirect did not run; "
             "the case is not exercised",
    "unknown": "UI Automation gave no readable keyboard focus: the case is not exercised",
}


def decoy_problems(facts: dict) -> list[str]:
    """Why the decoy case did not show BDD-35 on the real actuator; empty when it did. Pure: the
    facts are statuses, codes, counts and booleans that the run read, never screen text."""
    problems = []
    if facts["status"] != "indeterminate" or facts["error"] != "INDETERMINATE_EFFECT":
        problems.append("the engine did not report indeterminate with INDETERMINATE_EFFECT")
    if facts["input_delivered"] is not True or facts["verified"] is not False:
        problems.append("the focusing click was not reported delivered and unverified")
    if facts["retryable"] is not False or facts["next_step"] != "reconcile_effect":
        problems.append("the result does not say do not retry (reconcile_effect)")
    if not facts["grounding_ok"]:
        problems.append("the focusing click did not reach Campo auxiliar (identity after the click)")
    if facts["placement"] != "target":
        problems.append(PLACEMENT_PROBLEM.get(facts["placement"], PLACEMENT_PROBLEM["unknown"]))
    if not facts["replay_is_stored"]:
        problems.append("the repeated request_id did not return the stored result")
    if facts["delivered_delta"] != 1 or facts["rejected_delta"] != 0:
        problems.append(f"the engine counted {facts['delivered_delta']} deliveries and "
                        f"{facts['rejected_delta']} rejections for one request")
    if facts["counts"] is None:
        problems.append("the window title cannot be read")
    elif keys_reached_a_field(facts["counts"]):
        problems.append("a key reached a field the engine had not confirmed (safety event)")
    if not facts["folder_empty"]:
        problems.append("the run folder is not empty")
    return problems


def decoy_budget_problem(app: str, live: bool, max_live: int) -> str | None:
    """The live budget a decoy run needs: its attempts and the repetition. Below it the run would
    run out of live requests in the middle of the case."""
    if app == "decoy" and live and max_live < DECOY_LIVE_REQUESTS:
        return (f"--app decoy needs --max-live of at least {DECOY_LIVE_REQUESTS} (up to "
                f"{DECOY_ATTEMPTS} attempts and their repetition)")
    return None


def decoy_flow() -> dict:
    """BDD-35 on the real actuator: one `type` of the synthetic text into Campo auxiliar, whose
    focusing click the editor redirects. The OS-side state is the title with every length at 0 and
    the folder empty (no key reached any field, and nothing was saved)."""
    return {
        "exe": EDITOR_EXE,
        "initial": {"present": ["pendiente"], "absent": ["guardado", "fallido"]},
        "steps": [(DECOY_FIELD, NOT_BY_OCR, editor_state(editor_title(0, False, (0, 0)), "absent"),
                   EDITOR_TEXT)],
        "vocabulary": {"Editor", "Estado:", "pendiente", "guardado", "fallido", DECOY_FIELD,
                       DECOY_TARGET},
    }


def note_state(root: Path, expected: bytes) -> str:
    """The run folder as the OS shows it: ``absent`` (empty), ``ok`` (exactly ``nota.txt``,
    a regular file, with `expected` as its bytes), ``mismatch:<n>`` (other bytes, only their
    length), ``extra`` (any other entry), ``not_a_file`` or ``unreadable``."""
    try:
        names = sorted(os.listdir(root))
    except OSError:
        return "unreadable"
    if not names:
        return "absent"
    if names != [EDITOR_NOTE]:
        return "extra"
    path = root / EDITOR_NOTE
    if path.is_symlink() or not path.is_file():
        return "not_a_file"
    try:
        data = path.read_bytes()
    except OSError:
        return "unreadable"
    return "ok" if data == expected else f"mismatch:{len(data)}"


def editor_state(title: str, note: str) -> str:
    return f"{title}|file:{note}"


def editor_flow() -> dict:
    """E2E-02: type the synthetic text into the editor's field, click ``Guardar``. Two steps,
    and no other target exists: the engine's ``type`` clicks the field itself (ADR-0018)."""
    chars = len(EDITOR_TEXT)
    return {
        "exe": EDITOR_EXE,
        "initial": {"present": ["pendiente"], "absent": ["guardado", "fallido"]},
        # (label, OCR check, OS-side state[, text to type]); the state is the window title and
        # the folder's content, read from the OS.
        "steps": [
            (EDITOR_FIELD, NOT_BY_OCR, editor_state(editor_title(chars, False), "absent"),
             EDITOR_TEXT),
            (EDITOR_SAVE, NOT_BY_OCR, editor_state(editor_title(chars, True), "ok")),
        ],
        "vocabulary": {EDITOR_FIELD, EDITOR_SAVE, "Editor", "Estado:", "pendiente", "guardado",
                       "fallido"},
    }


# The element UI Automation reports with keyboard focus, as ``pid TAB RuntimeId TAB control type``.
# Read-only: the RuntimeId is compared in memory with the two fields' RuntimeIds and never written to
# a record; no name and no value is read, and the control type is a kind of element, not content.
UIA_FOCUS_RUNTIME_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$f = [System.Windows.Automation.AutomationElement]::FocusedElement
if ($f) {
  $rid = ''
  try { $rid = ($f.GetRuntimeId() -join '.') } catch {}
  $type = $f.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
  [Console]::Out.Write("$($f.Current.ProcessId)`t$rid`t$type")
}
"""


def parse_focus_runtime(output: str) -> dict | None:
    """``{pid, runtime_id, control_type}`` from UIA_FOCUS_RUNTIME_PS; None when the output has
    another shape (nothing had the focus, or the read failed)."""
    parts = output.strip("\r\n").split("\t")
    if len(parts) != 3 or not parts[0].strip().isdigit():
        return None
    return {"pid": int(parts[0]), "runtime_id": parts[1].strip(), "control_type": parts[2].strip()}


def uia_focus_runtime() -> dict | None:
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", UIA_FOCUS_RUNTIME_PS],
                             capture_output=True, text=True, encoding="utf-8", errors="replace",
                             check=False, creationflags=driver.CREATE_NO_WINDOW, timeout=30).stdout
    except subprocess.TimeoutExpired:
        return None
    return parse_focus_runtime(out)


class NavLabel(str):
    """A navigation target: its exact UI Automation name, the control types that stand for it in
    the window, the engine zone it was expected in and, optionally, its AutomationId.

    The control types decide which UI Automation element is meant (``identity_lookup``) and,
    through the inverse of the accessibility adapter's role mapping, which of the engine's
    elements that share the name can be it (``roles``): a breadcrumb is a Button or SplitButton
    (role ``button``), a tab title or a file-list item is not. The zone is NOT a condition: in
    the ninth round the engine gave zone ``unknown`` to the navigation targets, so it is only
    recorded as an observation."""

    def __new__(cls, name: str, control_types: tuple[str, ...], zone: str,
                automation_id: str | None = None):
        obj = super().__new__(cls, name)
        obj.control_types = control_types
        obj.zone = zone
        obj.automation_id = automation_id
        obj.roles = tuple(sorted(role for role, types in driver.ROLE_CONTROL_TYPES.items()
                                 if set(types) & set(control_types)))
        return obj


BREADCRUMB_TYPES = ("Button", "SplitButton")


def explorer_flow(root_name: str, back_name: str = EXPLORER_BACK,
                  back_automation_id: str | None = None) -> dict:
    """E2E-03: the window starts on ``carpeta-b``; breadcrumb ``carpeta-a`` (up), breadcrumb of
    the root (up), then Back (to ``carpeta-a``). Single clicks only. The OS-side check after
    each click is the window's current folder below the root."""
    return {
        "exe": EXPLORER_EXE,
        "initial": NOT_BY_OCR,
        "steps": [
            (NavLabel(FOLDER_A, BREADCRUMB_TYPES, "top_bar"), NOT_BY_OCR, (FOLDER_A,)),
            (NavLabel(root_name, BREADCRUMB_TYPES, "top_bar"), NOT_BY_OCR, ()),
            (NavLabel(back_name, ("Button",), "top_bar", back_automation_id), NOT_BY_OCR,
             (FOLDER_A,)),
        ],
        # Every click of this flow rebuilds the breadcrumb or the history (ADR-0022): the closed
        # list of driver.NAVIGATING_STEPS, whose success criterion is the folder check above.
        "navigating": driver.NAVIGATING_STEPS["explorer"],
        "vocabulary": {FOLDER_A, FOLDER_B, root_name, back_name},
    }


def is_click_only(flow: dict) -> bool:
    """No step types, and every target is a plain name or a NavLabel (no key, no wheel)."""
    return all(len(step) == 3 and isinstance(step[0], str) for step in flow["steps"])


def pick_new_window(before: set, after: set, locations: dict, root: Path) -> int | None:
    """The window this client opened: among the Explorer windows that were not there before,
    the one whose current folder is `root`. None unless it is exactly one."""
    fresh = [h for h in sorted(after - before) if relative_parts(root, locations.get(h)) == ()]
    return fresh[0] if len(fresh) == 1 else None


# ---- read-only Windows queries ----------------------------------------------------------------

EXPLORER_LOCATION_PS = r"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$shell = New-Object -ComObject Shell.Application
$found = @()
foreach ($w in $shell.Windows()) {
  try { if ([int64]$w.HWND -eq __HWND__) { $found += $w.Document.Folder.Self.Path } } catch {}
}
if ($found.Count -eq 1) { [Console]::Out.Write($found[0]) }
"""


def explorer_location(hwnd: int) -> str | None:
    """The folder shown by the Explorer window `hwnd` (``Shell.Application``, read-only): a
    file system path, or None when it is not exactly one window or cannot be read."""
    script = EXPLORER_LOCATION_PS.replace("__HWND__", str(int(hwnd)))
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", script],
                             capture_output=True, text=True, encoding="utf-8", errors="replace",
                             check=False, creationflags=driver.CREATE_NO_WINDOW,
                             timeout=60).stdout
    except subprocess.TimeoutExpired:
        return None
    return out.strip() or None


def explorer_windows() -> set[int]:
    """The visible top-level File Explorer windows (class CabinetWClass of explorer.exe)."""
    found: set[int] = set()

    def visit(hwnd, _):
        if (user32.IsWindowVisible(hwnd) and driver._class(hwnd) == EXPLORER_CLASS
                and driver._exe(driver._pid(hwnd)) == EXPLORER_EXE):
            found.add(int(hwnd))
        return True

    user32.EnumWindows(driver.WNDENUMPROC(visit), 0)
    return found


def is_recorded_explorer_window(hwnd: int) -> bool:
    return (bool(user32.IsWindow(hwnd)) and driver._class(hwnd) == EXPLORER_CLASS
            and driver._exe(driver._pid(hwnd)) == EXPLORER_EXE)


# ---- the protocol -----------------------------------------------------------------------------


class DossierProtocol(driver.Protocol):
    """The reduced protocol's ``Protocol`` with the lifecycle of the two flows. Everything that
    touches the OS is a method here, so a test replaces it with a fake."""

    def __init__(self, args, server, stop_file, flow, ledger, temp_root: Path | None = None,
                 pin_file: Path | None = None):
        super().__init__(args, server, stop_file, flow, ledger)
        self.temp_root = Path(temp_root) if temp_root is not None else windows_temp_dir()
        self.pin_file = pin_file
        self.tree: RunTree | None = None
        self.explorer_hwnd: int | None = None
        self.editor_pid: int | None = None
        self.root_name: str | None = None

    # -- OS access (overridden by the tests)
    def spawn(self, argv: list[str]) -> int:
        return subprocess.Popen(argv).pid

    def grant_editor_foreground(self, pid: int) -> bool:
        """Best-effort foreground rights for the decoy process this client just started.
        Windows can refuse if this client has no foreground rights; never grant ASFW_ANY.
        https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-allowsetforegroundwindow
        """
        if (self.args.app != "decoy" or pid != self.editor_pid or pid not in self.started
                or not 0 < pid < 0xFFFFFFFF):
            return False
        try:
            return bool(user32.AllowSetForegroundWindow(pid))
        except OSError:
            return False

    def request_editor_foreground(self, hwnd) -> bool:
        """Documented activation only, restricted to the started decoy's main window.
        No toolkit Focus command: winit implements that with synthetic Alt input. The
        return value is API acceptance, never evidence of keyboard focus (UIA checks that).
        https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow
        """
        if (self.args.app != "decoy" or self.editor_pid not in self.started or not hwnd
                or driver._pid(hwnd) != self.editor_pid
                or driver.is_helper_class(driver._class(hwnd))):
            return False
        try:
            return bool(user32.SetForegroundWindow(hwnd))
        except OSError:
            return False

    def activate(self, hwnd) -> str | None:
        if self.args.app != "decoy":
            return super().activate(hwnd)
        # Reuse the driver's bounded verification, with an explicit method list so none
        # of its legacy input-queue attachment methods can run for this case.
        return driver.activate_window(
            hwnd, methods=(("set_foreground", self.request_editor_foreground),),
            foreground_now=self.front_hwnd)

    def title_of(self, hwnd) -> str:
        return driver.window_title(hwnd)

    def location_of(self, hwnd) -> str | None:
        return explorer_location(hwnd)

    def open_explorer(self, start: Path) -> int | None:
        """Opens ONE File Explorer window on `start` and returns its handle: the only new
        window whose current folder is `start`. None when it cannot be told."""
        before = explorer_windows()
        root = start
        launch = ["explorer.exe", f"/root,{start}"] if self.args.explorer_launch == "root" \
            else ["explorer.exe", f"/e,{start}"]
        subprocess.Popen(launch)
        deadline = time.monotonic() + self.args.foreground_wait_explorer
        while time.monotonic() < deadline:
            time.sleep(0.5)
            after = explorer_windows()
            locations = {h: self.location_of(h) for h in after - before}
            picked = pick_new_window(before, after, locations, root)
            if picked is not None:
                return picked
        return None

    def post_close(self, hwnd: int) -> None:
        """WM_CLOSE to the window this client opened: a window message, not input."""
        user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)

    def window_exists(self, hwnd: int) -> bool:
        return bool(user32.IsWindow(hwnd))

    def is_our_explorer_window(self, hwnd: int) -> bool:
        return is_recorded_explorer_window(hwnd)

    def front_window(self) -> dict:
        return driver.foreground()

    def front(self) -> dict:
        return self.front_window()

    def bring_front(self, hwnd) -> None:
        """Window management, not input: the recorded window maximized on the primary monitor
        (the request for the foreground is `await_front`'s, by handle)."""
        if not driver.on_primary_maximized(hwnd):
            driver.maximize_on_primary(hwnd)

    # -- the checks of the flows
    def read_end_state(self, hwnd):
        if self.args.app in EDITOR_APPS:
            return editor_state(self.title_of(hwnd),
                                note_state(self.tree.root, EDITOR_TEXT.encode("utf-8")))
        return relative_parts(self.tree.root, self.location_of(self.explorer_hwnd))

    def require_foreground(self) -> dict:
        if self.args.app in EDITOR_APPS:
            return super().require_foreground()
        # Explorer: the foreground is the one window this client opened, maximized on the
        # primary monitor; any other Explorer window (the owner's) stops the protocol.
        fg = self.front_window()
        if (fg["exe"] != EXPLORER_EXE or fg["hwnd"] != self.explorer_hwnd
                or not fg["maximized_primary"]):
            self.engage_stop("the foreground is not the Explorer window the client opened")
        return fg

    @staticmethod
    def nav_candidates(elements, spec) -> list[dict]:
        """The engine's elements a navigation target can be: the exact name and the role that
        the target's UI Automation control types stand for (a breadcrumb is a ``button``; a tab
        title or a file-list item is not). The engine's zone is not a condition."""
        return [e for e in elements
                if e.get("label", "").strip() == str(spec) and e.get("role") in spec.roles]

    def find_target(self, elements, spec):
        if not isinstance(spec, NavLabel):
            return super().find_target(elements, spec)
        found = self.nav_candidates(elements, spec)
        if len(found) > 1:
            self.engage_stop(f"ambiguous label: {len(found)} elements share the step's label "
                             "and role")
        if found and found[0]["actionability"] == "confirmed":
            return spec, found[0]
        return spec, None

    def label_control_types(self, label, target):
        if isinstance(label, NavLabel):
            return label.control_types
        return super().label_control_types(label, target)

    def identity_lookup(self, hwnd, label, target):
        if not isinstance(label, NavLabel):
            return super().identity_lookup(hwnd, label, target)
        rows = self.identify(hwnd, str(label), label.control_types)
        if label.automation_id and rows is not None:
            rows = [r for r in rows if r["automation_id"] == label.automation_id]
        status, found = driver.identity_decision(rows)
        record = {"status": status,
                  "candidates": sum(1 for r in rows or [] if not r["offscreen"]),
                  "offscreen_ignored": sum(1 for r in rows or [] if r["offscreen"])}
        if found is not None:
            # The window under the click point must be the recorded one (the engine checks
            # it again with the actuator's own attestation, ADR-0020).
            centre = self.element_centre(hwnd, str(label), label.control_types)
            under = self.window_under(centre) if centre is not None else None
            if under != self.explorer_hwnd:
                record["status"] = "outside_recorded_window"
                found = None
            else:
                record.update(runtime_id=found["runtime_id"], automation_id=found["automation_id"],
                              control_type=found["control_type"])
        return record, found

    def record_target(self, res, elements, spec, hwnd, stopped=False):
        super().record_target(res, elements, spec, hwnd, stopped)
        if isinstance(spec, NavLabel):
            # The engine's zone for the candidates, as an observation only (fixed vocabulary).
            res["targets"][-1]["zones"] = dict(sorted(Counter(
                str(e.get("zone")) for e in self.nav_candidates(elements, spec)).items()))
            res["targets"][-1]["roles_expected"] = list(spec.roles)

    # -- one run
    def run_once(self, n: int) -> dict:
        res = driver.new_result(E2E_ID[self.args.app], n)
        res["action_freshness"] = "target_window" if getattr(self.args, "target_window_freshness", False) else "operator_config"
        res.update(suite="dossier")
        started = time.perf_counter()
        res["user_idle_s_at_start"] = driver.idle_seconds()
        self.tree = None
        self.explorer_hwnd = None
        self.editor_pid = None
        try:
            if self.args.app == "editor":
                self.run_editor(res, n)
            elif self.args.app == "decoy":
                self.run_decoy(res, n)
            else:
                self.run_explorer(res, n)
        except Abort as error:
            res["aborted"] = str(error)
            self.finish(res)
            res["total_s"] = round(time.perf_counter() - started, 2)
            with open(self.args.results, "a", encoding="utf-8") as fh:
                fh.write(json.dumps(res, ensure_ascii=False) + "\n")
            raise
        self.finish(res)
        res["total_s"] = round(time.perf_counter() - started, 2)
        return res

    def settle_observation(self, res: dict, check):
        """Observes until the visible state is as `check` says. The editor's also needs
        accessibility-sourced elements: its field and its button are named by UI Automation
        only, and the first read can come before the window holds the keyboard focus."""
        snap, elements = None, []
        for _ in range(6):
            snap, elements, obs = self.observe()
            res["observations"].append(obs)
            seen = self.ocr_ok(elements, check)
            if seen and (self.args.app not in EDITOR_APPS
                         or any(e.get("label_source") == "accessibility" for e in elements)):
                return snap, elements, True
            time.sleep(0.5)
        return snap, elements, self.ocr_ok(elements, check)

    def focus_owner(self, pid: int | None) -> dict:
        """Read-only: the process, class and control type of the element UI Automation reports
        with keyboard focus (never its name or value), and whether it belongs to `pid`."""
        owner = driver.uia_focus_owner()
        if owner is not None:
            owner["is_app"] = pid is not None and owner["pid"] == pid
        return owner or {"unreadable": True}

    def focus_runtime(self) -> dict | None:
        """Read-only: the process and RuntimeId of the element that has the keyboard focus."""
        return uia_focus_runtime()

    def run_editor(self, res: dict, n: int) -> None:
        tree = self.tree = RunTree(self.temp_root, new_run_id(f"s4-e2e02-r{n}"))
        folder = tree.create()
        pid = self.editor_pid = self.spawn([self.args.editor_exe, "--folder", str(folder)])
        self.started.add(pid)
        ok, requests = self.wait_foreground(pid)
        res["activation_requests"] = requests
        res.update(driver.activation_fields(self.activation))
        if not ok:
            res["failure"] = "the editor did not reach the foreground maximized"
            return
        hwnd = self.app_window(pid)
        self.app_hwnd = hwnd
        time.sleep(self.args.settle)
        snap, elements, visible = self.settle_observation(res, self.flow["initial"])
        if not visible:
            res["failure"] = "initial state not visible by OCR"
        accessible = sum(1 for e in elements if e.get("label_source") == "accessibility")
        res["accessibility_elements"] = accessible
        if accessible == 0 and res["failure"] is None:
            # The field's name and role come from UI Automation only: without evidence from it
            # there is nothing to click or type into. Say whose window holds the keyboard focus
            # (the engine attests the application by it) and stop before any input.
            res["uia_focus"] = self.focus_owner(pid)
            res["failure"] = "no accessibility evidence from the editor"
        if self.args.mode == "probe":
            self.probe_print(elements)
        if res["failure"] is None:
            initial = editor_state(editor_title(0, False), "absent")
            if self.read_end_state(hwnd) != initial:
                res["failure"] = "the editor does not start empty with its folder empty"
        if res["failure"] is None and self.args.mode == "run":
            self.steps(res, snap, elements, hwnd, pid)
            res["note_state"] = note_state(tree.root, EDITOR_TEXT.encode("utf-8"))

    def run_decoy(self, res: dict, n: int) -> None:
        """BDD-35 on the real actuator. The editor starts with --decoy-focus: a click on Campo
        auxiliar hands the keyboard focus to Campo de destino. One `type` of the synthetic text on
        Campo auxiliar goes through the engine's gate (a STALE_SNAPSHOT refusal, which delivers
        nothing, is observed again and sent as a new request, at most three attempts). The engine
        must report indeterminate, with
        its focusing click delivered, verified false, and no key sent; a repeat of the same
        request_id must return that stored result with no new delivery; and UI Automation must
        place the keyboard focus on Campo de destino, so the case is not passed by a focus that was
        never moved. This client sends no input itself."""
        tree = self.tree = RunTree(self.temp_root, new_run_id(f"s4-bdd35-r{n}"))
        folder = tree.create()
        pid = self.editor_pid = self.spawn([self.args.editor_exe, "--folder", str(folder),
                                            DECOY_FLAG])
        self.started.add(pid)
        res["foreground_granted"] = self.grant_editor_foreground(pid)
        ok, requests = self.wait_foreground(pid)
        res["activation_requests"] = requests
        res.update(driver.activation_fields(self.activation))
        if not ok:
            if self.args.operator_focus:
                # A denied foreground request is not a reason to force activation. The
                # existing operator wait only reads focus; the operator acts by hand.
                self.wait_editor_focus(pid, res)
                ok = self.is_front(self.front(), pid)
            if not ok:
                res["failure"] = "the editor did not reach the foreground maximized"
                return
        hwnd = self.app_window(pid)
        self.app_hwnd = hwnd
        # Even an already-foreground window may lack keyboard activation (D1). Ask once
        # by its handle, then leave the existing UIA focus wait and guards authoritative.
        res["startup_focus_requested"] = self.request_editor_foreground(hwnd)
        time.sleep(self.args.settle)
        snap, elements, visible = self.settle_observation(res, self.flow["initial"])
        if not visible:
            res["failure"] = "initial state not visible by OCR"
        accessible = {e.get("label", "").strip() for e in elements
                      if e.get("label_source") == "accessibility"}
        res["accessibility_elements"] = sum(1 for e in elements
                                            if e.get("label_source") == "accessibility")
        if res["failure"] is None and not {DECOY_FIELD, DECOY_TARGET} <= accessible:
            # Both fields are named by UI Automation only: without that evidence there is nothing
            # to identify or to check the focus against. Nothing is sent.
            res["uia_focus"] = self.focus_owner(pid)
            res["failure"] = "no accessibility evidence for the decoy fields"
        if res["failure"] is None:
            initial = editor_state(editor_title(0, False, (0, 0)), "absent")
            if self.read_end_state(hwnd) != initial:
                res["failure"] = "the decoy editor does not start empty with its folder empty"
        if self.args.mode == "probe":
            self.probe_print(elements)
            # Read-only: whether the window holds the keyboard focus now (the case's precondition).
            res["focus_probe"] = focus_summary(self.focus_runtime(), pid)
        if res["failure"] is None and self.args.mode == "run":
            self.decoy_step(res, snap, elements, hwnd, pid)
            res["note_state"] = note_state(tree.root, EDITOR_TEXT.encode("utf-8"))

    def wait_editor_focus(self, pid: int, res: dict) -> dict | None:
        """Read-only: waits until UI Automation reports the keyboard focus inside the editor's
        process. The bound is --focus-wait seconds, or --operator-focus-wait after an instruction to
        the operator (--operator-focus: the operator gives the window the focus by hand, with its
        taskbar button, and never types). Nothing is sent meanwhile. Returns the last read; the
        caller decides what it means."""
        bound = self.args.operator_focus_wait if self.args.operator_focus else self.args.focus_wait
        if self.args.operator_focus:
            self.announce(f"OPERATOR: give the editor's window the keyboard focus by hand (click its "
                          f"taskbar button once; do not type). Waiting up to {bound:.0f} s; nothing "
                          "is sent meanwhile.")
        started = time.monotonic()
        focus = self.focus_runtime()
        while not editor_holds_focus(focus, pid) and time.monotonic() - started < bound:
            time.sleep(0.5)
            focus = self.focus_runtime()
        res["focus_wait_s"] = round(time.monotonic() - started, 1)
        return focus

    GUARD_POLL_S = 0.5  # how often the guard reads the window while it waits (read only)

    def foreground_pid(self) -> int | None:
        """Read-only: the process of the window the operating system has in front."""
        return driver.foreground().get("pid")

    def is_minimized(self, hwnd) -> bool:
        """Read-only: whether the editor's window is minimized (IsIconic)."""
        return bool(driver.user32.IsIconic(hwnd))

    def foreground_fact(self) -> dict:
        """Read-only: the foreground window as require_foreground reads it, without stopping the run
        when it is not the editor's: the guard decides what happens before any request."""
        return driver.foreground()

    def guard_reads(self, pid: int, hwnd) -> dict:
        """What the guard reads, as booleans only (no title, no RuntimeId): the window in front is
        the editor's, UI Automation holds the keyboard focus inside the editor, and the window is not
        minimized."""
        foreground = self.foreground_pid() == pid
        focus = editor_holds_focus(self.focus_runtime(), pid)
        minimized = self.is_minimized(hwnd)
        return {"foreground": foreground, "focus": focus, "minimized": minimized,
                "ok": foreground and focus and not minimized}

    def guard(self, res: dict, pid: int, hwnd, where: str) -> str:
        """The guard before a live request, and before the observation that precedes it. Read only:
        a request goes out only when the guard passes. Otherwise nothing is sent: the case says on
        stderr what it sees and waits for the operator (--operator-focus, up to its bound; otherwise
        --focus-wait), at most DECOY_GUARD_WAITS times per case. Returns "ready" (nothing to wait
        for), "waited" (the window is back: the caller observes again, the snapshot being older than
        the wait) or "lost" (no focus within the bound, or the waits are used up: nothing was sent and
        nothing was spent)."""
        state = self.guard_reads(pid, hwnd)
        if state["ok"]:
            return "ready"
        waits = res.get("operator_waits", 0)
        if waits >= DECOY_GUARD_WAITS:
            res.setdefault("guard", []).append({"where": where, **state, "outcome": "lost"})
            return "lost"
        res["operator_waits"] = waits + 1
        bound = self.args.operator_focus_wait if self.args.operator_focus else self.args.focus_wait
        self.announce(guard_message(state, bound))
        started = time.monotonic()
        while not state["ok"] and time.monotonic() - started < bound:
            time.sleep(self.GUARD_POLL_S)
            state = self.guard_reads(pid, hwnd)
        outcome = "waited" if state["ok"] else "lost"
        res.setdefault("guard", []).append({"where": where, **state, "outcome": outcome,
                                            "waited_s": round(time.monotonic() - started, 1)})
        return outcome

    def guard_lost(self, res: dict, where: str) -> str:
        """The failure of a case whose guard gave up: nothing was sent and nothing was spent."""
        return (f"no request ({where}): the editor's window did not get the foreground and the "
                f"keyboard focus back within the bound ({res['operator_waits']} operator waits); "
                "nothing was sent and nothing was spent")

    def decoy_step(self, res: dict, snap: dict, elements: list[dict], hwnd, pid: int) -> None:
        """The request of the case and its checks (see decoy_problems). Nothing is sent before the
        decoy's point is grounded to the decoy by identity (ADR-0022, rule 1), and nothing is sent
        while the guard fails: the editor's window must be the foreground window, hold the keyboard
        focus and not be minimized, all of it read only, and a failed guard waits for the operator. A
        request is retried only after STALE_SNAPSHOT, which the engine answers before any delivery:
        the case observes again, grounds again and sends a new request (a new request_id), at most
        DECOY_ATTEMPTS times. The repetition is the stored-result check of the final request, not a
        second delivery, and it is guarded too."""
        field = self.target(elements, DECOY_FIELD)
        redirect = self.target(elements, DECOY_TARGET)
        if field is None or redirect is None:
            res["failure"] = "the decoy fields are not both confirmed exactly once"
            return
        res["steps"] = 1
        res["operator_waits"] = 0
        # The precondition of the case, read before anything else: the editor's window must hold the
        # operating system's keyboard focus. Without it no element of the editor can hold the focus,
        # and a result would not say anything about the redirect (the S4 decoy round). Read-only.
        focus = self.wait_editor_focus(pid, res)
        res["focus_before"] = focus_summary(focus, pid)
        focus_read_at = time.monotonic()
        if not editor_holds_focus(focus, pid):
            res["failure"] = ("no request: UI Automation reports the keyboard focus outside the editor "
                              "(the window has no operating system focus); nothing was sent")
            return
        sent = 0
        while True:
            if self.guard(res, pid, hwnd, "before the observation") == "lost":
                res["failure"] = self.guard_lost(res, "before the observation")
                return
            # A fresh observation for every attempt: a long wait (the operator's) ages the observation
            # past [engine] max_snapshot_age_ms, and a STALE_SNAPSHOT means the screen moved since it.
            snap, elements, obs = self.observe()
            observed_at = time.monotonic()
            res["observations"].append(obs)
            field = self.target(elements, DECOY_FIELD)
            redirect = self.target(elements, DECOY_TARGET)
            if field is None or redirect is None:
                res["failure"] = ("after the focus wait the decoy fields are not both confirmed "
                                  "exactly once")
                return
            fg = self.foreground_fact()
            ident = self.identify_before(res, fg, DECOY_FIELD, field)
            if ident is None:
                return
            pre = self.pre_click_grounding(1, fg, DECOY_FIELD, field, ident)
            res.setdefault("pre_grounded", []).append({"attempt": sent + 1, **pre})
            if not pre["grounded"]:
                res["failure"] = f"no request, the decoy point is not grounded ({pre['status']})"
                return
            redirect_record, redirect_ident = self.identity_lookup(fg["hwnd"], DECOY_TARGET, redirect)
            res["redirect_identity"] = redirect_record["status"]
            if redirect_ident is None:
                res["failure"] = ("no request, the redirect target's identity is "
                                  f"{redirect_record['status']}")
                return
            # The guard right before the request: the last read before anything is sent.
            state = self.guard(res, pid, hwnd, f"before attempt {sent + 1}")
            if state == "lost":
                res["failure"] = self.guard_lost(res, f"before attempt {sent + 1}")
                return
            if state == "waited":
                continue  # the window is back, but the snapshot is older than the wait: observe again
            attempt = sent + 1
            before = self.metrics()
            args = {"request_id": f"s4-bdd35-r{res['run']}-{attempt}",
                    "session_id": snap["session_id"], "snapshot_id": snap["snapshot_id"],
                    "geometry_epoch": snap["geometry_epoch"], "target_id": field["target_id"],
                    "expected_revision": field["revision"], "action": "type", "dry_run": False,
                    "expected_effect": "visual_change", "text": EDITOR_TEXT}
            self.spend_live()
            res["actions_requested"] += 1
            sent_at = time.monotonic()
            out, _, meta, ms = self.srv.call("execute_action", args)
            sent += 1
            error = out.get("error") or {}
            err = error.get("code")
            act = {"step": 1, "attempt": attempt, "action": "type",
                   "expected_effect": "visual_change", "status": out.get("status"),
                   "input_delivered": out.get("input_delivered"), "verified": out.get("verified"),
                   "observed_effect": out.get("observed_effect"), "error": err,
                   "rtt_ms": round(ms, 1), "stages_ms": driver.stage_ms(meta),
                   **driver.freshness_fields(out, meta),
                   "observe_to_action_ms": round((sent_at - observed_at) * 1000, 1),
                   "since_focus_ms": round((sent_at - focus_read_at) * 1000, 1),
                   "target_role": field["role"], "label_source": field["label_source"]}
            refusal = driver.engine_error(out)
            if refusal:  # a refusal or an uncertainty: the engine's own sentence
                act["message"] = refusal["message"]
            res["actions"].append(act)
            driver.log("action", run=res["run"], **act)
            if out.get("status") == "effect_observed":
                res["effect_observed"] += 1
            elif out.get("status") == "indeterminate":
                res["indeterminate"] += 1
            elif out.get("status") == "rejected":
                res["rejected"] += 1
            if err == "STALE_SNAPSHOT" and not out.get("input_delivered"):
                if sent < DECOY_ATTEMPTS:
                    res["stale_retries"] += 1
                    continue
                res["decoy"] = {"attempts": sent, "status": out.get("status"), "error": err,
                                "input_delivered": False}
                res["failure"] = (f"the screen changed before each of the {DECOY_ATTEMPTS} attempts "
                                  "(STALE_SNAPSHOT): nothing was delivered and nothing was repeated")
                return
            break
        grounding_ok = False
        if out.get("input_delivered"):
            res["delivered"] += 1
            res["typed_delivered"] += 1
            # The focusing click must have reached Campo auxiliar, by identity: a click on the
            # target itself would leave the focus on it too and look like the case.
            grounding_ok = driver.identity_hit(ident, self.cursor_chain())
            res.setdefault("grounding", []).append(grounding_ok)
            if not grounding_ok:
                res["grounding_wrong"] = 1
        # The repetition of the final request, guarded: its stored result, and no second delivery.
        if self.guard(res, pid, hwnd, "before the repetition") == "lost":
            res["decoy"] = {"attempts": sent, "status": out.get("status"), "error": err,
                            "input_delivered": out.get("input_delivered"),
                            "grounding_ok": grounding_ok, "repetition_sent": False}
            res["failure"] = ("no repetition: the editor's window lost the foreground or the keyboard "
                              "focus before it, so the repetition was not sent and nothing was spent")
            res["success"] = False
            return
        self.spend_live()
        res["actions_requested"] += 1
        again, _, _, _ = self.srv.call("execute_action", args)
        again_error = (again.get("error") or {}).get("code")
        after = self.metrics()
        counts = decoy_counts(self.title_of(hwnd))
        focus_after = self.focus_runtime()
        placement = focus_placement(focus_after, pid, ident.get("runtime_id"),
                                    redirect_ident.get("runtime_id"))
        facts = {
            "attempts": sent,
            "status": out.get("status"), "error": err,
            "input_delivered": out.get("input_delivered"), "verified": out.get("verified"),
            "retryable": error.get("retryable"), "next_step": error.get("next_step"),
            "placement": placement,
            "focus_before": res["focus_before"],
            "focus_after": focus_summary(focus_after, pid),
            "replay": driver.answer_record(again),
            "grounding_ok": grounding_ok,
            "replay_is_stored": ((again.get("status"), again_error, again.get("input_delivered"))
                                 == (out.get("status"), err, out.get("input_delivered"))),
            "delivered_delta": after["delivered_actions"] - before["delivered_actions"],
            "rejected_delta": after["rejected_actions"] - before["rejected_actions"],
            "counts": counts,
            "folder_empty": note_state(self.tree.root, EDITOR_TEXT.encode("utf-8")) == "absent",
            "repetition_sent": True,
        }
        res["decoy"] = facts
        if counts is not None and keys_reached_a_field(counts):
            res["keys_without_focus"] = 1  # a safety event: the run ends the invocation
        problems = decoy_problems(facts)
        res["failure"] = "; ".join(problems) or None
        res["success"] = res["failure"] is None

    def run_explorer(self, res: dict, n: int) -> None:
        tree = self.tree = RunTree(self.temp_root, new_run_id(f"s4-e2e03-r{n}"))
        root = tree.create()
        self.root_name = root.name
        tree.add_dir(FOLDER_A)
        tree.add_dir(f"{FOLDER_A}/{FOLDER_B}")
        tree.add_file(ROOT_FILE[0], ROOT_FILE[1])
        tree.add_file(f"{FOLDER_A}/{FOLDER_B}/{DEEP_FILE[0]}", DEEP_FILE[1])
        before = tree.snapshot()
        res["tree_entries"] = len(before)
        self.flow = explorer_flow(root.name, getattr(self.args, "explorer_back_name", EXPLORER_BACK),
                                  getattr(self.args, "explorer_back_id", None) or None)
        start = root / FOLDER_A / FOLDER_B
        hwnd = self.open_explorer(start)
        if hwnd is None:
            res["failure"] = "the Explorer window could not be told apart"
            return
        self.explorer_hwnd = hwnd
        res["window_recorded"] = True
        if self.pin_file is not None:
            self.pin_file.write_text(str(hwnd), encoding="ascii")
            res["pin_written"] = True
        self.bring_front(hwnd)
        arrived = self.wait_explorer_front(hwnd)
        res.update(driver.activation_fields(self.activation))
        if not arrived:
            res["failure"] = "the Explorer window did not reach the foreground maximized"
            return
        time.sleep(self.args.settle)
        snap, elements, _ = self.settle_observation(res, NOT_BY_OCR)
        if relative_parts(root, self.location_of(hwnd)) != (FOLDER_A, FOLDER_B):
            res["failure"] = "the Explorer window does not start on carpeta-b"
            return
        if self.args.mode == "probe":
            self.probe_print(elements)
            self.explorer_probe(hwnd, elements)
            return
        self.steps(res, snap, elements, hwnd, None)
        res["tree_unchanged"] = tree.snapshot() == before
        if not res["tree_unchanged"]:
            res["success"] = False
            res["failure"] = res["failure"] or "the tree changed"

    def explorer_probe(self, hwnd: int, elements: list[dict]) -> None:
        """What each navigation step would find, without input: per target name, the engine's
        elements by zone, role and actionability and the UI Automation elements by control
        type. The names are the client's own folders."""
        for step in self.flow["steps"]:
            label = step[0]
            engine = Counter((e.get("zone"), e.get("role"), e.get("actionability"))
                             for e in elements if e.get("label", "").strip() == str(label))
            usable = self.nav_candidates(elements, label)
            rows = self.identify(hwnd, str(label), driver.ANY_CONTROL_TYPE) or []
            uia = Counter((r["control_type"], r["offscreen"]) for r in rows)
            # The identifiers (never names) of the on-screen elements of the target's own
            # control types: what an AutomationId for the target would be.
            ids = sorted({(r["control_type"], r["automation_id"]) for r in rows
                          if r["control_type"] in label.control_types and not r["offscreen"]})
            driver.log("explorer_probe", target=str(label), zone_expected=label.zone,
                       control_types=list(label.control_types), roles=list(label.roles),
                       engine=[[*key, n] for key, n in sorted(engine.items(), key=str)],
                       engine_usable=[[e.get("zone"), e.get("actionability")] for e in usable],
                       uia=[[*key, n] for key, n in sorted(uia.items(), key=str)],
                       uia_identifiers=[list(pair) for pair in ids])

    def wait_explorer_front(self, hwnd: int) -> bool:
        """The recorded window in front and maximized on the primary monitor, asked for by its
        handle (the shell's explorer.exe owns many windows, so never by process). Window
        management only: no input. See `Protocol.await_front`."""
        def ok(fg):
            return (fg["hwnd"] == hwnd and fg["exe"] == EXPLORER_EXE
                    and bool(fg["maximized_primary"]))

        arrived, _ = self.await_front(ok, lambda: hwnd, lambda: self.bring_front(hwnd),
                                      lambda: self.diagnose(None, hwnd))
        return arrived

    def finish(self, res: dict) -> None:
        """Closes what this run opened and deletes what it created. Never raises: whatever
        cannot be done is recorded."""
        closed = True
        try:
            if self.args.app in EDITOR_APPS:
                self.close(self.editor_pid)
                closed = self.editor_pid not in self.started
            elif self.explorer_hwnd is not None:
                closed = self.close_explorer(self.explorer_hwnd)
            res["window_closed"] = closed
        except Exception as error:  # cleanup must not hide the run's own result
            closed = False
            res["window_closed"] = False
            res["close_error"] = type(error).__name__
        if self.pin_file is not None:
            try:
                self.pin_file.unlink(missing_ok=True)
            except OSError:
                res["pin_left"] = True
        if self.tree is None:
            return
        if not closed:
            # An open window may still show the tree: it is left for the operator.
            res["cleanup"] = {"deleted_files": 0, "deleted_dirs": 0, "left": [self.tree.root.name]}
        else:
            res["cleanup"] = self.tree.cleanup(
                adopt=(EDITOR_NOTE,) if self.args.app == "editor" else ())
        driver.log("cleanup", run=res.get("run"), **res["cleanup"])

    def close_explorer(self, hwnd: int) -> bool:
        """Closes the window this client opened, by its handle, and only if it still is a
        File Explorer window; never kills a process. True when it is gone."""
        if not self.window_exists(hwnd):
            return True
        if not self.is_our_explorer_window(hwnd):
            return False  # the handle names something else now: not touched
        self.post_close(hwnd)
        for _ in range(40):
            if not self.window_exists(hwnd):
                return True
            time.sleep(0.25)
        return False


# ---- command line -----------------------------------------------------------------------------


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--exe", required=True)
    p.add_argument("--config", required=True, type=Path)
    p.add_argument("--target-window-freshness", action="store_true",
                   help="M2: use a target_window config copy beside --results, outside the repo")
    p.add_argument("--app", choices=sorted(E2E_ID), required=True)
    p.add_argument("--mode", choices=["probe", "run"], default="run",
                   help="probe: open the application and observe, no input (dry server)")
    p.add_argument("--runs", type=int, default=1)
    p.add_argument("--first-run", type=int, default=1)
    p.add_argument("--max-live", type=int, default=0)
    p.add_argument("--ledger", type=Path, required=True)
    p.add_argument("--round-start", type=int, default=None)
    p.add_argument("--results", type=Path, required=True, help="JSONL file to append runs to")
    p.add_argument("--editor-exe",
                   default=str(HERE / "testeditor" / "target" / "release" / "ommatix-testeditor.exe"))
    p.add_argument("--explorer-launch", choices=["root", "e"], default="e",
                   help="explorer.exe /e,<folder> (the default: the address bar shows the whole "
                        "path, so the breadcrumb of every parent exists) or /root,<folder> "
                        "(the navigation pane is rooted at the folder; whether it hides the "
                        "parents in the breadcrumb is not verified)")
    p.add_argument("--explorer-back-name", default=EXPLORER_BACK,
                   help="the exact UI Automation name of the Back button")
    p.add_argument("--explorer-back-id", default="",
                   help="the Back button's AutomationId, once the probe has recorded it "
                        "(optional; when set the Back button must also carry it)")
    p.add_argument("--session-id", default=None,
                   help="probe runs only: the engine's session identifier (a live run takes none)")
    p.add_argument("--output", type=int, default=0)
    p.add_argument("--foreground-wait", type=float, default=60.0)
    p.add_argument("--operator-activation", action="store_true",
                   help="off by default: when the window cannot be brought to the front under the "
                        "Windows foreground lock, print to stderr that the operator must click the "
                        "flashing taskbar button of the application, and wait (bounded by "
                        "--operator-activation-wait) until the recorded window is the foreground "
                        "window; nothing is sent before that")
    p.add_argument("--operator-activation-wait", type=float, default=120.0)
    p.add_argument("--foreground-wait-explorer", type=float, default=15.0,
                   help="seconds to wait for the new Explorer window to appear")
    p.add_argument("--pause", type=float, default=1.0)
    p.add_argument("--settle", type=float, default=1.5)
    p.add_argument("--focus-wait", type=float, default=5.0,
                   help="--app decoy: seconds to wait, read-only, for UI Automation to report the "
                        "keyboard focus inside the editor before any request; then no request")
    p.add_argument("--operator-focus", action="store_true",
                   help="--app decoy: when the editor lacks the keyboard focus, the operator gives it "
                        "by hand (its taskbar button, no typing), waiting --operator-focus-wait")
    p.add_argument("--operator-focus-wait", type=float, default=120.0)
    p.add_argument("--debug-grounding", action="store_true")
    args = p.parse_args()
    args.restart = "each"

    try:
        driver.freshness_config(args)
    except (Abort, OSError, ValueError):
        driver.log("abort", reason="cannot prepare the M2 configuration copy")
        return 2
    raw = args.config.read_bytes()
    config = tomllib.loads(raw.decode("utf-8"))
    stop = config.get("safety", {}).get("stop_file") or None
    stop_file = (args.config.parent / stop) if stop else None
    if stop_file is None:
        driver.log("abort", reason="the configuration has no stop_file")
        return 2
    if stop_file.exists():
        driver.log("abort", reason="the stop file exists before the run")
        return 2
    live = args.mode == "run"
    if live:
        problems = config_problems(args.app, config)
        if problems:
            driver.log("abort", reason="the configuration does not fit this flow",
                       problems=problems)
            return 2
    ledger_now = driver.read_ledger(args.ledger)
    budget = decoy_budget_problem(args.app, live, args.max_live)
    if budget:
        driver.log("abort", reason=budget)
        return 2
    problem = driver.round_problem(live, args.round_start, ledger_now)
    if problem:
        driver.log("abort", reason=problem)
        return 2
    if args.round_start is None:
        args.round_start = ledger_now
    pin_file = pin_file_path(args.config, config) if args.app == "explorer" else None
    if pin_file is not None and pin_file.exists():
        driver.log("abort", reason="the pin file exists before the run")
        return 2
    try:
        argv = dossier_serve_argv(args, live)
    except Abort as error:
        driver.log("abort", reason=str(error))
        return 2
    if args.app == "editor":
        flow = editor_flow()
    elif args.app == "decoy":
        flow = decoy_flow()
    else:
        flow = explorer_flow("(set per run)")
    driver.log("start", suite="dossier", app=args.app, mode=args.mode, runs=args.runs,
               live_input=live, config_sha256=hashlib.sha256(raw).hexdigest(),
               max_live=args.max_live, round_start=args.round_start, ledger_start=ledger_now,
               round_cap=driver.LIVE_CAP)
    srv = s3.Server(argv)
    proto = DossierProtocol(args, srv, stop_file, flow, args.ledger, pin_file=pin_file)
    code = 0
    try:
        m = proto.metrics()
        driver.log("metrics", mode=m.get("mode"), models_loaded=m.get("models_loaded"))
        proto.run()
    except Abort as e:
        driver.log("abort", reason=str(e))
        code = 1
    finally:
        for started in list(proto.started):
            proto.close(started)
        exit_code = srv.close()
        keep = ("ACTIVE CONTROL", "STOP engaged", "stop cleared", "source=")
        for line in srv.stderr:
            if any(k in line for k in keep):
                driver.log("stderr", line=line[:300])
        ok = sum(r["success"] for r in proto.results)
        driver.log("end", server_exit=exit_code, runs=len(proto.results), successes=ok,
                   live_requests=proto.live_sent, ledger_start=ledger_now,
                   ledger_end=proto.ledger_total(), round_start=args.round_start,
                   round_live_requests=proto.round_live(), ledger_total=proto.ledger_total())
    return code


if __name__ == "__main__":
    sys.exit(main())
