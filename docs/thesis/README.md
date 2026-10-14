# Ommatix | Dossier de ingeniería e investigación

**Versión 1.0 revisada · 13 de octubre de 2026 · consolidada en el repositorio el 14 de octubre de 2026**

Dossier preparado a partir de la conversación “Arquitectura de Computación Visual para Agentes” [F0]. Contiene los documentos solicitados, una auditoría técnica con correcciones explícitas y pruebas de referencia reproducibles. **El dossier no es un MVP de control de escritorio compilado o desplegado.** Sí define con precisión qué debe implementarse y medirse para declararlo funcional.

Es la línea base del repositorio: el código de `src/` se realinea con este dossier siguiendo el plan S0–S4 del capítulo 06. Mientras dure esa transición, el [README del repositorio](../../README.md) indica qué partes del código ya siguen la tesis.

## Lectura principal

Los Markdown de esta carpeta son la fuente editable. La edición consolidada en PDF y Word, y la conversación original [F0], se conservan fuera del repositorio.

Para tomar decisiones, leer primero la auditoría (00), el SDD (02) y los requisitos (03). Para implementar el core, continuar con algoritmos (04), pruebas (05), integración/DoD (06) y contratos (07). Para el trabajo académico, leer el marco de tesis (01), evaluación (09) y referencias (11).

## Índice de documentos

| Archivo | Contenido |
|---|---|
| [00_AUDITORIA_TECNICA.md](00_AUDITORIA_TECNICA.md) | 24 hallazgos, correcciones y distinción de evidencia |
| [01_TESIS_Y_MARCO_TEORICO.md](01_TESIS_Y_MARCO_TEORICO.md) | Problema, antecedentes, hipótesis, fundamentos y metodología |
| [02_SDD_ARQUITECTURA_MVP.md](02_SDD_ARQUITECTURA_MVP.md) | Componentes, puertos, datos, concurrencia y acciones |
| [03_ESPECIFICACION_REQUISITOS.md](03_ESPECIFICACION_REQUISITOS.md) | Alcance funcional y criterios de aceptación |
| [04_ALGORITMOS_CORE.md](04_ALGORITMOS_CORE.md) | Formulación, pseudocódigo, invariantes y complejidad |
| [05_TDD_PLAN_Y_MATRIZ.md](05_TDD_PLAN_Y_MATRIZ.md) | Estrategia TDD y relación con la referencia ejecutable |
| [06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md](06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md) | Spike, integración, hitos, DoD y evolución |
| [07_CONTRATOS_MCP_Y_ERRORES.md](07_CONTRATOS_MCP_Y_ERRORES.md) | Perfiles MCP, acciones, payloads y taxonomía de errores |
| [08_SEGURIDAD_Y_PRIVACIDAD.md](08_SEGURIDAD_Y_PRIVACIDAD.md) | Amenazas, permisos, prompt injection y protección de datos |
| [09_EVALUACION_EXPERIMENTAL.md](09_EVALUACION_EXPERIMENTAL.md) | Dataset, baselines, métricas, comparación y ablaciones |
| [10_RUNBOOK_Y_DEMO.md](10_RUNBOOK_Y_DEMO.md) | Procedimiento de validación y demo futura con control real |
| [11_REFERENCIAS_Y_PROVENIENCIA.md](11_REFERENCIAS_Y_PROVENIENCIA.md) | 24 fuentes primarias y trazabilidad con el PDF |
| [12_ESTADO_DE_VALIDACION.md](12_ESTADO_DE_VALIDACION.md) | Resultados efectivamente verificados y pendientes |
| [S0_RESULTADOS.md](S0_RESULTADOS.md) | Informe del spike S0: detector OmniParser `icon_detect_v3`, OCR de Windows, hashes y latencias medidas |
| [S3_CORRIDAS_SUPERVISADAS.md](S3_CORRIDAS_SUPERVISADAS.md) | Corridas supervisadas de S3 con captura y entrada reales sobre `notepad.exe`: acciones enviadas, estados, códigos, métricas y hallazgos |
| [S4_ACEPTACION_REDUCIDA.md](S4_ACEPTACION_REDUCIDA.md) | Protocolo S4 reducido: 20 corridas E2E puntuadas sólo con clics (aplicación de prueba propia y Bloc de notas), Calculadora no puntuada, falsos éxitos, latencias, hallazgos y lo que falta para las 450 corridas |

