# Architecture Decision Records

ADR-0001 to ADR-0012 come from the thesis dossier ([`docs/thesis/`](../thesis/README.md)),
translated into English; each keeps its original Spanish text in `## Resumen (ES)`. The
project owner accepted all twelve on 2026-10-14. ADR-0013 and ADR-0014 record decisions the
owner made when the repository was bootstrapped.

| # | Title | Status |
|---|---|---|
| [0001](0001-rust-runtime-and-independent-oracles.md) | Rust for the runtime; independent oracles | Accepted (2026-10-14) |
| [0002](0002-mcp-stdio-with-explicit-profiles.md) | MCP over stdio with explicit profiles | Accepted (2026-10-14) |
| [0003](0003-conservative-spatial-fusion-with-ioc.md) | Conservative spatial fusion with IoC | Accepted (2026-10-14) |
| [0004](0004-masked-open-set-appearance-cache.md) | Masked, open-set appearance cache | Accepted (2026-10-14) |
| [0005](0005-persistent-tracking-instead-of-cell-hash.md) | Persistent tracking instead of a cell hash | Accepted (2026-10-14) |
| [0006](0006-transactional-snapshots-and-commits.md) | Transactional snapshots and commits | Accepted (2026-10-14) |
| [0007](0007-fail-closed-actuation-and-honest-simulation.md) | Fail-closed actuation and honest simulation | Accepted (2026-10-14) |
| [0008](0008-one-real-platform-first.md) | One real platform before promising portability | Accepted (2026-10-14) |
| [0009](0009-models-and-licences-decided-in-the-spike.md) | Models and licences decided in the spike | Accepted (2026-10-14) |
| [0010](0010-performance-budgets-subject-to-measurement.md) | Performance budgets subject to measurement | Accepted (2026-10-14) |
| [0011](0011-modular-monolith-and-adaptive-incrementality.md) | Modular monolith and adaptive incrementality | Accepted (2026-10-14) |
| [0012](0012-data-minimisation-and-evidence-traceability.md) | Data minimisation and evidence traceability | Accepted (2026-10-14) |
| [0013](0013-bilingual-documentation.md) | Bilingual documentation (EN/ES) | Accepted |
| [0014](0014-hexagonal-crate-layout.md) | Hexagonal crate layout for the modular monolith | Accepted |

The twelve ADRs written while the repository was bootstrapped, before the dossier was
consolidated, are archived in [`bootstrap/`](bootstrap/README.md) as B0001–B0012. They
describe the prototype that `src/` still contains while it is realigned.

## Writing an ADR

Copy [0000-template.md](0000-template.md), take the next number and open it in the same PR as
the change it justifies. An accepted ADR is not edited; a new ADR supersedes it, and both are
kept. Test identifiers (UT, IT, CT, SEC, PERF, EV, E2E, DOC) refer to the planned tests in
[`docs/thesis/matrices/plan_pruebas.csv`](../thesis/matrices/plan_pruebas.csv).

Evidence labels follow the dossier: [F0] is the source conversation (kept outside the
repository); S01–S24 are the primary sources catalogued in
[`docs/thesis/fuentes/referencias.json`](../thesis/fuentes/referencias.json); [P] marks a new
proposal and [D] a deduction of our own. The thesis CI job checks that every S reference in
an ADR exists in that catalogue.

## Resumen (ES)

ADR-0001 a ADR-0012 son las de la tesis, traducidas al inglés y con su texto original en
español. El responsable del proyecto aprobó las doce el 14 de octubre de 2026. ADR-0013 y ADR-0014 recogen las
decisiones del arranque: documentación bilingüe y crates hexagonales. Las ADR del arranque se
archivan en `bootstrap/` como B0001–B0012.
