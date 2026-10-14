# Bootstrap ADRs (archived)

These twelve records were written on 13 and 14 October 2026, while the repository was
bootstrapped and before the thesis dossier ([`docs/thesis/`](../../thesis/README.md)) was
consolidated into it. They describe the prototype that `src/` contains today, which is being
realigned with the thesis in stages S0–S4. None of them is a current decision.

They keep a `B` prefix so that their numbers do not clash with the thesis ADRs, and they are
kept because code comments and the [MVP review](../../reviews/2026-10-14-mvp-review.md) cite
them. The two bootstrap decisions that still hold were recorded again: bilingual
documentation as [ADR-0013](../0013-bilingual-documentation.md) and the hexagonal crate layout
as [ADR-0014](../0014-hexagonal-crate-layout.md).

| # | Title | Superseded by |
|---|---|---|
| [B0001](B0001-record-architecture-decisions.md) | Record architecture decisions | [ADR index](../README.md) |
| [B0002](B0002-rust-perception-core.md) | Rust for the perception core | [ADR-0001](../0001-rust-runtime-and-independent-oracles.md) |
| [B0003](B0003-hexagonal-modular-monolith.md) | Hexagonal modular monolith as a Cargo workspace | [ADR-0014](../0014-hexagonal-crate-layout.md), [ADR-0011](../0011-modular-monolith-and-adaptive-incrementality.md) |
| [B0004](B0004-mcp-stdio-agent-interface.md) | MCP over stdio as the agent interface | [ADR-0002](../0002-mcp-stdio-with-explicit-profiles.md) |
| [B0005](B0005-grid-incremental-perception.md) | Grid-based incremental perception and uniform-grid spatial index | [ADR-0011](../0011-modular-monolith-and-adaptive-incrementality.md), [ADR-0005](../0005-persistent-tracking-instead-of-cell-hash.md), [ADR-0003](../0003-conservative-spatial-fusion-with-ioc.md) |
| [B0006](B0006-deterministic-classical-detector.md) | Deterministic classical detector for the MVP | [ADR-0009](../0009-models-and-licences-decided-in-the-spike.md) |
| [B0007](B0007-bilingual-documentation.md) | Bilingual (EN/ES) documentation | [ADR-0013](../0013-bilingual-documentation.md) |
| [B0008](B0008-actuation-port-dry-run-default.md) | Actuation through an InputController port, dry-run by default | [ADR-0007](../0007-fail-closed-actuation-and-honest-simulation.md) |
| [B0009](B0009-gherkin-acceptance-through-mcp.md) | Executable Gherkin acceptance tests through MCP on a simulated desktop | Thesis test plan ([chapters 05](../../thesis/05_TDD_PLAN_Y_MATRIZ.md) and [06](../../thesis/06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md)) |
| [B0010](B0010-flat-region-detector.md) | Detect elements as flat-colour regions, without assuming a background colour | [ADR-0009](../0009-models-and-licences-decided-in-the-spike.md) |
| [B0011](B0011-incremental-perception-invariants.md) | Invariants of the incremental pipeline | [ADR-0006](../0006-transactional-snapshots-and-commits.md), [ADR-0011](../0011-modular-monolith-and-adaptive-incrementality.md) |
| [B0012](B0012-settled-actions-and-wire-contract.md) | Actions wait for the screen to settle; the wire contract is owned by the adapter | [ADR-0007](../0007-fail-closed-actuation-and-honest-simulation.md), [ADR-0002](../0002-mcp-stdio-with-explicit-profiles.md) |

## Resumen (ES)

Las doce ADR escritas al arrancar el repositorio, antes de consolidar la tesis. Describen el
prototipo que hoy está en `src/`, que se realinea con la tesis en S0–S4; ninguna es una
decisión vigente. Se conservan con prefijo `B` porque el código y la revisión del MVP las
citan. Las dos decisiones del arranque que siguen vigentes se registraron de nuevo como
ADR-0013 y ADR-0014.
