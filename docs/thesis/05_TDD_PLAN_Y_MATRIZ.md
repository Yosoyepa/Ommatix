# 05. Estrategia TDD, BDD y trazabilidad

## 5.1 Qué está ejecutado y qué es una especificación

La matriz `matrices/plan_pruebas.csv` identifica UT, integración, contratos, seguridad, E2E, rendimiento y evaluación. Las pruebas **UT** son compromisos de la implementación productiva; **OR** identifica comprobaciones de la referencia Python. Aprobar una OR no equivale a aprobar el UT completo en Rust. El informe local registra 48 tests OR, sin detector, OCR, MCP ni Windows. Los identificadores **S4R-xx** son los flujos del protocolo reducido de S4 (S4R-02, Calculadora; S4R-03, Bloc de notas) y no sustituyen a E2E-02 ni a E2E-03 de la matriz.

Las características `features/*.feature` están redactadas en español y contienen comportamientos de aceptación. Se entregan como especificaciones BDD, no como pruebas Cucumber ejecutadas: aún deben enlazarse a step definitions del runtime. Se mantiene el formato de Gherkin documentado por Cucumber [S24], con una línea por paso y etiquetas de trazabilidad.

## 5.2 Bucle Red–Green–Refactor propuesto

Para cada hallazgo crítico, escribir primero un test que falle con la estrategia defectuosa. Registrar fallo, implementar el cambio mínimo y ejecutar toda la suite. Refactorizar sólo manteniendo resultados y revisar el coste después de la corrección. No escribir retrospectivamente “se siguió TDD” si no existe historial de commits y ejecuciones red/green.

Ejemplo: UT-03 cambia un único píxel en una fila impar; un diferenciador que visita sólo filas pares debe fallar. El oráculo exacto establece la salida esperada. Una versión SIMD posterior debe coincidir para fixtures aleatorios y adversariales. OR-29 y OR-34 muestran contraejemplos de dHash y celda, mientras OR-30 y OR-37 validan sólo las propiedades sustitutas correspondientes.

## 5.3 Pirámide de pruebas

**Core puro:** geometría, regiones, descriptor, registro y precondiciones con clock y fixtures inyectados. Sin esperas reales ni APIs del SO. **Propiedades:** permutaciones, traslaciones, escalados controlados, buffers con padding y secuencias generadas. **Integración:** captura/modelos/actuación con permisos y errores reales. **Contratos:** clientes MCP, schemas y límites. **E2E:** una tarea con verificador independiente. **Rendimiento:** corpus separado y build release, nunca inferido del tiempo total de tests unitarios.

Los mocks sólo verifican intención y contratos de llamada. Un mock de SendInput no demuestra entrega a Windows. Un fixture de cajas no mide recall del detector. Una asociación de ID conocida por el test no valida un matcher visual.

## 5.4 Núcleo de regresiones

UT-01 a UT-15 cubren incrementalidad y commit; UT-16 a UT-26, fusión y affordance; UT-27 a UT-31, hashing/caché; UT-32 a UT-37, identidad; UT-38 a UT-43, referencias y simulación; UT-44, evidencia de accesibilidad (ADR-0017); UT-45, alcance de ventana de la política local (ADR-0020). La matriz contiene expectativa y correspondencia OR cuando existe.

**Pruebas que no necesitan entrada real ni pantalla (1.0.35).** IT-06, SEC-03, SEC-04, SEC-06, PERF-01 y DOC-01 se implementan en Rust y llevan su identificador en el nombre de cada función (`sec_03_…`, `it_06_…`, `perf_01_…`), sobre dobles de los puertos, el escritorio simulado y archivos temporales falsos: nunca sobre pesos, capturas ni entrada reales. REL-01 tiene sus comprobaciones automáticas (`rel_01_…`), pero su estado sigue pendiente (corpus de aceptación congelado, commit de entrega e instalación, ver `S4_REPRODUCIBILIDAD.md`). El estado de cada una en la matriz es `IMPLEMENTADO_EN_RUST_S4` sólo cuando se cumple su criterio con esa evidencia; lo que exige la máquina de referencia con las etapas reales (OCR y árbol de UI Automation reales ante texto hostil, revisión de los registros de una corrida en vivo) queda dicho en el `CHANGELOG.md`, no se marca como hecho. Una prueba de Rust acredita una prueba planificada sólo si lleva su identificador, y `src/acceptance/tests/repository.rs` comprueba en ambos sentidos que la matriz y el código coinciden.

**Varios agentes (ADR-0039 y ADR-0040, propuestas; 1.0.119).** IT-09 a IT-21 cubren el intermediario del adaptador MCP: entrega única entre clientes, página compartida, rechazo sin arrendamiento, paso, fin y cota del arrendamiento, concesión y diario por cliente, purga con el último cliente, igualdad con el servidor stdio para un cliente solo, fallo bajo el cerrojo e intercalados generados. CT-11 a CT-13 cubren el argumento `lease`, el metadato `io.ommatix/lease` y el servidor stdio que acepta `lease` sin cambiar nada; RF-19 las agrupa y BDD-63 a BDD-67 son sus escenarios. Corren sobre el escritorio simulado y dobles del actuador; ningún transporte ni el ejecutable intervienen.

