"""Test support for the pinned agent CLI (``clipin``): the real guard copies, hashes and runs a 290 MB binary, so tests with a
fake CLI replace it. Tolerant of the module's absence, so that a test written first can be run against a checkout
that does not have it yet (red-first evidence)."""

import contextlib
import functools
from unittest import mock

try:
    import clipin
except ImportError:  # no pin yet
    clipin = None

PIN_SHA = "019e5174d5d954585edb1a946fe280e44a973050b4897034adf0e16306f85fdd"


@contextlib.contextmanager
def _verified(claude, proof=None, *, problems=(), sha=PIN_SHA, log=None, **_unused):
    """What ``clipin.verified`` yields for a fake CLI: the configured command itself as the path (there is no binary to copy)."""
    if log is not None:
        log.append("enter")
    yield clipin.Verified(list(problems), sha, None if problems else claude)
    if log is not None:
        log.append("exit")


def pinned(sha=PIN_SHA, problems=(), log=None):
    """``clipin.verified`` answers ``(problems, sha)`` for a fake CLI."""
    if clipin is None:
        return contextlib.nullcontext()
    return mock.patch.object(clipin, "verified", functools.partial(_verified, problems=problems, sha=sha, log=log))


def launch_ok():
    """The guard finds nothing wrong (the stand-in proofs carry old dates and there is no real binary)."""
    return pinned()


def launch_refused(*tokens):
    return pinned(None, tokens)
