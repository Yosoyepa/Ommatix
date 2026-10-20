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

Nothing is executed before the scope and hash checks pass, and what is verified is what runs (review of PR 107): ``verified``
makes a PRIVATE COPY of the pinned binary in a directory it creates, holds it against writes where the platform allows
(Windows sharing mode), hashes THE COPY, asks the version of THE COPY and yields the copy's path, which the caller launches;
replacing the configured file at any moment changes nothing that runs. Every launch is verified on its own, and the copy is
removed afterwards. It installs, updates and deletes nothing of the operator's; ``--version`` runs with a minimal
environment (the CLI's automatic update off).
"""

from __future__ import annotations

import calendar
import contextlib
import hashlib
import os
import re
import shutil
import stat
import subprocess
import tempfile
import time
from dataclasses import dataclass, field
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


@dataclass
class Verified:
    """What ``verified`` yields: ``problems`` (closed tokens; empty means the launch may go on), the SHA-256 of the binary and
    ``path``, the executable to launch (the verified private copy; None when there are problems)."""
    problems: list = field(default_factory=list)
    sha: str | None = None
    path: object = None

    def kwargs(self) -> dict:
        """The ``claude=`` argument for the command builders: the verified copy, or nothing for an agent that is not Claude Code."""
        return {} if self.path is None else {"claude": self.path}


def _hold_against_writes(path):
    """On Windows, opens ``path`` with a sharing mode that denies others write, delete and rename (the file still runs and can
    still be read); returns the function that releases it. Elsewhere nothing is held (the copy is in a 0700 directory and
    read-only). Raises OSError when the hold cannot be taken on Windows."""
    if os.name != "nt":
        return lambda: None
    import _winapi
    handle = _winapi.CreateFile(str(path), 0x80000000, 0x1, 0, 3, 0, 0)  # GENERIC_READ, FILE_SHARE_READ, OPEN_EXISTING
    return lambda: _winapi.CloseHandle(handle)


@contextlib.contextmanager
def verified(claude, proof: dict | None = None, *, repo_root=None, now: float | None = None, hasher=None,
             version_runner=None, copier=None):
    """Verifies ``claude`` and yields a ``Verified`` whose ``path`` is the private copy to launch; the copy lives while the
    ``with`` block does. Order (nothing is executed until the copy has been hashed): a single executable named by an absolute
    path that exists and is outside the repository; its SHA-256 is the pin's; the proof (if any) names that hash and is at most 24 h
    old; a private copy is made, held against writes and hashed again; ``--version`` of the COPY is the pinned version."""
    hasher = hasher or sha256_file
    version_runner = version_runner or run_version
    copier = copier or shutil.copyfile
    exe = executable_of(claude)
    if exe is None:
        yield Verified(["claude_not_a_single_executable"])
        return
    path = Path(exe)
    if not path.is_absolute():
        yield Verified(["claude_not_an_absolute_path"])
        return
    if not path.is_file():
        yield Verified(["claude_missing"])
        return
    root = Path(REPO_ROOT if repo_root is None else repo_root).resolve()
    try:
        path.resolve().relative_to(root)
        inside = True
    except ValueError:
        inside = False
    if inside:
        yield Verified(["claude_inside_repository"])
        return
    try:
        sha = hasher(path)
    except OSError:
        yield Verified(["claude_unreadable"])
        return
    if sha != CLAUDE_CODE_PIN["sha256"]:
        yield Verified(["claude_hash_differs"], sha)
        return
    if proof is not None:
        problems = (["cli_differs_from_proof"] if sha != proof.get("cli_sha256") else []) + age_problems(
            proof.get("date_utc"), now)
        if problems:
            yield Verified(problems, sha)
            return
    private = Path(tempfile.mkdtemp(prefix="ommatix-cli-"))
    release = None
    copy = private / path.name
    try:
        try:
            copier(path, copy)
            os.chmod(copy, stat.S_IREAD | stat.S_IEXEC)
            release = _hold_against_writes(copy)
            copy_sha = hasher(copy)
        except OSError:
            yield Verified(["claude_copy_failed"], sha)
            return
        if copy_sha != CLAUDE_CODE_PIN["sha256"]:
            yield Verified(["claude_copy_differs"], sha)
            return
        try:
            output = version_runner(str(copy))
            first = output.split()[0] if isinstance(output, str) and output.split() else ""
            version = first if re.fullmatch(r"\d+\.\d+\.\d+", first) else None
        except (OSError, subprocess.SubprocessError):
            version = None
        if version is None:
            yield Verified(["claude_version_unreadable"], sha)
        elif version != CLAUDE_CODE_PIN["version"]:
            yield Verified(["claude_version_differs"], sha)
        else:
            yield Verified([], sha, str(copy))
    finally:
        if release is not None:
            with contextlib.suppress(OSError):
                release()
        with contextlib.suppress(OSError):
            os.chmod(copy, stat.S_IWRITE | stat.S_IREAD)
        shutil.rmtree(private, ignore_errors=True)


def check(claude, **kwargs) -> tuple[list[str], str | None]:
    """``(problems, sha256)`` for a ``--claude`` value (a verdict only: the private copy is made and removed again)."""
    with verified(claude, **kwargs) as v:
        return list(v.problems), v.sha


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
    """A verdict for a launch with this proof (see ``verified``); nothing stays running."""
    with verified(claude, proof, now=now, **kwargs) as v:
        return list(v.problems)
