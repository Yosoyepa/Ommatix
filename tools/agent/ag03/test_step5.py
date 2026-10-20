"""Regressions for the independent step-5 review of AG-03 (S1-S5; docs/reviews/2026-10-20-ag03-step5.md).

Written and run RED against f16bb60 before the fixes (output kept beside the prompt). Fakes only: the native
boundary is a set of Python objects that stand in for ``ctypes.WinDLL`` (no DLL is loaded, no desktop, window,
foreground, key state or job exists), the input sender records batches and never sends, and the child process of
S3 is a Python object. No screen, no input, no provider, no key.

S1 a prepared click is bound to the virtual-desktop geometry its coordinates were normalised on.
S2 the final native evidence closes with a foreground read; focus is read for the foreground thread.
S3 ownership of the CLI is recorded BEFORE it exists; interrupting its start never releases the campaign lock
   unless the child is positively shown gone.
S4 a key-state read that cannot be attested (input desktop) is unknown, never "no modifier held".
S5 every native function the code calls has its signature declared.
"""

import ast
import contextlib
import ctypes
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from ctypes import wintypes
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import fake_os as fake  # noqa: E402
import pixel_guards as g  # noqa: E402
import pixel_runner as pr  # noqa: E402
import pixel_server as ps  # noqa: E402
import pixel_win32 as w32  # noqa: E402
import run_goal as rg  # noqa: E402

TARGET_PID = 4242


def make_server(os_, sender, clock):
    records, reservations = [], []
    server = ps.PixelServer(
        ps.ServerConfig(phase=2, synthetic=False, attested=g.Attested(TARGET_PID, g.TESTAPP_EXE)),
        ps.Deps(clock=clock, os=os_, sender=sender, reserve=lambda: reservations.append(1), log=records.append))
    server.shot = g.Shot(0, 100, (0, 0, 4, 3), 4, 3)
    return server, records, reservations


def error_code(answer):
    return json.loads(answer["content"][0]["text"]).get("error", {}).get("code")


# ---- S1: the geometry that gives the normalised coordinates their meaning -------------------------------------

class DesktopOs(fake.FakeOs):
    """A fake operating system whose virtual-desktop geometry the tests change (or make unreadable)."""

    def __init__(self, clock):
        super().__init__(clock, fake.window_4x3())
        self.layout = (0, 0, 1920, 1080)

    def virtual_screen(self):
        if self.layout is None:
            raise OSError("fake: the virtual screen cannot be read")
        return self.layout


UNSET = object()  # "no change" (None is a change: an unreadable geometry)


class S1VirtualDesktopTests(unittest.TestCase):
    LAYOUTS = {"origin": (-1920, 0, 1920, 1080), "width": (0, 0, 3840, 1080), "height": (0, 0, 1920, 2160),
               "origin_and_width": (-1920, 0, 3840, 1080), "unreadable": None}

    def run_click(self, change_in_prepare=UNSET, change_at_final_read=UNSET):
        clock = fake.FakeClock()
        os_ = DesktopOs(clock)
        sender = fake.FakeSender()
        if change_in_prepare is not UNSET:
            sender.on_prepare = lambda: setattr(os_, "layout", change_in_prepare)
        if change_at_final_read is not UNSET:
            os_.hooks[2] = lambda o: setattr(o, "layout", change_at_final_read)
        server, records, reservations = make_server(os_, sender, clock)
        return server, server.call_tool("click", {"x": 2, "y": 1}), sender, records, reservations

    def test_unchanged_geometry_delivers_with_coordinates_normalised_on_it(self):
        server, answer, sender, records, reservations = self.run_click()
        self.assertEqual(len(sender.batches), 1)
        move = sender.batches[0][0]
        self.assertEqual((move["dx"], move["dy"]), w32.normalise_absolute(2, 1, (0, 0, 1920, 1080)))
        self.assertTrue(records[-1].get("delivered"))

    def test_a_desktop_change_while_the_batch_is_prepared_sends_nothing(self):
        for name, layout in self.LAYOUTS.items():
            with self.subTest(change=name):
                server, answer, sender, records, reservations = self.run_click(change_in_prepare=layout)
                self.assertEqual(sender.batches, [], "coordinates were sent after their desktop changed")
                self.assertEqual(len(reservations), 1, "the reservation is kept")
                self.assertEqual(server.terminal, "safety")
                self.assertIn(error_code(answer), ("DESKTOP_CHANGED", "DESKTOP_UNREADABLE"))

    def test_a_desktop_change_at_the_final_read_sends_nothing(self):
        for name, layout in self.LAYOUTS.items():
            with self.subTest(change=name):
                server, answer, sender, records, reservations = self.run_click(change_at_final_read=layout)
                self.assertEqual(sender.batches, [])
                self.assertEqual(len(reservations), 1)
                self.assertEqual(server.terminal, "safety")

    def test_a_point_the_desktop_cannot_hold_is_a_refusal_not_a_crash(self):
        clock = fake.FakeClock()
        os_ = DesktopOs(clock)
        os_.layout = (0, 0, 2, 2)  # the click maps to (2, 1): outside this desktop
        sender = fake.FakeSender()
        server, records, reservations = make_server(os_, sender, clock)
        answer = server.call_tool("click", {"x": 2, "y": 1})  # must answer, not raise
        self.assertEqual(sender.batches, [])
        self.assertEqual(len(reservations), 1)
        self.assertEqual(error_code(answer), "INPUT_NOT_PREPARED")


