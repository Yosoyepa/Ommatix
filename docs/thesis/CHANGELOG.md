# Historial documental

## 1.0.3 — 2026-10-14: S2a, caché de apariencia e identidad

No cambia ningún requisito, algoritmo, contrato ni escenario. Cambia el estado de las matrices:

- UT-27 a UT-43 pasan a `IMPLEMENTADO_EN_RUST_S2` en `matrices/plan_pruebas.csv`. Son pruebas de los módulos `appearance` (algoritmo 3A) e `identity` (algoritmo 3B y comprobaciones de vigencia de una acción) de `ommatix-domain`.
- Los contraejemplos del diseño original (dHash de 64 bits, ID FNV por celda, probabilidad de colisión) sólo existen dentro de las pruebas UT-27, UT-32 y UT-33.
- El caché no fija umbrales de distancia ni de margen: la tesis los calibra antes del test (H4), así que quien lo usa debe elegirlos. El IoU mínimo de asociación (0,5) es un valor inicial [P].
- BDD-11..21 y el contrato MCP (CT-01..10, BDD-22..26) quedan para S2b.

## 1.0.2 — 2026-10-14: S1, núcleo geométrico e incremental

No cambia ningún requisito, algoritmo, contrato ni escenario. Cambia el estado de las matrices:

- UT-01 a UT-26 pasan a `IMPLEMENTADO_EN_RUST_S1` en `matrices/plan_pruebas.csv`. Son pruebas del crate `ommatix-domain` (módulos `incremental`, `snapshot`, `fusion` y `bbox`) que llevan su ID en el nombre y reproducen los casos del oráculo indicados en la matriz.
- BDD-01 a BDD-10 pasan a `EJECUTADO_SOBRE_NUCLEO_S1` en `matrices/gherkin_trazabilidad.csv`. Se ejecutan desde esta carpeta con `src/acceptance/tests/thesis.rs` y ejercitan el núcleo directamente; el recorrido por MCP llega con el contrato en S2. BDD-11 y BDD-12 esperan a S2 porque hablan del ID del elemento y de su publicación.
- Los requisitos RF-02 a RF-07 siguen pendientes: su aceptación exige capturas, detector y OCR reales.
- Diferencia conocida con el oráculo: la normalización de texto en Rust usa NFC y `to_lowercase`; el oráculo usa `casefold`, que además convierte caracteres como `ß` en `ss`. Ninguna prueba del plan depende de esa diferencia.

## 1.0.1 — 2026-10-14: consolidación en el repositorio

El dossier entra al repositorio con el nombre final del proyecto, Ommatix, y pasa a ser su línea base. No cambia ningún requisito, algoritmo, contrato ni escenario; los cambios son de ubicación y de estado:

- Los capítulos, contratos, matrices, plantillas, features, diagramas y la referencia ejecutable viven en `docs/thesis/`. El índice `LEEME.md` pasa a ser `README.md`.
- Las doce ADR pasan a `docs/adr/` como ADR-0001 a ADR-0012, en inglés y con su texto original en español. Se añaden ADR-0013 (documentación bilingüe) y ADR-0014 (crates hexagonales), que recogen decisiones del arranque del repositorio. Las ADR escritas durante ese arranque se archivan en `docs/adr/bootstrap/`.
- El responsable del proyecto aprueba las doce ADR el 14 de octubre de 2026, entre ellas ADR-0008: Windows 11 x64, un monitor y DPI de 100, 125 y 150 %. Siguen pendientes las aplicaciones de prueba, el cliente MCP, los pesos, las licencias, el hardware de evaluación, el corpus y las políticas de uso.
- `validar_paquete.py` lee los capítulos de esta carpeta y las ADR de `docs/adr/`. La integración continua ejecuta el oráculo y el validador en cada cambio.
- Las vistas de `diagramas/` tienen una versión con etiquetas en inglés en `docs/diagrams/`. Los `.svg` no se versionan; se generan desde los `.dot`.
- La conversación original [F0], la edición de lectura en PDF y Word, el manifiesto, las sumas SHA-256 y el informe de maquetación se conservan fuera del repositorio. La integridad la da git.

## 1.0 revisada — 2026-10-13

Se materializa la solicitud de documentos del PDF en archivos separados y dossier de lectura. Se preserva la fuente original sin modificaciones. Se añaden auditoría de 24 hallazgos, fuentes primarias, requisitos, contratos, ADR y validación trazable.

Cambios sustantivos: IDs por tracking en lugar de FNV cuantizado; descriptor perceptual con soporte/rechazo; incrementalidad transaccional; accionabilidad conservadora; política de ejecución; perfiles MCP actuales; selección de modelos por spike; presupuesto medido y separación explícita entre documentación, oráculos y MVP funcional.

El generador de Rust del PDF no se presenta como producto compilado. En lugar de transcribir código truncado y conservar defectos conocidos, se entrega una referencia escalar de pruebas y una plantilla de puertos Rust. Los documentos mantienen el runtime Rust como arquitectura objetivo.
