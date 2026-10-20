"""Test support for the pinned agent CLI (``clipin``): the real check hashes and runs a 290 MB binary, so tests with a
fake CLI replace it. Tolerant of the module's absence, so that a test written first can be run against a checkout
that does not have it yet (red-first evidence)."""

import contextlib
from unittest import mock

try:
    import clipin
except ImportError:  # no pin yet
    clipin = None

PIN_SHA = "019e5174d5d954585edb1a946fe280e44a973050b4897034adf0e16306f85fdd"


def pinned(sha=PIN_SHA, problems=()):
    """``clipin.check`` answers ``(problems, sha)`` for a fake CLI."""
    return mock.patch.object(clipin, "check", return_value=(list(problems), sha)) if clipin else contextlib.nullcontext()


def launch_ok():
    """``clipin.launch_problems`` finds nothing wrong (the stand-in proofs carry old dates and no real binary)."""
    return mock.patch.object(clipin, "launch_problems", return_value=[]) if clipin else contextlib.nullcontext()


def launch_refused(*tokens):
    return (mock.patch.object(clipin, "launch_problems", return_value=list(tokens)) if clipin
            else contextlib.nullcontext())
