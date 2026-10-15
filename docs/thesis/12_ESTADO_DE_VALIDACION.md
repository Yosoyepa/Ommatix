# 12. Estado de validación y límites de la entrega

> **Nota de consolidación (14 de octubre de 2026).** Este capítulo describe el dossier tal como se entregó el 13 de octubre, antes de consolidarlo en el repositorio. Desde entonces la integración continua ejecuta el oráculo y el validador en cada cambio, y `referencia_ejecutable/resultados/` guarda la última ejecución registrada. El repositorio sí compila Rust (`cargo test --workspace`) y se realinea con este dossier siguiendo S0–S4 (capítulo 06). Desde S2b, `ommatix-domain` implementa los algoritmos 1, 2, 3A y 3B y los snapshots transaccionales, y el motor une los algoritmos 1, 2, 3A y 3B con el contrato MCP Ommatix 1.0 sobre escritorios simulados: ejecuta las pruebas CT-01..CT-10 y los escenarios BDD-01..BDD-27 con step definitions (`CHANGELOG.md`, 1.0.2–1.0.4). El caché de íconos sólo actúa con prototipos y umbrales que configura el operador, todavía sin calibrar. La primera parte de S3 añade, tras features de compilación apagadas por defecto, la captura DXGI, la entrada `SendInput` (que atestigua la aplicación en primer plano), un detector ONNX que falla de forma segura y el OCR de Windows; BDD-29..31 se ejecutan sobre un doble del actuador real (`CHANGELOG.md`, 1.0.5). Después se añaden la parada local del operador (`[safety] stop_file`), un último vistazo a la política y a la aplicación en primer plano justo antes de entregar y la comprobación del foco de teclado antes de escribir; BDD-28 y BDD-32 se ejecutan sobre el mismo doble (`CHANGELOG.md`, 1.0.6). Hasta S0, ningún modelo estaba seleccionado y ninguna prueba IT ni SEC se había ejecutado. El spike S0 ([`S0_RESULTADOS.md`](S0_RESULTADOS.md), `CHANGELOG.md` 1.0.8) selecciona para el spike el detector `icon_detect_v3` de OmniParser (MIT) en CPU y ejecuta la captura DXGI, el detector y el OCR reales en corridas supervisadas sobre `notepad.exe`. IT-01 e IT-02 quedan ejecutadas sólo en parte. Una inferencia cuesta unos 7,5 s y la precisión no está medida. Las corridas supervisadas de S3 ([`S3_CORRIDAS_SUPERVISADAS.md`](S3_CORRIDAS_SUPERVISADAS.md), `CHANGELOG.md` 1.0.9) envían por primera vez entrada real (cuatro clics sobre un menú de `notepad.exe`) y ejecutan SEC-01 y, en parte, IT-01, IT-03, IT-05, SEC-02 y SEC-05 con un cliente MCP mínimo propio. No se ha probado un cliente MCP de terceros (S3–S4). Por eso lo que este capítulo dice de la conformidad con un cliente MCP real, de la captura y de la ejecución en Windows sigue vigente.

## 12.1 Resultado efectivamente obtenido

**La entrega es un dossier de diseño revisado y una referencia algorítmica parcial, no un runtime de escritorio terminado.** Los documentos definen un MVP funcional que todavía debe implementarse e integrarse. El estado del producto permanece pendiente aunque una verificación documental o un oráculo esté aprobado.