**Transporte por un tubo local (ADR-0043 y ADR-0040, propuestas; 1.0.120).** RF-20 agrupa UT-76 a UT-80 (el archivo de concesiones, el interruptor y sus combinaciones prohibidas, la derivación del nombre del tubo, el archivo del tique y los códigos de salida de `attach`, y el fin de `serve` cuando el oyente deja de aceptar), IT-22 a IT-34 (el bucle síncrono frente al asíncrono, la concesión pedida en cada llamada, el saludo acotado, el tique de un solo uso, el tique que no sale, la sesión por el tubo igual que en memoria, un motor por nombre, la cota de clientes al aceptar, la parada del oyente, la retransmisión de `attach`, la falta de motor, el interruptor apagado byte a byte y los agentes por `attach` entre procesos reales) y SEC-08 a SEC-10 (descriptor sólo del propietario, cliente a nivel de identificación, clientes remotos rechazados); BDD-68 a BDD-71 son sus escenarios, sobre flujos en memoria. Las pruebas del tubo corren en Windows con la función, entre hilos y entre procesos reales; ninguna con pantalla, entrada real, un segundo usuario ni un proceso elevado. IT-35 (un anfitrión MCP real), IT-36 (la compilación MSVC y el trabajo de CI) y SEC-11 (un segundo usuario y un motor elevado) quedan planificadas y sin ejecutar (`PENDIENTE_PRODUCTO`).

Las lagunas explícitas incluyen: value frente a placeholder, matching perceptivo uno-a-uno, virtualización de filas, reglas contextuales del caché, concurrencia real y transformaciones de detector/OCR. Estos casos no se marcan verdes por semejanza con una prueba más simple.

## 5.5 Fixtures y corpus

Cada fixture productivo debe registrar origen, dimensiones, DPI, tema, formato, stride, clase de cambio y resultado esperado. Las capturas se anonimizan antes de versionar y se guardan fuera del repositorio público cuando contengan datos. Los fixtures sintéticos del oráculo se generan en memoria y no contienen credenciales.

Se proponen pruebas de propiedades con generador de semilla fija y reducción del caso fallido. Para equivalencia se usa un baseline escalar independiente de la implementación optimizada, no la misma función llamada dos veces. Para concurrencia se añaden pruebas del protocolo de commits y, en Rust, una herramienta apropiada de exploración de interleavings tras evaluar su integración.

## 5.6 Gates de CI propuestos

La rama de producto deberá ejecutar formato, análisis estático, tests unitarios/de propiedades, validación de contratos y revisión de dependencias. Se propone cobertura de líneas del core >=90% y cobertura explícita de todas las ramas de denegación críticas; cobertura numérica no reemplaza calidad de aserciones. La compilación e integración Windows requieren runner y permisos apropiados.

Los tests de carga y modelos van en jobs separados, con versiones y hardware identificados. No se hace fallar CI por una diferencia mínima de microsegundos en un host compartido; se usan budgets en hardware de referencia y detección de regresiones con tolerancias predefinidas. Los fallos intermitentes se investigan; no se reintentan hasta ocultarlos.

## 5.7 Ejecución de lo entregado

Desde `docs/thesis/`: `python referencia_ejecutable/run_tests.py`. Requiere Python 3.11+ y biblioteca estándar. Produce log y JSON en `referencia_ejecutable/resultados/`. El tiempo reportado es duración de tests en el entorno de prueba, no latencia del motor.

El validador adicional `validar_paquete.py` comprueba JSON Schemas y ejemplos cuando `jsonschema` está instalado. Su chequeo de Gherkin es estructural, no un parser Cucumber completo. Desde 1.0.35 comprueba también la trazabilidad de DOC-01: cada requisito resuelve sus pruebas planificadas (rangos y prefijos incluidos), cita ADR que existen, tiene propietario y evidencia cuyos archivos existen, ninguna prueba planificada queda sin requisito, escenario o riesgo, cada ADR nombra pruebas que existen y cada estado sale del vocabulario de las matrices; un requisito aceptado por cambio de alcance (`ACEPTADO_POR_ALCANCE`, ADR-0029) cita su nota de alcance. No decide si la evidencia basta: eso es el estado de cada prueba. La plantilla Rust muestra fronteras de diseño, no una implementación compilada. En el repositorio, `cargo test --workspace` prueba el código de `src/`, que se realinea con este dossier por etapas; un test Rust aprobado sólo acredita su UT cuando lleva su identificador.

## 5.8 Evidencia de cierre

Cada gate aprobado debe apuntar a commit, comando, entorno, fecha y archivo de resultados. Un check sin evidencia permanece pendiente. La trazabilidad separa pruebas locales de referencia, pruebas productivas y experimentos científicos; ninguna de las tres sustituye automáticamente a las otras dos.
