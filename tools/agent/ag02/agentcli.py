#!/usr/bin/env python3
"""The agent a goal is run with (ADR-0037): one small interface, four implementations.

``run_goal.py`` runs a goal through an ``AgentCLI``: it asks it for the command, the child's
environment, the isolation proof it needs and the facts for provenance, and reads the stream the
child prints (the ``init`` / ``assistant`` / ``user`` / ``result`` events of ``StreamMetrics``).
Everything else - the budget proxy, the oracle, the scoring, the safety rules - is the same for every
agent.

* ``claude`` - Claude Code (ADR-0036), behaviour unchanged.
* ``minimal`` - ``minimal_agent.py`` (ADR-0037): a tool-calling loop of our own for an
  OpenAI-compatible Chat Completions endpoint (DeepSeek, MiMo); the confinable path.
* ``codex`` and ``opencode`` - implemented as *probes*: their configuration builders and a verdict
  on the real request they produce, and nothing that runs a goal. Neither CLI can be confined to
  the three Ommatix tools today (``preflight_cli.py`` shows why, per version), so both fail closed:
  a run needs a *passing* proof, and even then the stream of these CLIs is not read here.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import run_goal as rg  # noqa: E402

AGENT_CLIS = ("claude", "minimal", "codex", "opencode")
MINIMAL_AGENT = HERE / "minimal_agent.py"
MINIMAL_PROOF_SCHEMA = "ag02-isolation-proof-minimal-1"
# Raised whenever the minimal agent's verifier is made stricter; a proof from an older revision is refused.
MINIMAL_VERIFIER_REVISION = 1
CLI_PROOF_SCHEMA = "ag02-cli-probe-1"
# The names a result may carry (closed vocabularies; anything else is recorded as "unknown").
CLI_NAMES = ("claude-code", "minimal-agent", "codex", "opencode")
PROVIDER_NAMES = ("anthropic", "deepseek", "mimo")

# The providers the minimal agent may talk to. ``base_url`` is where the observations go (ADR-0037);
# ``caveat`` is the provider's data-use statement as far as it was checked, recorded with the results.
# Each was read from the provider's own documentation on 2026-10-18 except where "unverified" says so.
PROVIDERS = {
    "deepseek": {
        "base_url": "https://api.deepseek.com", "auth_style": "bearer", "default_model": "deepseek-flash",
        "effort": {"reasoning_effort": "max", "thinking": {"type": "enabled"}},
        "key_file_hint": "deepseek.key",
        "caveat": ("DeepSeek's API documentation has no data-use statement; the terms were not read by "
                   "the author of this change (unverified). Inputs are synthetic test-application labels."),
        "docs": "https://api-docs.deepseek.com/",
    },
    "mimo": {
        "base_url": "https://api.xiaomimimo.com/v1", "auth_style": "api-key",
        "default_model": "mimo-v2.6-flash", "effort": None, "key_file_hint": "mimo.key",
        "caveat": ("Xiaomi's quick-start has no data-use statement; a free tier may retain or train on "
                   "inputs (unverified). The model id and the effort parameter are not in the pages "
                   "read: the owner supplies them (--model, --extra-param)."),
        "docs": "https://mimo.mi.com/static/docs/quick-start/summary/first-api-call.md",
    },
}
# Authorised by the owner but not runnable: GPT-6 Luna is reachable only through the Codex
# subscription (no API key), and Codex cannot be confined (see ``codex`` below).
AUTHORISED_NOT_RUNNABLE = {"openai-gpt-6-luna": "only through Codex, which cannot be confined to the three tools"}


class AgentCLI:
    name = ""
    provider = ""
    # Whether the child needs the controlled Claude configuration directory of ADR-0036.
    uses_claude_config_dir = False
    # Whether the goal runner's Anthropic routing-variable refusal applies to this agent.
    checks_anthropic_routing = False

    def __init__(self, args=None):
        self.args = args

    def refusal(self) -> str | None:
        """Why this agent cannot run a goal at all (None when it can)."""
        return None

    def build_argv(self, *, model: str, mcp_config: Path, system_instruction: str, max_budget_usd: float) -> list[str]:
        raise NotImplementedError

    def env(self, extra: dict | None = None) -> dict:
        raise NotImplementedError

    def load_proof(self, path: Path) -> dict:
        raise NotImplementedError

    def proof_problems(self, proof: dict, args, config_dir, system_instruction: str = "") -> list[str]:
        raise NotImplementedError

    def provenance(self) -> dict:
        return {"cli": self.name, "provider": self.provider}


class ClaudeCLI(AgentCLI):
    name, provider = "claude-code", "anthropic"
    uses_claude_config_dir = True
    checks_anthropic_routing = True

    def build_argv(self, *, model, mcp_config, system_instruction, max_budget_usd):
        return rg.build_claude_argv(claude=self.args.claude, model=model, mcp_config=mcp_config,
                                    system_instruction=system_instruction, max_budget_usd=max_budget_usd)

    def env(self, extra=None):
        return rg.agent_env(extra)

    def load_proof(self, path):
        return rg.load_isolation_proof(path)

    def proof_problems(self, proof, args, config_dir, system_instruction=""):
        return rg.proof_problems(proof, args, config_dir)


# ---- the minimal agent ---------------------------------------------------------------------

def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def canonical_sha(value) -> str:
    return sha256_text(json.dumps(value, sort_keys=True, separators=(",", ":")))


def minimal_extra_param(provider: str, effort: str | None, extra_param: dict | None) -> dict:
    """The request parameters beyond the fixed ones: the provider's own spelling of ``--effort max``
    (only where its documentation was read) merged with the operator's ``--extra-param``."""
    out = {}
    if effort:
        spelling = PROVIDERS[provider]["effort"]
        if spelling is None:
            raise SystemExit(f"--effort: the {provider} parameter is not documented in the pages read; "
                             "pass the exact parameter with --extra-param")
        if effort != "max":
            raise SystemExit("--effort: only 'max' is pre-registered by the owner")
        out |= spelling
    return out | (extra_param or {})


