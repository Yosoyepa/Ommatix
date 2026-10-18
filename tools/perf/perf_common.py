"""Shared helpers of the read-only supervised-session tools (``soak.py``, ``dpi_probe.py``).

Nothing here sends input of any kind. It reads the foreground window, the counters of one
process, the state of the operator's configuration, and talks MCP to a ``ommatix serve`` the
caller started. Windows calls are made through ``ctypes`` and only on Windows, so that the
pure parts can be tested anywhere.
"""
from __future__ import annotations

import concurrent.futures
import importlib.util
import json
import math
import os
import statistics
import subprocess
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
TESTAPP = REPO / "tools" / "s4" / "testapp" / "target" / "release" / "ommatix-testapp.exe"
DEFAULT_EXE = REPO / "target" / "release" / "ommatix.exe"
S3_DRIVER = REPO / "tools" / "s3" / "mcp_driver.py"
TIMINGS_KEY = "io.ommatix/timings"


# ---- pure helpers -------------------------------------------------------------------------

def percentile(values: list[float], q: float) -> float:
    """Nearest rank, as the S0 report and ``measure_latency.py`` do."""
    ordered = sorted(values)
    return ordered[max(0, math.ceil(q * len(ordered)) - 1)]


def summary(values: list[float]) -> dict:
    """n, median, p95, p99 and max of ``values`` (``{"n": 0}`` when empty)."""
    if not values:
        return {"n": 0}
    return {
        "n": len(values),
        "median": round(statistics.median(values), 3),
        "p95": round(percentile(values, 0.95), 3),
        "p99": round(percentile(values, 0.99), 3),
        "max": round(max(values), 3),
    }


def stage_ms(meta: dict) -> dict:
    """The per-stage timings of a response's ``_meta`` in milliseconds."""
    return {k.removesuffix("_us"): round(v / 1000, 3)
            for k, v in meta.get(TIMINGS_KEY, {}).items() if isinstance(v, (int, float))}


def config_problem(config_text: str) -> str | None:
    """Why this operator configuration is not input free, or None when it is.

    These tools observe only. A configuration that could deliver live input (actuation
    enabled, an allowed application, a live engine mode) is refused before anything starts,
    even though the tools never call ``execute_action``.
    """
    try:
        config = tomllib.loads(config_text)
    except tomllib.TOMLDecodeError:
        return "the configuration is not valid TOML"
    safety = config.get("safety", {})
    if safety.get("actuation_enabled", False):
        return "[safety] actuation_enabled is true"
    if safety.get("allowed_applications"):
        return "[safety] allowed_applications is not empty"
    if config.get("engine", {}).get("mode", "dry_run") != "dry_run":
        return "[engine] mode is not dry_run"
    return None


def write_json(path: Path, document: dict) -> None:
    """Writes the document whole or not at all (a partial report survives a crash)."""
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    os.replace(temporary, path)


def run_read_only(argv: list[str], cwd: Path = REPO, timeout: float = 60) -> str | None:
    """Output of a read-only command, or None when it is missing or fails."""
    try:
        done = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, timeout=timeout,
                              check=False)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return done.stdout.strip() if done.returncode == 0 else None


def source_state() -> dict:
    """Commit and whether the tracked files are clean, for the report."""
    return {
        "commit": run_read_only(["git", "rev-parse", "HEAD"]),
        "tree_clean": run_read_only(
            ["git", "status", "--porcelain", "--untracked-files=no"]) == "",
    }


def count_threads(pid: int, api) -> int | None:
    """Threads of ``pid`` in a system thread snapshot, or None when the snapshot cannot be
    read. ``api`` is the snapshot surface: ``snapshot()`` returns a handle or None;
    ``first(handle)`` and ``next(handle)`` return the owning process id of the next entry, or
    None at the end, and raise OSError when the walk fails; ``close(handle)`` runs once a
    handle was obtained, on every path. The Windows calls are ``Win32Threads``; the tests pass
    a fake."""
    handle = api.snapshot()
    if handle is None:
        return None
    try:
        count = 0
        owner = api.first(handle)
        while owner is not None:
            if owner == pid:
                count += 1
            owner = api.next(handle)
        return count
    except OSError:
        return None
    finally:
        api.close(handle)


# ---- Windows, read only -------------------------------------------------------------------