# ---- a fake native boundary shared by S2, S4 and S5 --------------------------------------------------------------

class Fn:
    """One fake native function: it records which parts of its signature were declared, the call order, and can
    flip the foreground after the Nth native call overall."""

    def __init__(self, world, name, action):
        self.world, self.name, self.action = world, name, action
        self.declared, self._argtypes, self._restype = set(), None, ctypes.c_int

    argtypes = property(lambda self: self._argtypes)
    restype = property(lambda self: self._restype)

    @argtypes.setter
    def argtypes(self, value):
        self._argtypes = value
        self.declared.add("argtypes")

    @restype.setter
    def restype(self, value):
        self._restype = value
        self.declared.add("restype")

    def __call__(self, *args):
        result = self.action(*args)
        self.world.calls += 1
        self.world.trace.append(self.name)
        if self.world.flip_at == self.world.calls:
            self.world.fg = 900
        return result


class Dll:
    def __init__(self, world, actions):
        self.world, self.fns = world, {name: Fn(world, name, action) for name, action in actions.items()}

    def __getattr__(self, name):
        if name.startswith("__"):
            raise AttributeError(name)
        if name not in self.fns:
            self.fns[name] = Fn(self.world, name, lambda *args: 1)
        return self.fns[name]


class NativeWorld:
    """user32/kernel32/advapi32/gdi32/shcore as Python objects. Window 100 is the target (process 4242), 900 a
    foreign window (process 9999). ``held`` are modifier virtual-key codes that read as down."""

    def __init__(self):
        self.fg, self.calls, self.flip_at, self.trace = 100, 0, None, []
        self.held = set()
        self.metrics = {76: 0, 77: 0, 78: 1920, 79: 1080}
        self.thread_desktop, self.input_desktop, self.names_readable = "Default", "Default", True
        self.gui_threads = []
        self.open_desktop_access = []  # the access masks asked of OpenInputDesktop
        self.denied_access_bits = 0  # access bits the fake input desktop refuses (a restricted desktop)
        self.caller_integrity = 0x2000  # the runner's mandatory-integrity RID (medium); None: unreadable
        w = self

        def pid_of(hwnd):
            return TARGET_PID if hwnd in (100, 101) else 9999

        def process(hwnd, output):
            if output is not None:
                output._obj.value = pid_of(hwnd)
            return 777 if hwnd in (100, 101) else 888

        def cls(hwnd, output, size):
            output.value = "TestAppWindow" if hwnd in (100, 101) else "ForeignClass"
            return len(output.value)

        def rect(hwnd, output):
            output._obj.left, output._obj.top, output._obj.right, output._obj.bottom = 0, 0, 4, 3
            return 1

        def origin(hwnd, output):
            output._obj.x, output._obj.y = 0, 0
            return 1

        def monitor_info(handle, output):
            ctypes.cast(output, ctypes.POINTER(ctypes.c_uint32))[9] = 1
            return 1

        def image(handle, flags, output, size):
            # forward slashes: ``os.path.basename`` of the (Windows-only) production code splits them on every platform
            output.value = "C:/synthetic/ommatix-testapp.exe" if handle == TARGET_PID else "C:/synthetic/foreign.exe"
            return 1

        def open_token(handle, access, output):
            output._obj.value = 2
            return 1

        def token_info(handle, kind, output, size, returned):
            if kind == 25:  # TokenIntegrityLevel of the caller: a TOKEN_MANDATORY_LABEL whose first field is the SID
                if w.caller_integrity is None:
                    return 0
                ctypes.c_void_p.from_buffer(output).value = 0x1000  # a fake SID address, never dereferenced
                returned._obj.value = 16
                return 1
            output._obj.TokenIsElevated = 0
            returned._obj.value = 4
            return 1

        def open_input_desktop(flags, inherit, access):
            w.open_desktop_access.append(access)
            return 11 if w.input_desktop and not access & w.denied_access_bits else 0

        def gui_info(thread, output):
            w.gui_threads.append(thread)
            focus_owner = w.fg if thread == 0 else (100 if thread == 777 else 900)
            output._obj.hwndActive, output._obj.hwndFocus = focus_owner, focus_owner + 1
            return 1

        def user_object(handle, index, buffer, size, needed):
            if not w.names_readable:
                return 0
            buffer.value = {10: w.thread_desktop, 11: w.input_desktop}.get(handle) or ""
            return 1 if buffer.value else 0

        self.user = Dll(w, {
            "GetForegroundWindow": lambda: w.fg, "GetWindowThreadProcessId": process, "GetClassNameW": cls,
            "GetClientRect": rect, "ClientToScreen": origin, "MonitorFromWindow": lambda *a: 1,
            "GetMonitorInfoW": monitor_info, "IsIconic": lambda *a: 0, "GetGUIThreadInfo": gui_info,
            "GetAncestor": lambda h, flags: 100 if h in (100, 101) else 900, "WindowFromPoint": lambda *a: 100,
            "GetAsyncKeyState": lambda vk: -32768 if vk in w.held else 0,
            "GetSystemMetrics": lambda index: w.metrics[index],
            "OpenInputDesktop": open_input_desktop,
            "GetThreadDesktop": lambda tid: 10 if w.thread_desktop else 0,
            "GetUserObjectInformationW": user_object, "CloseDesktop": lambda h: 1})
        self.kernel = Dll(w, {"OpenProcess": lambda access, inherit, pid: pid, "QueryFullProcessImageNameW": image,
                              "CloseHandle": lambda h: 1, "GetCurrentThreadId": lambda: 5,
                              "GetCurrentProcess": lambda: -1})
        self.advapi = Dll(w, {"OpenProcessToken": open_token, "GetTokenInformation": token_info,
                              "GetSidSubAuthorityCount": lambda sid: ctypes.pointer(ctypes.c_ubyte(1)),
                              "GetSidSubAuthority": lambda sid, index: ctypes.pointer(
                                  ctypes.c_uint32(w.caller_integrity or 0))})
        self.gdi = Dll(w, {})
        self.shcore = Dll(w, {})

    def windll(self, name, **kwargs):
        return {"user32": self.user, "kernel32": self.kernel, "advapi32": self.advapi, "gdi32": self.gdi,
                "shcore": self.shcore}[name]

    def run(self, tool, args):
        """One tool call on the real ``RealOs`` over this fake boundary and a recording sender."""
        clock, sender = fake.FakeClock(), fake.FakeSender()

        def missing(path):
            raise FileNotFoundError(path)
        server, records, reservations = make_server(w32.RealOs("synthetic-stop", stat=missing), sender, clock)
        with mock.patch.object(ctypes, "WinDLL", side_effect=self.windll, create=True):
            answer = server.call_tool(tool, args)
        return server, answer, sender, records, reservations


