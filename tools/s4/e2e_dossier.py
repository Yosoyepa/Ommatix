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

RUN_ROOT = "ommatix-e2e"
RUN_ID_LIMIT = 64
E2E_ID = {"editor": "E2E-02-plan", "explorer": "E2E-03-plan"}

EDITOR_EXE = "ommatix-testeditor.exe"
EDITOR_FIELD = "Editor de texto"
EDITOR_SAVE = "Guardar"
EDITOR_NOTE = "nota.txt"
EDITOR_TEXT = driver.TYPED_TEXT  # synthetic: "prueba 123"

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
GA_ROOT = 2
user32.WindowFromPoint.argtypes = [wintypes.POINT]
user32.WindowFromPoint.restype = wintypes.HWND
user32.GetAncestor.argtypes = [wintypes.HWND, wintypes.UINT]
user32.GetAncestor.restype = wintypes.HWND
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
    wanted = EDITOR_EXE if app == "editor" else EXPLORER_EXE
    if wanted not in allowed:
        problems.append(f"[safety] allowed_applications does not list {wanted}")
    classes = safety.get("allowed_window_classes", {})
    if app == "editor":
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


def editor_title(chars: int, saved: bool) -> str:
    return f"Ommatix S4 editor [caracteres={chars} guardado={'si' if saved else 'no'}]"


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


def root_window_at(point: tuple[int, int]) -> int | None:
    """The top-level window that receives a click at `point` (WindowFromPoint, GA_ROOT)."""
    hwnd = user32.WindowFromPoint(wintypes.POINT(int(point[0]), int(point[1])))
    if not hwnd:
        return None
    root = user32.GetAncestor(hwnd, GA_ROOT)
    return int(root or hwnd)


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

    def window_under(self, point: tuple[int, int]) -> int | None:
        return root_window_at(point)

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
        if self.args.app == "editor":
            return editor_state(self.title_of(hwnd),
                                note_state(self.tree.root, EDITOR_TEXT.encode("utf-8")))
        return relative_parts(self.tree.root, self.location_of(self.explorer_hwnd))

    def require_foreground(self) -> dict:
        if self.args.app == "editor":
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
        res.update(suite="dossier")
        started = time.perf_counter()
        res["user_idle_s_at_start"] = driver.idle_seconds()
        self.tree = None
        self.explorer_hwnd = None
        self.editor_pid = None
        try:
            if self.args.app == "editor":
                self.run_editor(res, n)
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
            if seen and (self.args.app != "editor"
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
            if self.args.app == "editor":
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
    p.add_argument("--session-id", default="s4d")
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
    p.add_argument("--debug-grounding", action="store_true")
    args = p.parse_args()
    args.restart = "each"

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
    argv = [args.exe, "serve", "--source", "dxgi", "--output", str(args.output),
            "--config", str(args.config), "--session-id", args.session_id]
    if live:
        argv += ["--input", "sendinput"]
    flow = editor_flow() if args.app == "editor" else explorer_flow("(set per run)")
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
