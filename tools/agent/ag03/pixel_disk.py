#!/usr/bin/env python3
"""What a pixel-agent session may leave on disk (ADR-0038 decision 5, O5: images exist only in memory).

Measured on 2026-10-20 with Claude Code 2.1.304 (real CLI, loopback stand-in, the pixel server in synthetic mode, drawn
PNGs; nothing captured, no provider): with the production command (``--no-session-persistence``) the isolated configuration
directory holds no ``projects/`` tree, no ``tool-results/`` file, no transcript and no image data; WITHOUT that option the CLI
writes every MCP image result as a PNG under ``<config>/projects/<cwd>/<session>/tool-results/`` and every transcript with the
image in base64 (the documentation lists a 30-day clean-up and says nothing on the option). The CLI also keeps, OUTSIDE the
configuration directory, a debug log of each MCP server per working directory
(``<cache>/claude-cli-nodejs/Cache/<cwd>/mcp-logs-<server>/<date>.jsonl``); on synthetic sessions it holds no image data. The
harness does not delete it, and what it would hold on a real screen is **not measured**.

This module is the pure part of the check the isolation preflight makes after its synthetic sessions: snapshots of a
directory before and after a session, the classification of what the session touched, and a scan for PNG signatures (raw,
JSON-escaped and base64 in the three alignments). It reads, never writes or deletes, and never prints a byte of a file.
"""

from __future__ import annotations

import base64
import hashlib
import os
import sys
from pathlib import Path

MAX_SCAN_BYTES = 256 * 1024 * 1024  # a touched file larger than this is "unreadable": the check refuses

_SIGNATURE = b"\x89PNG\r\n\x1a\n"
# The three alignments of the signature in base64 (the standalone one is the well-known "iVBORw0KGgo"), the raw bytes
# and their JSON-escaped form. ``IHDR`` alone is not a hit: it is an ordinary word.
NEEDLES = {
    "png_signature": _SIGNATURE,
    "png_signature_json_escaped": b"\\u0089PNG",
    "base64_png": b"iVBORw0KGgo",
    "base64_png_shifted_1": base64.b64encode(b"x" + _SIGNATURE)[2:12],
    "base64_png_shifted_2": base64.b64encode(b"xx" + _SIGNATURE)[3:12],
}


def image_hits(data: bytes) -> list[str]:
    """The names of the PNG patterns found in ``data`` (empty: none)."""
    return sorted(name for name, needle in NEEDLES.items() if needle in data)


class Snapshot(dict):
    """``{path: (size, mtime_ns)}`` of the files under a root, plus whether the listing is COMPLETE. A directory that could
    not be listed, a file whose metadata could not be read or a required root that is missing makes it incomplete: an
    incomplete listing is a refusal (``*_incomplete``), never an empty one (review of PR 106, R106-2)."""

    def __init__(self, files=(), incomplete: bool = False):
        super().__init__(files)
        self.incomplete = incomplete


def snapshot(root: Path, *, optional: bool = False) -> Snapshot:
    """The snapshot of ``root``. A root that does not exist is a complete empty snapshot only when ``optional`` (the CLI's
    cache may not exist before the first run); otherwise, like any enumeration or metadata error, it is incomplete."""
    found = Snapshot()
    if not os.path.exists(root):
        found.incomplete = not optional
        return found

    def listing_failed(_error):
        found.incomplete = True
    for directory, _dirs, files in os.walk(root, onerror=listing_failed):
        for name in files:
            path = Path(directory) / name
            try:
                stat = path.stat()
            except OSError:
                found.incomplete = True
                continue
            found[str(path)] = (stat.st_size, stat.st_mtime_ns)
    return found


def touched(before: dict, after: dict) -> list[str]:
    """The files that are new or changed in ``after`` (a file that only disappeared is not "touched")."""
    return sorted(path for path, meta in after.items() if before.get(path) != meta)


def _scan(path: str) -> list[str] | None:
    """The pattern names in one file, or None when it cannot be read (a refusal, never a pass)."""
    try:
        if Path(path).stat().st_size > MAX_SCAN_BYTES:
            return None
        return image_hits(Path(path).read_bytes())
    except OSError:
        return None