# ---- S2: the foreground is read LAST --------------------------------------------------------------------------

class S2ForegroundIsReadLastTests(unittest.TestCase):
    def count_native_calls(self):
        world = NativeWorld()
        server, answer, sender, records, reservations = world.run("type_text", {"text": "a"})
        self.assertEqual(len(sender.batches), 1, "the clean control must deliver")
        return world.calls, world

    def test_a_foreground_change_anywhere_before_the_final_closing_read_sends_no_keys(self):
        total, _ = self.count_native_calls()
        sent_when_flipped_after = []
        for flip_at in range(1, total + 1):
            with self.subTest(flipped_after_native_call=flip_at):
                world = NativeWorld()
                world.flip_at = flip_at
                server, answer, sender, records, reservations = world.run("type_text", {"text": "a"})
                if sender.batches:
                    sent_when_flipped_after.append(flip_at)
                else:
                    self.assertEqual(len(reservations), 1, "the reservation is kept")
                    self.assertEqual(server.terminal, "safety")
        # The one residual window - after the last query, before the OS dispatches - cannot be closed from user
        # mode: a change after the very last native read is the only case that is not seen.
        self.assertEqual(sent_when_flipped_after, [total], "a foreground change was missed before the last read")

    def test_a_foreground_change_inside_the_final_process_and_token_queries_sends_no_keys(self):
        world = NativeWorld()
        original = world.advapi.fns["GetTokenInformation"].action
        queries = []

        def token_info(handle, kind, output, size, returned):
            queries.append(1)
            if len(queries) == 2:  # the facts of the FINAL evidence (the initial one made the first query)
                world.fg = 900
            return original(handle, kind, output, size, returned)
        world.advapi.fns["GetTokenInformation"].action = token_info
        server, answer, sender, records, reservations = world.run("type_text", {"text": "a"})
        self.assertEqual(sender.batches, [], "keys were sent while the foreground was another application")
        self.assertEqual(len(reservations), 1)

    def test_keyboard_focus_is_read_for_the_current_foreground_thread(self):
        world = NativeWorld()
        server, answer, sender, records, reservations = world.run("type_text", {"text": "a"})
        self.assertEqual(len(sender.batches), 1)
        self.assertTrue(world.gui_threads)
        self.assertEqual(set(world.gui_threads), {0}, "focus was queried for a specified (possibly old) thread")

    def test_a_slow_final_read_is_stale_and_sends_nothing(self):
        clock = fake.FakeClock()
        os_ = fake.FakeOs(clock, fake.window_4x3())
        sender = fake.FakeSender()
        os_.hooks[2] = lambda o: clock.advance(5.0)  # the final read takes 5 s on the injected clock
        server, records, reservations = make_server(os_, sender, clock)  # the screenshot (10 s bound) is still fresh
        answer = server.call_tool("click", {"x": 2, "y": 1})
        self.assertEqual(sender.batches, [])
        self.assertEqual(len(reservations), 1)
        self.assertEqual(server.terminal, "safety")


