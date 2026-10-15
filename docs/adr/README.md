# Architecture Decision Records

ADR-0001 to ADR-0012 come from the thesis dossier ([`docs/thesis/`](../thesis/README.md)),
translated into English; each keeps its original Spanish text in `## Resumen (ES)`. The
project owner accepted all twelve on 2026-10-14. ADR-0013 and ADR-0014 record decisions the
owner made when the repository was bootstrapped. ADR-0015 records how S2b serves the MCP
contract, ADR-0016 bounds the `unsafe` that the Windows capture and input adapters of S3
need, ADR-0017 adds UI Automation evidence fused with the visual path, and ADR-0018 makes
`type` a two-phase action whose keys wait for element-level keyboard focus, and ADR-0019
runs the perception stages of one observation concurrently; the owner accepted all five on
2026-10-14. ADR-0020, accepted the same day, narrows the application allowlist to
windows (window classes and a pinned window) so that `explorer.exe` can be allowed without
the desktop and the taskbar. ADR-0021, accepted the same day, pins every artefact the
engine loads, the curated-icon file included, before loading anything, and keeps screen and
agent text out of protocol errors. ADR-0022, accepted the same day, makes the supervised test
clients ground the target point before every click and decide a closed list of navigating
clicks (E2E-03's breadcrumbs and Back) by the operating system's state instead of by the
identity of a control that the click itself rebuilds. ADR-0024, accepted the same day with the owner's
advance approval for INC-01 of the next-cut plan, times the commit and the response of an
observation as spans of their own, records the plan of every observation, and adds the
experimental full-frame baseline B0 behind a configuration switch that is off by default and is
never a production mode. ADR-0026, accepted on 2026-10-15 under the owner's pre-approval of the Paper's INC-04 increment, makes the
UI Automation runtime ID the discriminant of its element, scoped by process and window, and makes an uncertain
continuity abstain with a new ID and a recorded reason. ADR-0027, accepted the same day, adds an experimental R*-tree that enumerates the candidate controls of Algorithm 2 behind a switch that stays off by default; its results are the scan's, and the ADR does not decide whether it becomes the default.

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
| [0015](0015-mcp-transport-on-rmcp-wire-types.md) | MCP transport on rmcp wire types with a bounded message loop | Accepted (2026-10-14) |
| [0016](0016-bounded-unsafe-for-windows-capture-and-input.md) | Bounded unsafe for the Windows capture and input adapters | Accepted (2026-10-14) |
| [0017](0017-ui-automation-evidence-fused-with-vision.md) | UI Automation evidence, fused with the visual path | Accepted (2026-10-14) |
| [0018](0018-two-phase-type-with-element-level-focus-confirmation.md) | Two-phase type with element-level focus confirmation | Accepted (2026-10-14) |
| [0019](0019-concurrent-observation-stages.md) | Run the perception stages of one observation concurrently | Accepted (2026-10-14) |
| [0020](0020-operator-scope-on-windows.md) | The operator's scope on windows: window classes and a pinned window | Accepted (2026-10-14) |
| [0021](0021-pin-every-loaded-artefact-and-keep-text-out-of-errors.md) | Pin every artefact the engine loads before loading anything, and keep screen and agent text out of errors | Accepted (2026-10-14) |
| [0022](0022-ground-the-point-before-a-click-and-verify-the-os-after-a-navigating-click.md) | Ground the target point before every click, and verify the operating system after a click that rebuilds its own control | Accepted (2026-10-14) |
| [0023](0023-licence-and-reduced-mvp-scope.md) | Licence the code (MIT OR Apache-2.0) and the documents (CC BY 4.0), and close the MVP with an explicit reduced scope | Accepted (2026-10-15) |
| [0024](0024-observation-spans-and-a-switched-full-frame-baseline.md) | Time the commit and the response of an observation, record each observation's plan, and add the full-frame baseline B0 behind a switch that is off by default | Accepted (2026-10-15) |
| [0025](0025-bound-authorisations-by-snapshot-and-evidence-age.md) | Bound every authorisation by the age of the snapshot and of the accessibility evidence, enforced at every real-input boundary | Accepted (2026-10-15) |
| [0026](0026-native-identity-discriminant.md) | Native accessibility identity as the discriminant, with abstention and a recorded reason | Accepted (2026-10-15) |
| [0027](0027-candidate-enumeration-with-an-rstar-index.md) | Enumerate the candidate controls of Algorithm 2 with an R*-tree, behind a switch that stays off | Accepted (2026-10-15) |

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
decisiones del arranque: documentación bilingüe y crates hexagonales. ADR-0015, de S2b y aprobada el 14 de octubre de 2026, sirve MCP con los tipos de wire de `rmcp` y un bucle de mensajes
propio y acotado. ADR-0016, de S3 y aprobada el mismo día, limita el `unsafe` a
los módulos Windows de captura DXGI y entrada `SendInput`. ADR-0017, aprobada el mismo día, añade la
evidencia de UI Automation como un puerto más, fusionada con la visual y sin `unsafe` propio. ADR-0018,
aprobada el mismo día, entrega `type` en dos fases: las teclas esperan a que el foco del teclado esté en el campo destino, confirmado por UI Automation. ADR-0019, de RNF-05 y aprobada el mismo día, ejecuta de forma concurrente las etapas de percepción de una observación. ADR-0020, aprobada el mismo día, acota la lista de aplicaciones permitidas a ventanas (clases de ventana y una ventana registrada) para poder permitir `explorer.exe` sin el escritorio ni la barra de tareas. ADR-0021, aprobada el mismo día, comprueba por hash todo artefacto que carga el motor, el archivo de íconos curados incluido, antes de cargar nada, y no repite texto de pantalla ni del agente en los errores del protocolo. ADR-0022, aprobada el mismo día, hace que los clientes de prueba supervisada anclen el punto objetivo antes de cada clic y decidan una lista cerrada de clics de navegación (las migas de pan y «Atrás» de E2E-03) por el estado del sistema operativo y no por la identidad de un control que el propio clic reconstruye. ADR-0025, aprobada el 15 de octubre de 2026 con la aprobación previa del responsable para el INC-03 del siguiente corte, acota cada autorización por la edad de la captura del snapshot y por la de la lectura de accesibilidad que confirma el objetivo, con `max_snapshot_age_ms` (10 000 ms por defecto) y `STALE_SNAPSHOT` en cada frontera de entrada real. ADR-0024, aprobada el 15 de octubre de 2026 con la aprobación previa del responsable para el INC-01 del siguiente corte, fija los tramos de una observación en el registro de investigación (`total` hasta el snapshot publicado, `commit` y `response_build` propios), registra el plan de cada observación y añade la línea de base B0 detrás de un interruptor de configuración apagado por defecto, que nunca es un modo de producción. ADR-0026, aceptada el 15 de octubre de 2026 con la aprobación previa del responsable para el incremento INC-04 del paper, usa el `RuntimeId` de UI Automation de un elemento como su discriminante, acotado por proceso y ventana, y hace que una continuidad incierta se abstenga con un ID nuevo y un motivo registrado. ADR-0027, aprobada el mismo día, añade un R*-tree experimental que enumera los controles candidatos del Algoritmo 2 tras un conmutador desactivado por defecto; sus resultados son los del recorrido, y la ADR no decide si pasa a ser el valor por defecto. Las ADR del arranque se archivan en `bootstrap/` como B0001–B0012.