if sys.platform == "win32":
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.WinDLL("user32", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    pdh = ctypes.WinDLL("pdh", use_last_error=True)

    user32.GetForegroundWindow.restype = wintypes.HWND
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.GetProcessHandleCount.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]

    PROCESS_QUERY_LIMITED_INFORMATION = 0x1000

    class ProcessMemoryCountersEx(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("PageFaultCount", wintypes.DWORD),
            ("PeakWorkingSetSize", ctypes.c_size_t),
            ("WorkingSetSize", ctypes.c_size_t),
            ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
            ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
            ("PagefileUsage", ctypes.c_size_t),
            ("PeakPagefileUsage", ctypes.c_size_t),
            ("PrivateUsage", ctypes.c_size_t),
        ]

    psapi.GetProcessMemoryInfo.argtypes = [
        wintypes.HANDLE, ctypes.POINTER(ProcessMemoryCountersEx), wintypes.DWORD]

    TH32CS_SNAPTHREAD = 0x00000004
    ERROR_NO_MORE_FILES = 18
    INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value

    class ThreadEntry32(ctypes.Structure):
        _fields_ = [
            ("dwSize", wintypes.DWORD),
            ("cntUsage", wintypes.DWORD),
            ("th32ThreadID", wintypes.DWORD),
            ("th32OwnerProcessID", wintypes.DWORD),
            ("tpBasePri", wintypes.LONG),
            ("tpDeltaPri", wintypes.LONG),
            ("dwFlags", wintypes.DWORD),
        ]

    kernel32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    kernel32.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    kernel32.Thread32First.argtypes = [wintypes.HANDLE, ctypes.POINTER(ThreadEntry32)]
    kernel32.Thread32Next.argtypes = [wintypes.HANDLE, ctypes.POINTER(ThreadEntry32)]

    def foreground_pid() -> int:
        """The process that owns the foreground window (0 when there is none)."""
        pid = wintypes.DWORD(0)
        user32.GetWindowThreadProcessId(user32.GetForegroundWindow(), ctypes.byref(pid))
        return int(pid.value)

    def process_counters(pid: int) -> dict | None:
        """Working set, private bytes and handle count of a process, or None."""
        handle = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
        if not handle:
            return None
        try:
            counters = ProcessMemoryCountersEx()
            counters.cb = ctypes.sizeof(counters)
            handles = wintypes.DWORD(0)
            if not psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
                return None
            if not kernel32.GetProcessHandleCount(handle, ctypes.byref(handles)):
                return None
            return {"rss": int(counters.WorkingSetSize),
                    "private": int(counters.PrivateUsage),
                    "handles": int(handles.value),
                    "threads": thread_count(pid)}
        finally:
            kernel32.CloseHandle(handle)

    class Win32Threads:
        """The thread snapshot calls of kernel32, read only; the walk is ``count_threads``."""

        def __init__(self):
            self.entry = ThreadEntry32()
            self.entry.dwSize = ctypes.sizeof(self.entry)

        def snapshot(self):
            handle = kernel32.CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
            return None if not handle or handle == INVALID_HANDLE_VALUE else handle

        def first(self, handle):
            return self.owner(kernel32.Thread32First(handle, ctypes.byref(self.entry)))

        def next(self, handle):
            return self.owner(kernel32.Thread32Next(handle, ctypes.byref(self.entry)))

        def owner(self, ok):
            if ok:
                return int(self.entry.th32OwnerProcessID)
            error = ctypes.get_last_error()
            if error == ERROR_NO_MORE_FILES:
                return None
            raise OSError(error, "the thread snapshot could not be walked")

        def close(self, handle):
            kernel32.CloseHandle(handle)

    def thread_count(pid: int) -> int | None:
        """Threads of the process in a system thread snapshot, or None when it cannot be read
        (the soak records None for that sample and goes on)."""
        return count_threads(pid, Win32Threads())

    class PdhValue(ctypes.Structure):
        _fields_ = [("CStatus", wintypes.DWORD), ("value", ctypes.c_longlong)]

    class PdhItem(ctypes.Structure):
        _fields_ = [("name", wintypes.LPWSTR), ("value", PdhValue)]

    PDH_FMT_LARGE = 0x00000400
    PDH_MORE_DATA = 0x800007D2
    pdh.PdhOpenQueryW.argtypes = [wintypes.LPCWSTR, ctypes.c_size_t,
                                  ctypes.POINTER(wintypes.HANDLE)]
    pdh.PdhAddEnglishCounterW.argtypes = [wintypes.HANDLE, wintypes.LPCWSTR, ctypes.c_size_t,
                                          ctypes.POINTER(wintypes.HANDLE)]
    pdh.PdhCollectQueryData.argtypes = [wintypes.HANDLE]
    pdh.PdhGetFormattedCounterArrayW.argtypes = [
        wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD),
        ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]
    pdh.PdhCloseQuery.argtypes = [wintypes.HANDLE]

    class GpuMemory:
        """Dedicated GPU memory of one process through the performance counters
        (``\\GPU Process Memory(pid_N*)\\Dedicated Usage``, read by their English names so
        that the language of Windows does not matter). Read only; ``read()`` returns None and
        ``reason`` says why when the counter is not there (no WDDM 2 GPU, old driver)."""

        def __init__(self, pid: int):
            self.reason: str | None = None
            self.query = wintypes.HANDLE()
            self.counter = wintypes.HANDLE()
            path = f"\\GPU Process Memory(pid_{pid}*)\\Dedicated Usage"
            if pdh.PdhOpenQueryW(None, 0, ctypes.byref(self.query)) != 0:
                self.reason = "PdhOpenQuery failed"
            elif pdh.PdhAddEnglishCounterW(self.query, path, 0, ctypes.byref(self.counter)) != 0:
                self.reason = "the GPU Process Memory counter is not available"
            else:
                pdh.PdhCollectQueryData(self.query)  # the first collection only primes it

        def read(self) -> int | None:
            if self.reason is not None:
                return None
            if pdh.PdhCollectQueryData(self.query) != 0:
                return None
            size, count = wintypes.DWORD(0), wintypes.DWORD(0)
            status = pdh.PdhGetFormattedCounterArrayW(
                self.counter, PDH_FMT_LARGE, ctypes.byref(size), ctypes.byref(count), None)
            if status & 0xFFFFFFFF != PDH_MORE_DATA or size.value == 0:
                return None  # no instance yet: the process has not touched the GPU
            buffer = ctypes.create_string_buffer(size.value)
            if pdh.PdhGetFormattedCounterArrayW(
                    self.counter, PDH_FMT_LARGE, ctypes.byref(size), ctypes.byref(count),
                    buffer) != 0:
                return None
            items = ctypes.cast(buffer, ctypes.POINTER(PdhItem))
            return sum(int(items[i].value.value) for i in range(count.value)
                       if items[i].value.CStatus in (0, 1))

        def close(self) -> None:
            if self.query:
                pdh.PdhCloseQuery(self.query)
                self.query = wintypes.HANDLE()

    def image_running(image: str) -> bool:
        """Whether a process of this image name exists (``tasklist``, read only)."""
        out = run_read_only(["tasklist", "/FI", f"IMAGENAME eq {image}", "/NH"]) or ""
        return image.lower() in out.lower()