# ---- S4: a key-state read nobody attested is unknown -------------------------------------------------------------

class S4ModifierReadIsAttestedTests(unittest.TestCase):
    def test_idle_keys_on_the_attested_input_desktop_deliver(self):
        world = NativeWorld()
        server, answer, sender, records, reservations = world.run("type_text", {"text": "a"})
        self.assertEqual(len(sender.batches), 1)
        self.assertTrue(records[-1].get("delivered"))

    def test_a_held_modifier_refuses(self):
        world = NativeWorld()
        world.held = {0x11}
        server, answer, sender, records, reservations = world.run("type_text", {"text": "a"})
        self.assertEqual(sender.batches, [])
        self.assertEqual(error_code(answer), "MODIFIER_HELD")

    def test_the_same_all_zero_key_states_without_an_attested_input_desktop_refuse(self):
        cases = {"no input desktop": {"input_desktop": None}, "no thread desktop": {"thread_desktop": None},
                 "another desktop receives input": {"input_desktop": "Winlogon"},
                 "names unreadable": {"names_readable": False}}
        for name, change in cases.items():
            with self.subTest(case=name):
                world = NativeWorld()
                for attribute, value in change.items():
                    setattr(world, attribute, value)
                server, answer, sender, records, reservations = world.run("type_text", {"text": "a"})
                self.assertEqual(sender.batches, [], "all-zero key states were taken as 'no modifier held'")
                self.assertEqual(error_code(answer), "MODIFIERS_UNREADABLE")
                self.assertEqual(len(reservations), 1)
                self.assertEqual(server.terminal, "safety")


# ---- S5: every native signature is declared ----------------------------------------------------------------------

CALLED_DLL_NAMES = {"user32", "kernel32", "advapi32", "gdi32", "shcore"}


def called_native_functions(path):
    """{dll variable: {function}} of the ``dll.Function(...)`` calls and ``dll.Function`` loads in a source file."""
    found = {}
    for node in ast.walk(ast.parse(Path(path).read_text(encoding="utf-8"))):
        if (isinstance(node, ast.Attribute) and isinstance(node.value, ast.Name)
                and node.value.id in CALLED_DLL_NAMES and node.attr[:1].isupper()):
            found.setdefault(node.value.id, set()).add(node.attr)
    return found


