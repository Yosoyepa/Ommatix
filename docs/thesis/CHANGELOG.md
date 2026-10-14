# Historial documental

## 1.0.1 — 2026-10-14: consolidación en el repositorio

El dossier entra al repositorio con el nombre final del proyecto, Ommatix, y pasa a ser su línea base. No cambia ningún requisito, algoritmo, contrato ni escenario; los cambios son de ubicación y de estado:

- Los capítulos, contratos, matrices, plantillas, features, diagramas y la referencia ejecutable viven en `docs/thesis/`. El índice `LEEME.md` pasa a ser `README.md`.
- Las doce ADR pasan a `docs/adr/` como ADR-0001 a ADR-0012, en inglés y con su texto original en español. Se añaden ADR-0013 (documentación bilingüe) y ADR-0014 (crates hexagonales), que recogen decisiones del arranque del repositorio. Las ADR escritas durante ese arranque se archivan en `docs/adr/bootstrap/`.
- ADR-0008 queda aprobada el 14 de octubre de 2026: Windows 11 x64, un monitor y DPI de 100, 125 y 150 %. Las aplicaciones de prueba siguen pendientes.
- `validar_paquete.py` lee los capítulos de esta carpeta y las ADR de `docs/adr/`. La integración continua ejecuta el oráculo y el validador en cada cambio.
- Las vistas de `diagramas/` tienen una versión con etiquetas en inglés en `docs/diagrams/`. Los `.svg` no se versionan; se generan desde los `.dot`.
- La conversación original [F0], la edición de lectura en PDF y Word, el manifiesto, las sumas SHA-256 y el informe de maquetación se conservan fuera del repositorio. La integridad la da git.

## 1.0 revisada — 2026-10-13

Se materializa la solicitud de documentos del PDF en archivos separados y dossier de lectura. Se preserva la fuente original sin modificaciones. Se añaden auditoría de 24 hallazgos, fuentes primarias, requisitos, contratos, ADR y validación trazable.

Cambios sustantivos: IDs por tracking en lugar de FNV cuantizado; descriptor perceptual con soporte/rechazo; incrementalidad transaccional; accionabilidad conservadora; política de ejecución; perfiles MCP actuales; selección de modelos por spike; presupuesto medido y separación explícita entre documentación, oráculos y MVP funcional.

El generador de Rust del PDF no se presenta como producto compilado. En lugar de transcribir código truncado y conservar defectos conocidos, se entrega una referencia escalar de pruebas y una plantilla de puertos Rust. Los documentos mantienen el runtime Rust como arquitectura objetivo.