def _incomplete(before, after) -> bool:
    return bool(getattr(before, "incomplete", False) or getattr(after, "incomplete", False))


def _tree_problems(label: str, before: dict, after: dict) -> tuple[set[str], list[str]]:
    """The tokens common to every monitored location: ``disk_<label>_incomplete`` (a listing that could not be completed),
    ``_unreadable`` (a touched file that could not be read) and ``_image_data``; and the touched paths."""
    problems: set[str] = set()
    if _incomplete(before, after):
        problems.add(f"disk_{label}_incomplete")
    paths = touched(before, after)
    for path in paths:
        hits = _scan(path)
        if hits is None:
            problems.add(f"disk_{label}_unreadable")
        elif hits:
            problems.add(f"disk_{label}_image_data")
    return problems, paths


def config_problems(root: Path, before: dict, after: dict) -> tuple[list[str], dict]:
    """Closed tokens for what a session wrote in its configuration directory ``root`` (``before``/``after`` are its
    snapshots): anything under ``projects/`` (a persisted session), a ``tool-results`` file, a ``.jsonl`` transcript, image
    data in any touched file, a touched file that cannot be read, an incomplete listing. ``.claude.json`` and its backups
    may change; they must only hold no image. Returns ``(tokens, {"touched": n})``."""
    problems, paths = _tree_problems("config", before, after)
    for path in paths:
        try:
            parts = [part.lower() for part in Path(path).relative_to(root).parts]
        except ValueError:
            parts = [part.lower() for part in Path(path).parts]
        if "projects" in parts:
            problems.add("disk_config_projects_written")
        if "tool-results" in parts:
            problems.add("disk_config_tool_results")
        if parts and parts[-1].endswith(".jsonl"):
            problems.add("disk_config_transcript")
    return sorted(problems), {"touched": len(paths)}


def cache_problems(root: Path, before: dict, after: dict) -> tuple[list[str], dict]:
    """The same for the CLI's cache (the MCP debug logs): the files touched during the session must hold no image data."""
    problems, paths = _tree_problems("cache", before, after)
    return sorted(problems), {"touched": len(paths)}


def temp_problems(root: Path, before: dict, after: dict) -> tuple[list[str], dict]:
    """The same for the child's own temp directory (``TEMP``/``TMP``/``TMPDIR``/``CLAUDE_CODE_TMPDIR``, where the CLI keeps
    its image and scratch files): no image data may appear in it (review of PR 106, R106-1)."""
    problems, paths = _tree_problems("temp", before, after)
    return sorted(problems), {"touched": len(paths)}


# The variables that decide where the CLI puts its temporary and image files; one directory of the session's own.
TEMP_ENV_NAMES = ("TEMP", "TMP", "TMPDIR", "CLAUDE_CODE_TMPDIR")


def child_temp_env(directory) -> dict:
    """The child environment entries that give the session a temp directory of its own."""
    return {name: str(directory) for name in TEMP_ENV_NAMES}


def cli_cache_root(platform: str | None = None, environ: dict | None = None) -> Path:
    """Where Claude Code keeps its cache (the ``env-paths`` convention of the CLI, suffix ``nodejs``): observed on Windows
    (``%LOCALAPPDATA%\\claude-cli-nodejs\\Cache``); the macOS and Linux locations follow the same convention and are NOT
    measured. The directory may not exist."""
    platform = sys.platform if platform is None else platform
    env = os.environ if environ is None else environ
    home = Path(env.get("HOME") or env.get("USERPROFILE") or "~").expanduser()
    if platform == "win32":
        return Path(env.get("LOCALAPPDATA") or home / "AppData" / "Local") / "claude-cli-nodejs" / "Cache"
    if platform == "darwin":
        return home / "Library" / "Caches" / "claude-cli-nodejs"
    return Path(env.get("XDG_CACHE_HOME") or home / ".cache") / "claude-cli-nodejs"


def cache_root_fingerprint(environ: dict | None = None, platform: str | None = None) -> str:
    """SHA-256 of the cache root a child with this environment would use (no path leaves the machine in a proof): the
    preflight records it and the runner compares it with the production child's before every launch."""
    return hashlib.sha256(str(cli_cache_root(platform, environ)).encode("utf-8")).hexdigest()