class S5DeclarationTests(unittest.TestCase):
    def declared_after(self, run):
        world = NativeWorld()
        with mock.patch.object(ctypes, "WinDLL", side_effect=world.windll, create=True):
            run()
        return world

    def test_every_pixel_win32_function_has_both_parts_of_its_signature_declared(self):
        called = called_native_functions(w32.__file__)
        self.assertTrue(called["user32"] and called["kernel32"])
        world = NativeWorld()
        with mock.patch.object(ctypes, "WinDLL", side_effect=world.windll, create=True):
            w32._dlls()  # noqa: SLF001
            shcore_loader = getattr(w32, "_shcore", None)
            if shcore_loader is not None:
                shcore_loader()
        by_dll = {"user32": world.user, "kernel32": world.kernel, "advapi32": world.advapi, "gdi32": world.gdi,
                  "shcore": world.shcore}
        declared = {(dll, name): fn.declared for dll, d in by_dll.items() for name, fn in d.fns.items()}
        missing = [f"{dll}.{name}: {sorted({'argtypes', 'restype'} - declared.get((dll, name), set()))}"
                   for dll, names in called.items() for name in sorted(names)
                   if declared.get((dll, name)) != {"argtypes", "restype"}]
        self.assertEqual(missing, [], "native functions called without a full signature")

    def test_the_unsigned_and_short_returns_are_declared_exactly(self):
        world = NativeWorld()
        with mock.patch.object(ctypes, "WinDLL", side_effect=world.windll, create=True):
            w32._dlls()  # noqa: SLF001
        self.assertIs(world.user.SendInput.restype, wintypes.UINT)
        self.assertIs(world.user.GetWindowThreadProcessId.restype, wintypes.DWORD)
        self.assertIs(world.user.GetAsyncKeyState.restype, ctypes.c_short)

    def test_a_full_width_handle_survives_the_declared_conversion(self):
        handle = 0x100000123  # does not fit in 32 bits
        self.assertNotEqual(ctypes.c_int(handle).value, handle, "the default int conversion truncates it")
        job_dll = self.job_dll_after_open_job()
        declared = job_dll.CloseHandle.argtypes
        self.assertIsNotNone(declared, "CloseHandle has no declared argtypes")
        self.assertEqual(len(declared), 1)
        self.assertEqual(declared[0](handle).value, handle, "CloseHandle must take a pointer-sized handle")
        # (a pure ctypes conversion: this is not evidence of a kernel call)

    def job_dll_after_open_job(self):
        world = NativeWorld()
        kernel = Dll(world, {"CreateJobObjectW": lambda *a: 0x100000123, "SetInformationJobObject": lambda *a: 1})
        tree = object.__new__(rg.OwnedTree)
        tree.job = None
        with mock.patch.object(ctypes, "WinDLL", return_value=kernel, create=True):
            tree._open_job()  # noqa: SLF001
        self.assertEqual(tree.job, 0x100000123)
        return kernel

    def test_every_job_function_the_owned_tree_calls_is_fully_declared(self):
        kernel = self.job_dll_after_open_job()
        source = ast.parse(Path(rg.__file__).read_text(encoding="utf-8"))
        owned_tree = next(n for n in source.body if isinstance(n, ast.ClassDef) and n.name == "OwnedTree")
        called = {node.attr for node in ast.walk(owned_tree) if isinstance(node, ast.Attribute)
                  and node.attr[:1].isupper() and (
                      (isinstance(node.value, ast.Name) and node.value.id in ("k32", "ntdll"))
                      or (isinstance(node.value, ast.Attribute) and node.value.attr == "_k32"))}
        called -= {"NtResumeProcess"}  # declared on ntdll, checked below
        self.assertTrue({"CreateJobObjectW", "SetInformationJobObject", "AssignProcessToJobObject",
                         "TerminateJobObject", "QueryInformationJobObject", "CloseHandle"} <= called)
        undeclared = sorted(name for name in called if kernel.fns.get(name) is None
                            or kernel.fns[name].declared != {"argtypes", "restype"})
        self.assertEqual(undeclared, [], "job functions called without a full signature")


# ---- S3: ownership is recorded before the CLI exists -------------------------------------------------------------

class JobWorld:
    """A fake CLI child, job object and ntdll. ``interrupt`` names the native step that raises KeyboardInterrupt:
    popen_oserror (nothing created), popen (child created, then interrupted), create, limit, assign, resume
    (the child is running when it raises) or resume_return (the resume completed, then the interruption)."""

    def __init__(self, interrupt=None, kill_works=True, query_works=True, close_kills=True):
        self.interrupt, self.kill_works, self.query_works, self.close_kills = interrupt, kill_works, query_works, close_kills
        self.exists = self.alive = self.running = False
        self.job_open = self.kill_on_close = self.assigned = False
        self.popen_calls = 0
        self.calls, self.trace, self.flip_at = 0, [], None  # what ``Fn`` records
        world = self

        def raise_if(step):
            if world.interrupt == step:
                raise KeyboardInterrupt(f"synthetic interruption at {step}")

        def create(*args):
            raise_if("create")
            world.job_open = True
            return 0x100000123

        def set_information(job, kind, info, size):
            raise_if("limit")
            world.kill_on_close = bool(info._obj.Basic.LimitFlags & rg.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)
            return 1

        def assign(job, process):
            raise_if("assign")
            world.assigned = True
            return 1

        def query(job, kind, info, size, returned):
            if not world.query_works:
                return 0
            info._obj.ActiveProcesses = 1 if world.running else 0
            return 1

        def terminate(job, code):
            if world.kill_works:
                world.running = world.alive = False
            return 1

        def close(handle):
            world.job_open = False
            if world.close_kills and world.kill_on_close and world.assigned:
                world.running = world.alive = False
            return 1

        def resume(process):
            if world.interrupt == "resume":
                world.running = True  # the native call took effect, then the interruption arrived
            raise_if("resume")
            world.running = True
            return 0

        self.kernel = Dll(self, {"CreateJobObjectW": create, "SetInformationJobObject": set_information,
                                 "AssignProcessToJobObject": assign, "QueryInformationJobObject": query,
                                 "TerminateJobObject": terminate, "CloseHandle": close})
        self.ntdll = Dll(self, {"NtResumeProcess": resume})
        self.proc = None

    def windll(self, name, **kwargs):
        return self.kernel if name == "kernel32" else self.ntdll

    def popen(self, argv, **kwargs):
        self.popen_calls += 1
        if self.interrupt == "popen_oserror":  # CreateProcess refused: the program is not there
            raise FileNotFoundError("fake: cannot start the CLI")
        self.exists = self.alive = True  # created suspended: alive but running no instruction
        self.proc = FakeProc(self)
        if self.interrupt == "popen":
            raise KeyboardInterrupt("synthetic interruption inside Popen")
        if self.interrupt == "popen_post_create_oserror":  # the child exists; Popen own clean-up then fails
            raise OSError("fake: failure AFTER a successful CreateProcess, before Popen returned")
        return self.proc