class MinimalCLI(AgentCLI):
    name = "minimal-agent"

    def __init__(self, args=None):
        super().__init__(args)
        self.provider = getattr(args, "provider", None) or ""

    def refusal(self):
        if self.provider not in PROVIDERS:
            return "--provider must be one of: " + ", ".join(sorted(PROVIDERS))
        return None

    def base_url(self) -> str:
        override = getattr(self.args, "base_url", None)
        return override or PROVIDERS[self.provider]["base_url"]

    def request_extra(self) -> dict:
        return minimal_extra_param(self.provider, getattr(self.args, "effort", None),
                                   getattr(self.args, "extra_param", None))

    def build_argv(self, *, model, mcp_config, system_instruction, max_budget_usd):
        a = self.args
        argv = [sys.executable, "-I", str(MINIMAL_AGENT), "--base-url", self.base_url(), "--model", model,
                "--provider", self.provider, "--key-file", str(a.key_file or "<key-file>"),
                "--auth-style", PROVIDERS[self.provider]["auth_style"], "--mcp-config", str(mcp_config),
                "--system-prompt", system_instruction, "--extra-param", json.dumps(self.request_extra()),
                "--max-total-tokens", str(getattr(a, "max_total_tokens", 400_000))]
        if getattr(a, "price", None):
            argv += ["--price", a.price]
        return argv

    def env(self, extra=None):
        """Nothing of the operator's environment but the operating system's basics: no key, no
        provider selector, no proxy (the agent also ignores proxy variables), no Python switch."""
        wanted = {b.upper() for b in rg.BASIC_ENV}
        env = {k: v for k, v in os.environ.items() if k.upper() in wanted}
        return env | {"PYTHONUTF8": "1", "PYTHONNOUSERSITE": "1", **(extra or {})}

    def expected_proof_facts(self, model: str, system_instruction: str) -> dict:
        return {"agent_sha256": hashlib.sha256(MINIMAL_AGENT.read_bytes()).hexdigest(),
                "provider": self.provider, "base_url": self.base_url(), "requested_model": model,
                "auth_style": PROVIDERS[self.provider]["auth_style"],
                "extra_param_sha256": canonical_sha(self.request_extra()),
                "system_instruction_sha256": sha256_text(system_instruction)}

    def load_proof(self, path):
        try:
            proof = json.loads(Path(path).read_text(encoding="utf-8"))
        except (OSError, ValueError):
            raise SystemExit("the isolation proof is unreadable") from None
        revision = proof.get("verifier_revision") if isinstance(proof, dict) else None
        ok = (isinstance(proof, dict) and proof.get("schema") == MINIMAL_PROOF_SCHEMA
              and proof.get("passed") is True and type(revision) is int and revision == MINIMAL_VERIFIER_REVISION
              and sorted(proof.get("tools", [])) == sorted(rg.ALLOWED_TOOL_NAMES)
              and isinstance(proof.get("claude_code_version"), str) and rg.VERSION.match(proof["claude_code_version"])
              and isinstance(proof.get("model"), str) and isinstance(proof.get("init_keys_sha256"), str)
              and all(proof.get(k) == [] for k in ("plugins", "agents", "plugin_hashes", "agent_hashes"))
              and isinstance(proof.get("control_detected"), list) and proof["control_detected"]
              and isinstance(proof.get("date_utc"), str))
        if not ok:
            raise SystemExit(f"the isolation proof is not a passing {MINIMAL_PROOF_SCHEMA} file with verifier "
                             f"revision {MINIMAL_VERIFIER_REVISION}; run preflight_cli.py --agent-cli minimal again")
        return proof

    def proof_problems(self, proof, args, config_dir, system_instruction=""):
        facts = self.expected_proof_facts(args.model, system_instruction)
        return [f"{key}_differs_from_proof" for key, value in facts.items() if proof.get(key) != value]

    def provenance(self):
        return {"cli": self.name, "provider": self.provider,
                "agent_sha256": hashlib.sha256(MINIMAL_AGENT.read_bytes()).hexdigest(),
                "base_url_host": re.sub(r"^https?://([^/:]+).*$", r"\1", self.base_url()),
                "extra_param_keys": sorted(self.request_extra())}