| Verificación realizada [V] | Resultado | Evidencia |
|---|---|---|
| Suite de referencia Python | 48 tests; 0 fallos, 0 errores, 0 omitidos | oraculo_report.json y oraculo_unittest.log |
| Schemas de dominio Draft 2020-12 | 7 schemas aceptados por el metaschema | validacion_documental.json |
| Ejemplos de payload | 9 válidos aceptados y 8 inválidos rechazados | validacion_casos.json e informe |
| Mensajes JSONL ilustrativos | 13 mensajes analizados; controles de metadatos y contenido | validacion_documental.log |
| Gherkin | 32 escenarios en 6 archivos; revisión estructural propia | matriz e informe documental |
| Trazabilidad | 28 requisitos, 76 casos planificados y 32 escenarios relacionados | matrices/*.csv |
| Referencias y decisiones | 24 fuentes primarias y 12 ADR identificados | fuentes/referencias.json y ../adr/ |

La comprobación Gherkin revisa encabezados, IDs y la presencia de Dado/Cuando/Entonces. **No sustituye el parser oficial de Gherkin ni ejecuta step definitions.** La validación de metadatos JSONL tampoco constituye una suite de conformidad MCP.

El informe documental registra cuántos controles estructurales ejecutó en la última corrida. Puede aumentar al incorporar nuevos documentos, sin que cambien los 17 casos de payload ni las 48 pruebas del oráculo.

## 12.2 Entorno y reproducibilidad

La suite de referencia se ejecutó en Python 3.13.5, Linux-6.18.44-x86_64-with-glibc2.41. Los schemas se validaron con jsonschema 4.26.0. El reloj UTC y los contadores están registrados en los JSON de resultados, no se infieren de una captura de pantalla.

Desde `docs/thesis/`:

```bash
python referencia_ejecutable/run_tests.py
python validar_paquete.py
```

El primer comando usa exclusivamente la biblioteca estándar. El segundo requiere jsonschema 4.x; `requirements-validacion.txt` fija la versión utilizada en esta entrega. Instalar dependencias es una operación del entorno del usuario, no una descarga automática del validador. Ambas verificaciones pueden ejecutarse sin acceso al escritorio ni a una GPU.

Los tiempos que aparezcan en el log de unittest son duración de pruebas sintéticas. **No son latencia de percepción, captura, inferencia, MCP ni control del SO.** No se generaron resultados artificiales en las plantillas de benchmark.

## 12.3 Qué cubre la referencia ejecutable

La referencia implementa geometría validada, diferenciación exacta BGRA con stride, cierre contextual acotado, selección interna de propietario, orden de lectura determinista, un corredor geométrico limitado, comparación de gradientes con máscara y contraejemplos a los hashes originales. También incluye estado transaccional y un registro simulado con vigencia de referencias.

La asociación temporal se suministra explícitamente en los tests del registro. **No hay un tracker visual completo**, un optimizador global de asignaciones ni indexación R*-Tree en el oráculo. Esta separación permite comprobar invariantes sin fingir detección visual ni equivalencia total con el runtime propuesto.

Las clases y casos OR-01..OR-48 se relacionan con pruebas productivas cuando hay cobertura parcial. No toda fila de la matriz planificada tiene una implementación: los campos de estado lo indican. Un OR aprobado no convierte automáticamente su UT, IT o E2E relacionado en aprobado.

## 12.4 Qué no fue ejecutado

No se compiló Rust: el entorno de revisión no dispone de cargo/rustc. `plantillas/puertos_rust.rs` es una plantilla de interfaces, no una biblioteca compilada. No se entrega Cargo.toml ni se anuncia `cargo test` como comando que funcione en este paquete.

No se ejecutó un servidor o cliente MCP real, el parser oficial de Gherkin, ni escenarios BDD con step definitions. Tampoco se cargaron pesos ONNX, se probó DirectML/WinML, se capturó una pantalla de Windows o se inyectaron eventos. No se midieron consumo de RAM del producto, cobertura del runtime Rust, p95/p99 de percepción ni exactitud en aplicaciones reales.

Los cuatro diagramas cuentan con fuentes Mermaid editables. Las representaciones Graphviz de la edición de lectura se generan a partir de la misma arquitectura y conservan su propio archivo DOT; no se presentan como ejecución de Mermaid.

## 12.5 Condiciones para declarar el MVP funcional

Se deben cerrar S0 y los gates S1–S4, fijar hardware/modelos/licencias, implementar adaptadores reales y ejecutar las matrices sobre la plataforma objetivo. Las evidencias mínimas son capturas y entradas reales, éxito E2E, validación temporal, rechazo seguro, trazas de rendimiento reproducibles y conformidad del perfil MCP seleccionado.

Hasta que esas evidencias existan, las casillas DoD permanecen abiertas. Desde el 15 de octubre de 2026, con el alcance reducido del MVP (capítulo 06 §6.7), tres de las diez casillas están marcadas con su evidencia (reevaluación de la 1.0.58) y las demás no; EV-01, EV-02, IT-07 y el corpus de REL-01 pasan al próximo corte. Las metas de rendimiento y calidad son criterios propuestos, no resultados. El marco de tesis es una base investigativa con hipótesis y metodología; su capítulo de resultados empíricos queda pendiente de experimentación.
