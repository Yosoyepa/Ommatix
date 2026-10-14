## Summary / Resumen

<!-- What changes and why. Qué cambia y por qué. -->

## Affected crates / Crates afectados

- [ ] domain
- [ ] application
- [ ] adapter-capture
- [ ] adapter-vision
- [ ] adapter-input
- [ ] adapter-sim
- [ ] adapter-mcp
- [ ] app
- [ ] acceptance (Gherkin)
- [ ] thesis dossier (`docs/thesis/`)
- [ ] docs only

## Checklist

- [ ] `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` pass
- [ ] Thesis checks pass (`python referencia_ejecutable/run_tests.py`, `python validar_paquete.py` in `docs/thesis/`)
- [ ] Follows the thesis dossier; a change to a requirement, algorithm, contract or scenario updates it and its CHANGELOG
- [ ] Tests that implement planned tests carry their IDs (UT-xx, CT-xx…)
- [ ] Dependency rule respected (AGENTS.md §3)
- [ ] New user-visible behaviour has a Gherkin scenario traced to its requirement
- [ ] Paired docs updated in both languages (`*.md` + `*.es.md`)
- [ ] ADR added or updated if an architectural decision changed
- [ ] Hot path touched? Per-stage timings before/after included