# ---- Codex and OpenCode: probes that fail closed ----------------------------------------------

def surface_problems(extras: list[str], skills: bool) -> list[str]:
    problems = []
    if extras:
        problems.append("tool_surface_wider_than_three")
    if skills:
        problems.append("skills_from_real_home")
    return problems


class RefusedCLI(AgentCLI):
    """A CLI that cannot run a goal. ``load_proof`` shows the verdict of its probe; ``build_argv``
    never returns a command."""

    def refusal(self):
        return (f"{self.name} cannot be confined to the three Ommatix tools (ADR-0037); run "
                f"preflight_cli.py --agent-cli {self.name} to see the verdict for the installed version")

    def build_argv(self, **_):
        raise SystemExit(self.refusal())

    def load_proof(self, path):
        try:
            proof = json.loads(Path(path).read_text(encoding="utf-8"))
        except (OSError, ValueError):
            raise SystemExit(self.refusal()) from None
        if not (isinstance(proof, dict) and proof.get("schema") == CLI_PROOF_SCHEMA and proof.get("passed") is True):
            tokens = proof.get("problems") if isinstance(proof, dict) else None
            raise SystemExit(self.refusal() + (": " + ",".join(str(t) for t in tokens)
                                              if isinstance(tokens, list) else ""))
        raise SystemExit(f"{self.name}: a passing probe exists but this runner reads no {self.name} stream; "
                         "implementing it needs a new ADR revision")

    def proof_problems(self, proof, args, config_dir, system_instruction=""):
        return [self.refusal()]


class CodexCLI(RefusedCLI):
    name, provider = "codex", "openai"


class OpenCodeCLI(RefusedCLI):
    name, provider = "opencode", "opencode"


def make(args) -> AgentCLI:
    kind = getattr(args, "agent_cli", "claude") or "claude"
    return {"claude": ClaudeCLI, "minimal": MinimalCLI, "codex": CodexCLI, "opencode": OpenCodeCLI}[kind](args)