class FakeProc:
    def __init__(self, world):
        self.world, self._handle, self.returncode = world, 0x100000456, None
        self.stdin, self.stdout = io.BytesIO(), io.BytesIO()
        self.pid = 2 ** 22 + 7  # never used: this class is only for the Windows branch

    def poll(self):
        return None if self.world.alive else (self.returncode if self.returncode is not None else -9)

    def kill(self):
        if self.world.kill_works:
            self.world.alive = self.world.running = False

    def wait(self, timeout=None):
        if self.world.kill_works and self.world.alive and self.world.running and timeout is not None:
            self.world.alive = self.world.running = False  # the fake CLI finishes on its own
            self.returncode = 0
        if self.world.alive:
            if timeout is None:
                raise AssertionError("fake: a wait with no timeout on a live child would block forever")
            raise subprocess.TimeoutExpired("fake-cli", timeout)
        self.returncode = 0 if self.returncode is None else self.returncode
        return self.returncode


@unittest.skipUnless(os.name == "nt", "the job object is a Windows mechanism; the fakes stand in for it")
class S3OwnershipBeforeTheChildExistsTests(unittest.TestCase):
    def start(self, world, owned, after_resume=None):
        metrics = rg.StreamMetrics([], None, None)
        patches = [mock.patch.object(subprocess, "Popen", side_effect=world.popen),
                   mock.patch.object(ctypes, "WinDLL", side_effect=world.windll, create=True),
                   mock.patch.object(rg, "TREE_KILL_SETTLE_S", 0.2, create=True),
                   mock.patch.object(rg, "START_ABANDON_SETTLE_S", 0.2, create=True)]
        if after_resume == "interrupt":
            real = rg.OwnedTree._resume  # noqa: SLF001

            def resume_then_interrupt(self):
                real(self)
                raise KeyboardInterrupt("synthetic interruption after the native resume returned")
            patches.append(mock.patch.object(rg.OwnedTree, "_resume", resume_then_interrupt))
        with contextlib.ExitStack() as stack:
            for patch in patches:
                stack.enter_context(patch)
            try:
                rg.run_claude(["fake-cli"], "", Path(tempfile.gettempdir()), metrics, 5.0, env={}, ownership=owned)
            except (KeyboardInterrupt, OSError, rg.OwnedSessionNotQuiescent) as error:
                return error
        return None

    def assert_invariant(self, world, owned):
        """The ONLY thing that lets a caller release the campaign lock is a counter of zero."""
        if owned["unverified"] == 0:
            self.assertFalse(world.running, "the lock may be released with a CLI that is running")
            self.assertFalse(world.alive, "the lock may be released with a CLI that is alive")

    def test_a_clean_session_ends_with_nothing_unverified(self):
        world, owned = JobWorld(), {"unverified": 0}
        self.assertIsNone(self.start(world, owned))
        self.assertEqual(owned["unverified"], 0)
        self.assert_invariant(world, owned)
        self.assertTrue(world.kill_on_close)

    def test_a_start_that_cannot_create_the_child_owns_nothing(self):
        world, owned = JobWorld("popen_oserror"), {"unverified": 0}
        self.assertIsInstance(self.start(world, owned), OSError)
        self.assertEqual(owned["unverified"], 0)
        self.assertFalse(world.exists)

    def test_an_interruption_inside_popen_keeps_the_lock(self):
        world, owned = JobWorld("popen"), {"unverified": 0}
        self.assertIsInstance(self.start(world, owned), (KeyboardInterrupt, rg.OwnedSessionNotQuiescent))
        self.assertTrue(world.exists, "the child may exist: nothing proves otherwise")
        self.assertGreaterEqual(owned["unverified"], 1)
        self.assert_invariant(world, owned)

    def test_an_interruption_at_every_step_of_the_start_never_frees_a_live_child(self):
        for step in ("create", "limit", "assign", "resume", "resume_return"):
            with self.subTest(step=step):
                world, owned = JobWorld(None if step == "resume_return" else step), {"unverified": 0}
                error = self.start(world, owned, after_resume="interrupt" if step == "resume_return" else None)
                self.assertIsInstance(error, (KeyboardInterrupt, rg.OwnedSessionNotQuiescent))
                self.assert_invariant(world, owned)

    def test_an_interrupted_start_is_ended_and_proven_before_the_counter_drops(self):
        for step in ("create", "limit", "assign", "resume", "resume_return"):
            with self.subTest(step=step):
                world, owned = JobWorld(None if step == "resume_return" else step), {"unverified": 0}
                self.start(world, owned, after_resume="interrupt" if step == "resume_return" else None)
                self.assertFalse(world.alive or world.running, "the interrupted start left a child behind")
                self.assertEqual(owned["unverified"], 0, "a proven-gone child lets the lock go")
                self.assertFalse(world.job_open, "the job handle was closed")

    def test_a_child_that_cannot_be_shown_gone_keeps_the_counter(self):
        for name, options in {"kill fails": dict(kill_works=False, close_kills=False),
                              "job cannot be queried": dict(query_works=False)}.items():
            with self.subTest(case=name):
                world, owned = JobWorld("resume", **options), {"unverified": 0}
                error = self.start(world, owned)
                self.assertIsInstance(error, (KeyboardInterrupt, rg.OwnedSessionNotQuiescent))
                self.assertGreaterEqual(owned["unverified"], 1)
                self.assert_invariant(world, owned)


