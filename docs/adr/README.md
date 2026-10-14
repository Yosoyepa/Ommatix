# Architecture Decision Records

Format: lightweight [MADR](https://adr.github.io/madr/). English body plus a mandatory
`## Resumen (ES)` section. Copy [0000-template.md](0000-template.md), take the next number,
and open it in the same PR as the change it justifies. Superseded ADRs are kept and marked.

| # | Title | Status |
|---|---|---|
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-rust-perception-core.md) | Rust for the perception core | Accepted |
| [0003](0003-hexagonal-modular-monolith.md) | Hexagonal modular monolith as a Cargo workspace | Accepted |
| [0004](0004-mcp-stdio-agent-interface.md) | MCP over stdio as the agent interface | Accepted, amended by 0012 |
| [0005](0005-grid-incremental-perception.md) | Grid-based incremental perception and uniform-grid spatial index | Accepted, amended by 0011 |
| [0006](0006-deterministic-classical-detector.md) | Deterministic classical detector for the MVP | Superseded by 0010 |
| [0007](0007-bilingual-documentation.md) | Bilingual (EN/ES) documentation | Accepted |
| [0008](0008-actuation-port-dry-run-default.md) | Actuation through an InputController port, dry-run by default | Accepted, amended by 0012 |
| [0009](0009-gherkin-acceptance-through-mcp.md) | Executable Gherkin acceptance tests through MCP on a simulated desktop | Accepted |
| [0010](0010-flat-region-detector.md) | Detect elements as flat-colour regions, without assuming a background colour | Accepted |
| [0011](0011-incremental-perception-invariants.md) | Invariants of the incremental pipeline | Accepted |
| [0012](0012-settled-actions-and-wire-contract.md) | Actions wait for the screen to settle; the wire contract is owned by the adapter | Accepted |
