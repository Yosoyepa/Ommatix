# Security policy

Ommatix is a research prototype. Only the current `main` branch receives fixes; there are
no published releases or supported release lines yet. Production use is not supported.

## Report privately

Use [GitHub private vulnerability reporting](https://github.com/Yosoyepa/Ommatix/security/advisories/new).
Do not disclose a suspected vulnerability in a public issue, pull request or comment.
No response or resolution deadline is promised.

Reports about these boundaries are in scope:

- Bypassing the action gate, dry-run default, application allowlist, window-class restrictions,
  window pin or the operator's stop file.
- Defeating session, snapshot, revision, epoch, freshness or authorisation-age checks, including
  the boundary between a focusing click and typing keys.
- Screen, OCR or accessibility text changing permissions, actionability or policy through
  prompt injection.
- Leaking screen content, field values or secrets into logs or unexpected `structuredContent`;
  exposing screenshots or bypassing the redaction policy.
- Loading an artefact without its required SHA-256 pin
  ([ADR-0021](../docs/adr/0021-pin-every-loaded-artefact-and-keep-text-out-of-errors.md)).
- Exceeding MCP message-loop bounds or causing an external-input panic or denial of service.

Include the commit, OS, enabled build features, affected boundary, expected and actual
behaviour, and a minimal reproduction using the simulated desktop or fixtures where
possible. Describe the impact and any proposed fix. Share only sanitised configuration
keys and stderr diagnostics; remove credentials, file paths and personal data.

Do not include screenshots or screen text containing personal data. Do not send real input
or capture a real desktop to produce a report. Use synthetic labels and frames. Ordinary
configuration questions and unsupported-platform requests belong in [support](SUPPORT.md).

See [security and privacy](../docs/thesis/08_SEGURIDAD_Y_PRIVACIDAD.md) for the design and
[AGENTS.md](../AGENTS.md) for the fail-closed and privacy rules.