@unittest.skipIf(os.name == "nt", "a real child with a process group: no job object, no Win32; Windows uses the fakes")
class S3PosixInterruptedPopenTests(unittest.TestCase):
    """The same invariant where the owned tree is a process group (the CI platform): an interruption that lands inside
    ``Popen`` may leave a child behind that nobody holds, so the counter must not drop."""

    def test_an_interruption_inside_popen_never_lowers_the_counter_of_a_child_that_may_exist(self):
        children, owned = [], {"unverified": 0}
        real = subprocess.Popen

        def popen_then_interrupt(*args, **kwargs):
            children.append(real(*args, **kwargs))
            raise KeyboardInterrupt("synthetic interruption inside Popen")
        try:
            with mock.patch.object(subprocess, "Popen", popen_then_interrupt):
                with self.assertRaises((KeyboardInterrupt, rg.OwnedSessionNotQuiescent)):
                    rg.run_claude([sys.executable, "-c", "import time; time.sleep(60)"], "",
                                  Path(tempfile.gettempdir()), rg.StreamMetrics([], None, None), 5.0, env={},
                                  ownership=owned)
            self.assertEqual(len(children), 1)
            if children[0].poll() is None:  # alive: the lock must be kept
                self.assertGreaterEqual(owned["unverified"], 1, "the lock may be released with a live child")
        finally:
            for child in children:
                child.kill()
                child.wait()

    def test_a_command_that_cannot_start_owns_nothing(self):
        owned = {"unverified": 0}
        with self.assertRaises(OSError):
            rg.run_claude(["definitely-not-a-program-ommatix"], "", Path(tempfile.gettempdir()),
                          rg.StreamMetrics([], None, None), 5.0, env={}, ownership=owned)
        self.assertEqual(owned["unverified"], 0)


