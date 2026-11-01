"""Test support: run a test module without the developer's provider-routing variables.

The AG-02 and AG-03 tools refuse a real session while a variable that selects another provider or
endpoint is set (``run_goal.routing_problems``, ADR-0036). A Claude Code session sets
``ANTHROPIC_BASE_URL`` in the environment of the processes it starts, so a test that drives a tool's
``main`` from such a shell was refused by that variable instead of exercising what it tests.

A test module that drives a ``main`` calls ``start()`` from ``setUpModule`` and ``stop()`` from
``tearDownModule``: for the module's duration the routing variables are absent, and afterwards the
environment is exactly what it was. A test of the refusal sets the variables itself, inside it.
"""

import os
from unittest import mock

import run_goal as rg

_PATCHES = []


def start() -> None:
    """Removes every routing variable from ``os.environ`` until the matching ``stop()``."""
    patch = mock.patch.dict(os.environ)
    patch.start()
    _PATCHES.append(patch)
    for name in rg.routing_problems(os.environ):
        del os.environ[name]


def stop() -> None:
    """Restores the environment that the matching ``start()`` saw."""
    _PATCHES.pop().stop()
