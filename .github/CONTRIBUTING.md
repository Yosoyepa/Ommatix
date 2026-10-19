# Contributing to Ommatix

Ommatix is a thesis research prototype. [AGENTS.md](../AGENTS.md) is the authoritative
contribution guide for people and AI coding agents; the [thesis dossier](../docs/thesis/README.md)
is the specification. Read both before changing behaviour.

## Set up

Install the exact Rust release in [rust-toolchain.toml](../rust-toolchain.toml) and Python
3.11 or later for thesis checks. Enable the repository hooks once per clone:

```bash
git config core.hooksPath .githooks
cargo run -- demo
```

The demo uses the simulated desktop and dry-run input. See the
[getting-started guide](../docs/guide/getting-started.md) and [documentation map](../docs/README.md).

## Make a change

- Branch from `main` using `feat/…`, `fix/…`, `docs/…`, `chore/…` or `refactor/…`; submit a PR.
- Use Conventional Commits with an [approved scope](../AGENTS.md#7-git-workflow), such as
  `fix(application): reject an expired snapshot` or `docs(repo): clarify setup`.
- Keep code, identifiers, comments and CLI/MCP text in English; the dossier is in Spanish.
  Update README and other existing Spanish twins together. Issues and PRs may use either language.
- A change to a requirement, algorithm, contract or scenario updates `docs/thesis/` and its
  `CHANGELOG.md` in the same PR. Only the owner/architect changes acceptance status.
- A bug fix includes a regression test that fails without the fix. Planned tests carry
  their test ID; expected results come from the matching independent oracle where one exists.
  Passing an oracle test does not mark the Rust test passed.
- Never write a test that injects real input. Use simulated or fixture desktops and pure
  actuator helpers. Never commit screenshots, screen recordings, captures, datasets, model
  weights, secrets or screen values. Keep screen and agent text out of diagnostics.

## Verify and submit

Run [all required checks in AGENTS.md §6](../AGENTS.md#6-commands-and-definition-of-done),
including formatting, workspace clippy/tests and the thesis oracle and validator. On Windows,
include the documented Windows-feature clippy checks when applicable. Offline Python helper
tests and the media build are documented in their tool directories. Do not bypass hooks.

Fill [the PR template](pull_request_template.md): explain the problem, final change, evidence,
tests and material limitations. Mark inapplicable entries as such in the description instead
of claiming checks you did not run. Hot-path changes need per-stage measurements. Keep the
READMEs about the current state; put detailed usage in guides and history in the CHANGELOG.

Use the [issue forms](https://github.com/Yosoyepa/Ommatix/issues/new/choose) for bugs,
feature proposals and documentation questions. Report vulnerabilities through
[private vulnerability reporting](SECURITY.md). Follow the [code of conduct](CODE_OF_CONDUCT.md).