## Artefactos de implementación y aceptación

[`../adr/`](../adr/README.md) contiene los 12 registros de decisión de la tesis (ADR-0001 a ADR-0012, en inglés con la versión original en español). [`features/`](features) contiene 33 escenarios Gherkin en seis archivos, pendientes de step definitions; pasan a `src/acceptance/features/` a medida que el runtime los ejecuta. [`contratos/`](contratos/README.md) contiene siete esquemas JSON, 17 casos de payload y ejemplos MCP. [`matrices/`](matrices) contiene 28 requisitos, 77 pruebas planificadas, trazabilidad BDD y 14 riesgos.

[`diagramas/`](diagramas/README.md) contiene fuentes Mermaid y DOT; las mismas vistas, con etiquetas en inglés, están en [`../diagrams/`](../diagrams/README.md). [`plantillas/`](plantillas) contiene configuración segura, inventario de modelos, perfil de hardware, resultados de evaluación vacíos e interfaces Rust. [`fuentes/referencias.json`](fuentes/referencias.json) cataloga las 24 fuentes primarias.

## Validación reproducible

Requiere Python 3.11 o posterior. El oráculo sólo usa biblioteca estándar; la validación de schemas necesita jsonschema, fijado en `requirements-validacion.txt`. Desde esta carpeta:

```bash
python referencia_ejecutable/run_tests.py
python validar_paquete.py
```

La integración continua del repositorio ejecuta ambos comandos en cada cambio. Resultados incluidos: **48/48 pruebas de referencia aprobadas**, siete schemas válidos y **17/17 expectativas de payload satisfechas**. Los ejemplos inválidos deben ser rechazados: que fallen la validación es el resultado correcto. Los informes están en `referencia_ejecutable/resultados/`.

Estas verificaciones no prueban un servidor MCP, una integración OCR/ONNX, un actuador Windows ni tiempos del producto. La plantilla Rust no se compiló. Los tests son una referencia para trasladar invariantes al runtime, no un reemplazo del desarrollo ni de la aceptación E2E.

## Decisiones pendientes

El responsable del proyecto aprobó las doce ADR el 14 de octubre de 2026 ([docs/adr/](../adr/README.md)). La plataforma inicial es Windows 11 x64, un monitor seleccionado y DPI de 100, 125 y 150 % ([ADR-0008](../adr/0008-one-real-platform-first.md)). Las aplicaciones de prueba siguen siendo una propuesta de reducción de alcance pendiente de aprobación. Los modelos, pesos, licencias, Execution Provider y hardware se fijan en S0. No se incluye ni distribuye ningún peso de modelo.

Los IDs espaciales de cinco dígitos y la invariancia binaria del dHash original se sustituyen explícitamente en el diseño revisado. La geometría y el OCR no bastan para afirmar accionabilidad; se incorpora abstención. Se mantienen las metas de latencia como hipótesis y objetivos sujetos a benchmark.

## Integridad y cambios

`CHANGELOG.md` resume la revisión. Dentro del repositorio, la integridad y el historial de cambios los da git, por lo que el manifiesto y las sumas SHA-256 del paquete original no se incluyen. Los informes se regeneran al volver a ejecutar pruebas; se versionan cuando cambia un resultado, y si sólo cambió la fecha se descartan con `git restore referencia_ejecutable/resultados`.

Convención: [F0] = conversación original; [Sxx] = fuente externa primaria; [D] = deducción; [P] = propuesta; [V] = validación ejecutada. Las citas del dossier remiten a la bibliografía del capítulo 11. No hay resultados de tesis inventados ni DoD del producto marcado como cumplido.