@unittest.skipUnless(os.name == "nt", "the job object is a Windows mechanism; the fakes stand in for it")
class S3LockDispositionTests(unittest.TestCase):
    def run_main(self, world, series, calls):
        class Deps(series.deps):
            @staticmethod
            def run_claude(argv_, prompt, cwd, metrics, timeout, *a, **k):
                calls.append(1)
                return rg.run_claude(["fake-cli"], "", cwd, metrics, 5.0, env={}, ownership=k.get("ownership"))
        (series.root / "folders").mkdir(exist_ok=True)
        stderr = io.StringIO()
        with mock.patch.object(subprocess, "Popen", side_effect=world.popen), \
                mock.patch.object(ctypes, "WinDLL", side_effect=world.windll, create=True), \
                mock.patch.object(rg, "TREE_KILL_SETTLE_S", 0.2, create=True), \
                mock.patch.object(rg, "START_ABANDON_SETTLE_S", 0.2, create=True), \
                mock.patch.object(rg, "ancestor_context", return_value=[]), \
                contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(stderr):
            try:
                pr.main(series.argv, deps=Deps, driver=object(), display=series.display)
            except (KeyboardInterrupt, rg.OwnedSessionNotQuiescent, OSError) as error:
                return error, stderr.getvalue()
        return None, stderr.getvalue()

    def test_an_interrupted_resume_with_a_proven_gone_child_releases_the_lock(self):
        from test_campaign import Series
        series = Series(self)
        world, calls = JobWorld("resume"), []
        error, _ = self.run_main(world, series, calls)
        self.assertIsInstance(error, KeyboardInterrupt)
        self.assertFalse(world.running or world.alive)
        self.assertFalse((series.root / "campaign" / "campaign.lock").exists())

    def test_an_interrupted_resume_with_an_unprovable_child_keeps_the_lock_and_blocks_a_second_run(self):
        from test_campaign import Series
        series = Series(self)
        world, calls = JobWorld("resume", kill_works=False, close_kills=False), []
        error, stderr = self.run_main(world, series, calls)
        self.assertIsInstance(error, (KeyboardInterrupt, rg.OwnedSessionNotQuiescent))
        self.assertTrue(world.running, "the fake child is still running")
        self.assertTrue((series.root / "campaign" / "campaign.lock").exists(), "the lock was released")
        self.assertIn("by hand", stderr)
        second, starts = Series(self, root=series.root), []
        with self.assertRaises(SystemExit):
            second.run(extra=["--out", str(series.root / "out2")])
        self.assertEqual((second.calls, second.app_calls, starts), ([], {}, []))


# ---- the pure decoders and guards the fixes added (written with the fixes: the functions are new) ----------------

class PureDecoderTests(unittest.TestCase):
    def test_modifier_states_count_only_with_an_attested_input_desktop(self):
        idle, ctrl = [0, 0, 0, 0, 0], [0, -32768, 0, 0, 0]  # GetAsyncKeyState: bit 15 set (negative as a SHORT) = down
        self.assertIs(w32.decode_modifiers(idle, True), False)
        self.assertIs(w32.decode_modifiers(ctrl, True), True)
        for attested in (False, None):
            self.assertIsNone(w32.decode_modifiers(idle, attested))
            self.assertIsNone(w32.decode_modifiers(ctrl, attested))
        for malformed in (None, [], [0] * 4, [0, 0, 0, 0, "0"], [0, 0, 0, 0, True]):
            self.assertIsNone(w32.decode_modifiers(malformed, True))

    def test_the_desktop_attestation_needs_two_equal_names(self):
        self.assertTrue(w32.decode_desktop_attestation("Default", "Default"))
        for thread, input_ in (("Default", "Winlogon"), (None, "Default"), ("Default", None), ("", ""), (None, None),
                               (1, 1)):
            self.assertFalse(w32.decode_desktop_attestation(thread, input_))

    def test_the_virtual_desktop_needs_four_integers_and_a_size(self):
        self.assertEqual(w32.decode_virtual_screen((-1920, 0, 3840, 1080)), (-1920, 0, 3840, 1080))
        for bad in (None, (0, 0, 0, 0), (0, 0, 1, 1080), (0, 0, 1920), (0, 0, 1920.0, 1080), (0, 0, True, 1080)):
            self.assertIsNone(w32.decode_virtual_screen(bad))

    def test_the_desktop_guard_and_the_read_time_guard(self):
        self.assertIsNone(g.guard_desktop((0, 0, 1920, 1080), (0, 0, 1920, 1080)))
        self.assertEqual(g.guard_desktop((0, 0, 1920, 1081), (0, 0, 1920, 1080)), "DESKTOP_CHANGED")
        self.assertEqual(g.guard_desktop(None, (0, 0, 1920, 1080)), "DESKTOP_UNREADABLE")
        self.assertEqual(g.guard_desktop((0, 0, 1920, 1080), None), "DESKTOP_UNREADABLE")
        self.assertIsNone(g.guard_read_duration(1.0, 1.5))
        self.assertEqual(g.guard_read_duration(1.0, 2.5), "EVIDENCE_STALE")
        self.assertEqual(g.guard_read_duration(2.0, 1.0), "EVIDENCE_STALE")
        for token in ("DESKTOP_CHANGED", "DESKTOP_UNREADABLE", "EVIDENCE_STALE"):
            self.assertEqual(g.kind_of(token), "safety")
            self.assertIn(token, ps.RESULT_CODES)


if __name__ == "__main__":
    unittest.main()
