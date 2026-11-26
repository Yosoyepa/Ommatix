<!-- Read AGENTS.md and .github/CONTRIBUTING.md first. Tick only what you ran; write "n/a" beside a box that does not apply. -->

## Summary / Resumen

<!-- What changes and why. Qué cambia y por qué. -->

## Affected crates / Crates afectados

- [ ] domain
- [ ] application
- [ ] adapter-accessibility
- [ ] adapter-capture
- [ ] adapter-vision
- [ ] adapter-input
- [ ] adapter-sim
- [ ] adapter-mcp
- [ ] app
- [ ] acceptance (Gherkin)
- [ ] thesis dossier (`docs/thesis/`)
- [ ] docs only

## Checklist / Lista de comprobación

**Checks / Comprobaciones**

- [ ] `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` pass
- [ ] Thesis checks pass (`python referencia_ejecutable/run_tests.py`, `python validar_paquete.py` in `docs/thesis/`)

**Specification / Especificación**

- [ ] Follows the thesis dossier; a change to a requirement, algorithm, contract or scenario updates it and its CHANGELOG
- [ ] Tests that implement planned tests carry their IDs (UT-xx, CT-xx…)
- [ ] New user-visible behaviour has a Gherkin scenario traced to its requirement

**Design / Diseño**

- [ ] Dependency rule respected (AGENTS.md §3)
- [ ] ADR added or updated if an architectural decision changed
- [ ] Hot path touched? Per-stage timings before/after included

**Documentation / Documentación**

- [ ] Paired docs updated in both languages (`*.md` + `*.es.md`)