else:  # pragma: no cover - the tools are Windows only; the pure parts import anywhere
    def foreground_pid() -> int:
        raise OSError("Windows only")

    def process_counters(pid: int) -> dict | None:
        raise OSError("Windows only")

    def thread_count(pid: int) -> int | None:
        raise OSError("Windows only")

    def image_running(image: str) -> bool:
        raise OSError("Windows only")

    class GpuMemory:  # noqa: D101
        def __init__(self, pid: int):
            self.reason = "Windows only"

        def read(self) -> int | None:
            return None

        def close(self) -> None:
            return None


# ---- the engine over MCP ------------------------------------------------------------------

class McpTimeout(Exception):
    """The server did not answer within the time given."""


class Engine:
    """``ommatix serve`` as a child process, through the S3 transport (``tools/s3``).

    Every call has a deadline: a server that hangs ends the run (its owner kills the
    process) instead of leaving the tool waiting forever. Only the tools a caller names are
    ever called; this class has no way to send an action.
    """

    ALLOWED = ("observe_screen", "get_engine_metrics")

    def __init__(self, argv: list[str]):
        spec = importlib.util.spec_from_file_location("s3_driver", S3_DRIVER)
        self._s3 = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self._s3)
        self._server = self._s3.Server(argv)
        self._pool = concurrent.futures.ThreadPoolExecutor(max_workers=1)
        self.pid = self._server.proc.pid

    def alive(self) -> bool:
        return self._server.proc.poll() is None

    def call(self, name: str, arguments: dict, timeout: float = 120.0
             ) -> tuple[dict, bool, dict, float]:
        if name not in self.ALLOWED:
            raise ValueError(f"{name} is not a read-only tool")
        future = self._pool.submit(self._server.call, name, arguments)
        try:
            return future.result(timeout=timeout)
        except concurrent.futures.TimeoutError as error:
            raise McpTimeout(f"{name} did not answer in {timeout:.0f} s") from error
        except self._s3.Abort as error:
            # The transport's own failures (the server closed stdout, a JSON-RPC error).
            raise RuntimeError("the engine's transport failed") from error

    def close(self) -> None:
        """Ends the process this object started, and nothing else."""
        try:
            self._server.proc.stdin.close()
        except OSError:
            pass
        try:
            self._server.proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self._server.proc.kill()
            self._server.proc.wait()
        self._pool.shutdown(wait=False, cancel_futures=True)
