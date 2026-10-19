#!/usr/bin/env python3
"""The agent a goal is run with (ADR-0037): one small interface, four implementations.

``run_goal.py`` runs a goal through an ``AgentCLI``: it asks it for the command, the child's
environment, the isolation proof it needs and the facts for provenance, and reads the stream the
child prints (the ``init`` / ``assistant`` / ``user`` / ``result`` events of ``StreamMetrics``).
Everything else - the budget proxy, the oracle, the scoring, the safety rules - is the same for every
agent.

* ``claude`` - Claude Code (ADR-0036), behaviour unchanged; with ``--route mimo|deepseek`` the same
  confined harness is pointed at that provider's Anthropic-compatible endpoint (ADR-0037): the base URL
  and the key go into the child's environment only, built by ``route_env`` - one function for the
  preflight's stand-in and for production.
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
# The revision of the route preflight's request-level rules (a Claude Code route proof carries it).
#   2: re-review of PR 93 (N3) - only the reviewed request header names, whatever their values
ROUTE_VERIFIER_REVISION = 2
ROUTE_CONTEXT_TOKENS_DEFAULT = rg.ROUTE_CONTEXT_TOKENS_DEFAULT
# A key with one of these prefixes is a Xiaomi MiMo Token Plan key, whatever provider label or route it
# is offered to: coding tools only, and only with the Token Plan hosts of the MiMo route.
TOKEN_PLAN_PREFIXES = ("tp-", "ttp-")
MINIMAL_AGENT = HERE / "minimal_agent.py"
MINIMAL_PROOF_SCHEMA = "ag02-isolation-proof-minimal-1"
# Raised whenever the minimal agent's verifier is made stricter; a proof from an older revision is refused.
#   2: PR-93 review - decoded-request matching, parameter values, budgets, call validation, bounds, key path
MINIMAL_VERIFIER_REVISION = 2
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
# The routes of the confined Claude Code harness (owner decision of 2026-10-18): Claude Code, with every
# ADR-0036 control, pointed at a provider's Anthropic-compatible endpoint. ``base_urls`` is the closed
# list of endpoints it may be pointed at; the operator must name one explicitly. A Token Plan key
# (``TOKEN_PLAN_PREFIXES``) belongs to the MiMo route and its regional ``token_plan_host_prefix`` hosts only,
# whatever route or provider label it is offered to (its terms allow coding tools only).
CLAUDE_ROUTES = {
    "mimo": {
        "model": "mimo-v2.6-flash", "effort": "max",
        "base_urls": ("https://token-plan-sgp.xiaomimimo.com/anthropic",
                      "https://token-plan-cn.xiaomimimo.com/anthropic",
                      "https://token-plan-ams.xiaomimimo.com/anthropic"),
        "token_plan_host_prefix": "token-plan-",
        "key_file_hint": "mimo.key",
        "caveat": ("A Token Plan key's terms allow coding tools such as Claude Code only, which is why MiMo "
                   "runs through the Claude Code harness and not through the minimal agent. Xiaomi's pages "
                   "read have no data-use statement (unverified)."),
        "docs": "https://mimo.mi.com/docs/en-US/tokenplan/integration/claudecode",
    },
    "deepseek": {
        "model": "deepseek-flash", "effort": "max",
        "base_urls": ("https://api.deepseek.com/anthropic",),
        "token_plan_host_prefix": None,
        "key_file_hint": "deepseek.key",
        "caveat": ("DeepSeek's API documentation has no data-use statement; the terms were not read "
                   "(unverified). Inputs are synthetic test-application labels."),
        "docs": "https://api-docs.deepseek.com/quick_start/agent_integrations/claude_code",
    },
}
# The only environment variables a route child gets beyond the operating system's basics.
ROUTE_VARS = ("ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY", "ANTHROPIC_MODEL",
              "ANTHROPIC_SMALL_FAST_MODEL", "ANTHROPIC_DEFAULT_SONNET_MODEL", "ANTHROPIC_DEFAULT_OPUS_MODEL",
              "ANTHROPIC_DEFAULT_HAIKU_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL", "CLAUDE_CODE_EFFORT_LEVEL",
              "CLAUDE_CONFIG_DIR", "CLAUDE_CODE_DISABLE_CLAUDE_MDS")
# Authorised by the owner but not runnable: GPT-6 Luna is reachable only through the Codex
# subscription (no API key), and Codex cannot be confined (see ``codex`` below).
AUTHORISED_NOT_RUNNABLE = {"openai-gpt-6-luna": "only through Codex, which cannot be confined to the three tools"}


def route_key_problem(route: str, base_url: str, key: str) -> str | None:
    """Why `key` may not be used at `base_url` on this route: a Token Plan key (``tp-``) only with its
    regional Token Plan host, never with the pay-as-you-go URL; a Token Plan host only with such a key."""
    spec = CLAUDE_ROUTES[route]
    host = re.sub(r"^https?://([^/:]+).*$", r"\1", base_url)
    plan_key = key.startswith(TOKEN_PLAN_PREFIXES)  # judged before the provider is looked at
    plan_host = bool(spec["token_plan_host_prefix"]) and host.startswith(spec["token_plan_host_prefix"])
    if plan_key and route != "mimo":
        return (f"a Token Plan key (tp-/ttp-) belongs to the MiMo Token Plan route only, never to the {route} "
                "route: a wrong key file would send it to another provider")
    if plan_key and not plan_host:
        return "a Token Plan key (tp-) is refused with the pay-as-you-go URL: name the regional Token Plan URL"
    if plan_host and not plan_key:
        return "a Token Plan URL needs a Token Plan key (tp-/ttp-)"
    return None


def route_env(route: str, base_url: str, token: str, model: str, config_dir) -> dict:
    """The child environment of the confined Claude Code harness on a route: the operating system's
    basics and nothing of the operator's own, plus exactly the ``ROUTE_VARS``. The key appears here and
    nowhere else (never in an argument, a log or a result). ``ANTHROPIC_API_KEY`` carries the same
    value: with only ``ANTHROPIC_AUTH_TOKEN`` set, Claude Code 2.1.303 drops ``execute_action`` from
    its tool list (observed on the stand-in), while with both it lists the three tools and sends the same
    secret in ``Authorization: Bearer`` and in ``x-api-key`` - to the one endpoint the route names. The
    preflight proves the headers, the tools and the models on the stand-in with this very function."""
    wanted = {b.upper() for b in rg.BASIC_ENV}
    env = {k: v for k, v in os.environ.items() if k.upper() in wanted}
    env |= rg.QUIET_ENV
    env |= {"ANTHROPIC_BASE_URL": base_url, "ANTHROPIC_AUTH_TOKEN": token, "ANTHROPIC_API_KEY": token,
            "ANTHROPIC_MODEL": model, "ANTHROPIC_SMALL_FAST_MODEL": model,
            "ANTHROPIC_DEFAULT_SONNET_MODEL": model, "ANTHROPIC_DEFAULT_OPUS_MODEL": model,
            "ANTHROPIC_DEFAULT_HAIKU_MODEL": model, "CLAUDE_CODE_SUBAGENT_MODEL": model,
            "CLAUDE_CODE_EFFORT_LEVEL": CLAUDE_ROUTES[route]["effort"], "CLAUDE_CONFIG_DIR": str(config_dir),
            "CLAUDE_CODE_DISABLE_CLAUDE_MDS": "1"}
    return env


def route_env_problems(env: dict, route: str, allowed_base_urls) -> list[str]:
    """Closed tokens for a route child environment that is not exactly the harness-controlled route:
    the routing variables present must be ``ANTHROPIC_BASE_URL`` alone, naming one of the allowed
    endpoints (or the preflight's loopback stand-in), and no ``ANTHROPIC_*`` / ``CLAUDE_*`` name may
    exist beyond ``ROUTE_VARS`` and ``QUIET_ENV``."""
    problems = []
    if rg.routing_problems(env) != ["ANTHROPIC_BASE_URL"]:
        problems.append("routing_variables_differ")
    url = env.get("ANTHROPIC_BASE_URL", "")
    if url not in allowed_base_urls and not url.startswith("http://127.0.0.1:"):
        problems.append("route_base_url_not_allowed")
    allowed = set(ROUTE_VARS) | set(rg.QUIET_ENV)
    if any(k.upper().startswith(("ANTHROPIC_", "CLAUDE_")) and k not in allowed
           and k.upper() not in {b.upper() for b in rg.BASIC_ENV} for k in env):
        problems.append("route_extra_variable")
    return problems


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

    def key_problem(self) -> str | None:
        """Why the key file may not be used (None when it may); never says what the key is."""
        return None

    def provenance(self) -> dict:
        return {"cli": self.name, "provider": self.provider}


class ClaudeCLI(AgentCLI):
    name, provider = "claude-code", "anthropic"
    uses_claude_config_dir = True
    checks_anthropic_routing = True

    def __init__(self, args=None):
        super().__init__(args)
        self.route = getattr(args, "route", None)
        if self.route:
            self.provider = self.route

    def spec(self) -> dict:
        return CLAUDE_ROUTES[self.route]

    def route_url(self) -> str | None:
        return getattr(self.args, "route_base_url", None)

    def refusal(self):
        if not self.route:
            return None
        if self.route not in CLAUDE_ROUTES:
            return "--route must be one of: " + ", ".join(sorted(CLAUDE_ROUTES))
        if self.route_url() not in self.spec()["base_urls"]:
            return ("--route-base-url is required and must be one of: " + ", ".join(self.spec()["base_urls"]))
        if getattr(self.args, "key_file", None) is None:
            return "--key-file is required on a route (a file outside the checkout; its first line is the key)"
        if getattr(self.args, "max_usd_explicit", False):
            return ("a provider route takes no dollar cap: the CLI prices a response with Anthropic's prices for a "
                    "model it does not know, so its figure is invalid; pass --max-context-tokens-per-goal (and "
                    "--goal-timeout) instead")
        return None

    def read_key(self) -> str:
        import minimal_agent
        try:
            return minimal_agent.load_key(self.args.key_file)
        except minimal_agent.AgentError as error:
            raise SystemExit(f"the key file is refused: {error.token}") from None

    def key_problem(self):
        if not self.route:
            return None
        return route_key_problem(self.route, self.route_url(), self.read_key())

    def build_argv(self, *, model, mcp_config, system_instruction, max_budget_usd):
        return rg.build_claude_argv(claude=self.args.claude, model=model, mcp_config=mcp_config,
                                    system_instruction=system_instruction,
                                    max_budget_usd=None if self.route else max_budget_usd)

    def env(self, extra=None):
        if not self.route:
            return rg.agent_env(extra)
        env = route_env(self.route, self.route_url(), self.read_key(), self.args.model,
                        (extra or {}).get("CLAUDE_CONFIG_DIR") or self.args.claude_config_dir)
        if (problems := route_env_problems(env, self.route, self.spec()["base_urls"])):
            raise SystemExit("the route environment is not the harness-controlled one: " + ",".join(problems))
        return env

    def load_proof(self, path):
        proof = rg.load_isolation_proof(path)
        if self.route:
            route = proof.get("route")
            if not (isinstance(route, dict) and proof.get("route_verifier_revision") == ROUTE_VERIFIER_REVISION
                    and sorted(proof.get("request_tools", [])) == sorted(rg.ALLOWED_TOOL_NAMES)):
                raise SystemExit("the isolation proof is not a route proof of the current route verifier "
                                 f"revision {ROUTE_VERIFIER_REVISION}; run preflight.py --route again")
        return proof

    def route_facts(self, model: str) -> dict:
        return {"provider": self.route, "base_url": self.route_url(), "model": model,
                "effort": self.spec()["effort"], "auth": "bearer"}

    def proof_problems(self, proof, args, config_dir, system_instruction=""):
        problems = rg.proof_problems(proof, args, config_dir)
        if self.route:
            if proof.get("route") != self.route_facts(args.model):
                problems.append("route_differs_from_proof")
        elif proof.get("route") is not None:
            problems.append("route_proof_for_a_native_run")
        return problems

    def provenance(self):
        if not self.route:
            return {"cli": self.name, "provider": self.provider}
        return {"cli": self.name, "provider": self.provider,
                "route_host": re.sub(r"^https?://([^/:]+).*$", r"\1", self.route_url() or ""),
                "max_context_tokens_per_goal": getattr(self.args, "max_context_tokens_per_goal", None),
                "dollar_cap": "none"}


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
        if getattr(self.args, "max_usd_explicit", False) and not getattr(self.args, "price", None):
            return ("the minimal agent enforces a dollar cap only with --price (USD per million tokens: fresh "
                    "input, cached input, output); without prices pass no --max-budget-usd and use --max-total-tokens")
        return None

    def key_problem(self):
        """A MiMo Token Plan key is for coding tools only: refused here, before anything starts, for every
        provider label (changing the label does not change what the key is)."""
        key_file = getattr(self.args, "key_file", None)
        if key_file is None:
            return None
        import minimal_agent
        try:
            key = minimal_agent.load_key(Path(key_file))
        except minimal_agent.AgentError as error:
            return f"the key file is refused: {error.token}"
        if key.startswith(TOKEN_PLAN_PREFIXES):
            return ("a MiMo Token Plan key (tp-) is not for scripts or application backends, whatever provider "
                    "label it is offered under: use the Claude Code route (--agent-cli claude --route mimo)")
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
                "--max-total-tokens", str(getattr(a, "max_total_tokens", 400_000)),
                "--completion-ceiling", str(self.ceiling())]
        if getattr(a, "phase", 1) == 1:
            argv += ["--dry-run-only"]
        if getattr(a, "price", None):
            argv += ["--price", a.price]
            if getattr(a, "max_usd_explicit", False):
                argv += ["--max-usd", f"{max_budget_usd:g}"]
        return argv

    def ceiling(self) -> int:
        import minimal_agent
        return getattr(self.args, "completion_ceiling", None) or minimal_agent.DEFAULT_COMPLETION_CEILING

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
                "extra_param_sha256": canonical_sha(self.request_extra()), "completion_ceiling": self.ceiling(),
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
