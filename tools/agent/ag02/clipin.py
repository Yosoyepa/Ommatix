#!/usr/bin/env python3
"""The pinned agent CLI (decisions of 2026-10-20; ADR-0036, ADR-0037 and ADR-0038 notes).

The experiments (AG-02, AG-02R, AG-03) do not run on the operator's own Claude Code, which updates itself and whose
behaviour follows its configuration (on 2026-10-20 the same 2.1.304 binary began to add a ``DeferredToolPlaceholder`` entry
to every request's tool list, which the "exactly three tools" verdicts refuse). They run on ONE binary: Claude Code 2.1.304
for win32-x64, SHA-256 below, kept in a private directory outside the repository (``%LOCALAPPDATA%\\Ommatix\\agent-cli\\2.1.304``;
the file is byte-identical to the ``claude.exe`` of the npm package ``@anthropic-ai/claude-code-win32-x64@2.1.304``) and named
by an absolute ``--claude`` path. Before every launch the harness checks that the path is absolute and outside the
repository, that the file's SHA-256 is the pin and that ``<exe> --version`` says the pinned version; ``claude`` found on
``PATH`` is refused. A proof records the hash it was made with and is valid for at most 24 hours (the CLI can change its
behaviour without changing its version).

Nothing here installs, updates, writes or deletes anything: it reads and hashes the file and runs ``--version`` with a minimal
environment (the CLI's automatic update off).
"""

from __future__ import annotations

import calendar
import hashlib
import os
import re
import subprocess
import time
from pathlib import Path

CLAUDE_CODE_PIN = {"version": "2.1.304",
                   "sha256": "019e5174d5d954585edb1a946fe280e44a973050b4897034adf0e16306f85fdd"}
PROOF_MAX_AGE_S = 24 * 3600
FUTURE_TOLERANCE_S = 300
VERSION_TIMEOUT_S = 30
REPO_ROOT = Path(__file__).resolve().parents[3]
_VERSION_ENV = ("PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "COMSPEC", "TEMP", "TMP", "TMPDIR", "USERPROFILE", "HOME",
                "LOCALAPPDATA", "APPDATA")


def sha256_file(path) -> str:
    """SHA-256 of a file, streamed (the binary is ~290 MB)."""
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def run_version(exe: str) -> str:
    """``<exe> --version``, with a minimal environment and the CLI's automatic update off."""
    wanted = {name.upper() for name in _VERSION_ENV}
    env = {k: v for k, v in os.environ.items() if k.upper() in wanted} | {"DISABLE_AUTOUPDATER": "1"}
    done = subprocess.run([exe, "--version"], capture_output=True, text=True, timeout=VERSION_TIMEOUT_S, env=env,
                          check=False, creationflags=0x08000000 if os.name == "nt" else 0)
    if done.returncode != 0:
        raise OSError("the version query failed")
    return done.stdout


def executable_of(claude) -> str | None:
    """The single executable a ``--claude`` value names (a string, or a one-element list), else None."""
    if isinstance(claude, (list, tuple)):
        claude = claude[0] if len(claude) == 1 else None
    return claude if isinstance(claude, str) and claude else None


def check(claude, *, repo_root=None, hasher=sha256_file, version_runner=run_version) -> tuple[list[str], str | None]:
    """``(problems, sha256)`` for a ``--claude`` value: a single executable named by an absolute path that exists, outside the
    repository, whose SHA-256 is the pin's and whose ``--version`` is the pinned version. ``sha256`` is the file's hash
    when it was computed. Closed tokens; nothing about the file leaves in them."""
    exe = executable_of(claude)
    if exe is None:
        return ["claude_not_a_single_executable"], None
    path = Path(exe)
    if not path.is_absolute():
        return ["claude_not_an_absolute_path"], None
    if not path.is_file():
        return ["claude_missing"], None
    problems: list[str] = []
    root = Path(REPO_ROOT if repo_root is None else repo_root).resolve()
    try:
        path.resolve().relative_to(root)
        problems.append("claude_inside_repository")
    except ValueError:
        pass
    try:
        sha = hasher(path)
    except OSError:
        return ["claude_unreadable"], None
    if sha != CLAUDE_CODE_PIN["sha256"]:
        problems.append("claude_hash_differs")
    try:
        output = version_runner(str(path))
        first = output.split()[0] if isinstance(output, str) and output.split() else ""
        version = first if re.fullmatch(r"\d+\.\d+\.\d+", first) else None
    except (OSError, subprocess.SubprocessError):
        version = None
    if version is None:
        problems.append("claude_version_unreadable")
    elif version != CLAUDE_CODE_PIN["version"]:
        problems.append("claude_version_differs")
    return problems, sha


def age_problems(date_utc, now: float | None = None) -> list[str]:
    """A proof is valid for at most ``PROOF_MAX_AGE_S`` (and was not made in the future)."""
    try:
        made = calendar.timegm(time.strptime(date_utc, "%Y-%m-%dT%H:%M:%SZ"))
    except (TypeError, ValueError, OverflowError):
        return ["proof_date_unreadable"]
    age = (time.time() if now is None else now) - made
    if age < -FUTURE_TOLERANCE_S:
        return ["proof_from_the_future"]
    return ["proof_expired"] if age > PROOF_MAX_AGE_S else []


def launch_problems(claude, proof: dict, *, now: float | None = None, **kwargs) -> list[str]:
    """What stops a launch: the CLI is not the pin, it is not the binary the proof was made with, or the proof is too old."""
    problems, sha = check(claude, **kwargs)
    problems = list(problems)  # never extend a list the checker handed back
    if sha is not None and sha != proof.get("cli_sha256"):
        problems.append("cli_differs_from_proof")
    return problems + age_problems(proof.get("date_utc"), now)
