## Summary / Resumen

<!-- What changes and why. Qué cambia y por qué. -->

## Affected crates / Crates afectados

- [ ] domain
- [ ] application
- [ ] adapter-capture
- [ ] adapter-vision
- [ ] adapter-mcp
- [ ] app
- [ ] docs only

## Checklist

- [ ] `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` pass
- [ ] Dependency rule respected (AGENTS.md §3)
- [ ] Paired docs updated in both languages (`*.md` + `*.es.md`)
- [ ] ADR added or updated if an architectural decision changed
- [ ] Hot path touched? Per-stage timings before/after included
