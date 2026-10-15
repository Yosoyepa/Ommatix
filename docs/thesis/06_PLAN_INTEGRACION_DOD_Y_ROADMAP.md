# 06. Integración, Definition of Done y evolución

## 6.1 Dependencias y estrategia

El PDF propone cuatro sprints y un MVP en cuatro semanas [F0, pp. 26–27]. Esta revisión conserva la progresión core → identidad → integración, pero incorpora un spike inicial y una etapa de aceptación: los modelos, licencias y adaptadores reales no están resueltos. El calendario siguiente es una estimación de planificación [P] para un desarrollador con dedicación técnica sustancial, no una fecha comprometida.

Ruta crítica: modelo con clases y licencia utilizables → captura/inferencia real → fusión con datos reales → contrato de identidad/acción → actuador seguro → verificador E2E. Documentación y oráculos pueden avanzar antes; la percepción real no se pospone hasta el último día.

## 6.2 S0. Viabilidad y contratos, 3–5 días efectivos propuestos

Registrar hardware y versiones. Probar una captura DXGI y su stride; ejecutar dos detectores candidatos y un OCR sobre un pequeño corpus; confirmar clases, licencias, exportación ONNX y proveedor de ejecución. Comprobar compatibilidad del cliente MCP objetivo con la revisión elegida. Definir el directorio sandbox, aplicaciones permitidas y responsables.

**Salida:** informe de spike con muestras, hashes, tabla de latencia real preliminar, defectos observados y decisión del modelo. Si ningún modelo distingue roles suficientes, adaptar el contrato a roles unknown o incorporar una fuente de evidencia aprobada; no inventar clases. No entrenar una variante nano sin registrarlo como ampliación de alcance.

## 6.3 S1. Core geométrico e incremental, 1–2 semanas propuestas

Implementar geometría válida, diferenciador exacto, cierre halo, fusión de renglones, propiedad de texto y staging/commit. Construir un replay offline de capturas reales y comparar contra oráculos. Mantener la ejecución del SO deshabilitada.

**Salida:** UT-01..UT-26 aprobados en el runtime, fixtures revisados y reporte de cobertura. Medir el core sin mezclar inferencia. Entregar una visualización de diagnóstico local que permita inspeccionar bbox/relaciones sin exponer datos al agente remoto.

## 6.4 S2. Identidad, caché y contrato externo, 1–2 semanas propuestas

Implementar registros de ID/revisión, matcher con abstención, bajas/tombstones y caché contextual con límites. Integrar MCP moderno mediante SDK; legacy sólo con alcance explícito. Validar todos los inputs y errores, snapshots, paginación y dry-run. Añadir un diario de request_id que no almacene texto sensible en claro.

**Salida:** UT-27..UT-43 y CT aprobados; demostración de fallos seguros por snapshot viejo, target ambiguo y sesión incorrecta. La entrega todavía no se declara controlador funcional si no hay actuador real.

## 6.5 S3. Adaptadores reales y bucle de acción, 1–2 semanas propuestas

Integrar captura, modelos y revalidación de ventana/foco. Implementar click, type y scroll con política local, recuento de entrega, parada y verificación del efecto. Probar pérdida de captura, inyección denegada, salida del cliente, timeout y cierre del escritorio. Eliminar cualquier ruta que responda éxito a partir de un mock.

**Salida:** integración real en la máquina de referencia, evidencia IT/SEC y ensayo guiado en aplicaciones con datos desechables. Los fallos de postcondición quedan como indeterminados, sin retries automáticos de acciones potencialmente no idempotentes.

## 6.6 S4. Aceptación y entrega, 1–2 semanas propuestas

Congelar configuraciones y corpus, ejecutar protocolo de 450 corridas E2E, regresiones adversariales, benchmarks y soak. Revisar trazas para falsos éxitos, omisiones por filtros y leaks de datos. Empaquetar binario/dependencias/modelos autorizados o instrucciones de obtención.

**Salida:** informe de aceptación firmado por el responsable designado, listado de límites, evidencias por requisito y runbook. Una previsión global razonable de planificación es del orden de 6–8 semanas, sujeta al spike y dedicación disponible; se recalcula con resultados S0.

**Alcance del MVP (15 de octubre de 2026).** El protocolo de 450 corridas pasa al próximo corte. Para este MVP vale el protocolo reducido de S4 descrito en el apartado 6.7.

## 6.7 Definition of Done del producto

**Alcance reducido del MVP (decisión del responsable, 15 de octubre de 2026; [CHANGELOG 1.0.52](CHANGELOG.md) y [ADR-0023](../adr/0023-licence-and-reduced-mvp-scope.md)).** El MVP se cierra con un alcance reducido y explícito. La decisión no cambia ningún requisito, algoritmo, contrato, escenario ni estado de las matrices:

- **(a)** RNF-05 se acepta como incumplimiento documentado para este MVP: con dos regiones, p95 de 153 ms frente a 150 ms; con una región y con el refresco completo, las metas se cumplen ([S0_RESULTADOS.md](S0_RESULTADOS.md) §10). No se marca como cumplido.
- **(b)** Para este MVP, el protocolo reducido de S4 sustituye a las 450 corridas de EV-02. Consta de E2E-01, E2E-02, E2E-03 (diez de diez en la undécima ronda, 1.0.46), S4R-02 y S4R-03, cada uno con diez de diez corridas puntuadas y cero falsos éxitos ([S4_ACEPTACION_REDUCIDA.md](S4_ACEPTACION_REDUCIDA.md)).
- **(c)** Pasan al próximo corte, con sus motivos: **EV-01**, porque el corpus de 300 capturas etiquetadas no existe, no se versiona (ADR-0012) y exige capturas autorizadas y su etiquetado ([capítulo 09 §9.3](09_EVALUACION_EXPERIMENTAL.md)); **EV-02**, porque cada ronda supervisada tiene un cupo de 200 solicitudes reales y una corrida consume entre dos y ocho, así que 450 corridas exceden el MVP (el protocolo reducido es lo que se ejecuta); **IT-07**, porque los DPI de 125 % y 150 % exigen cambiar la escala a mano con el responsable presente, y el motor aún no rechaza las escalas fuera de perfil; y **el corpus congelado de REL-01**, que depende de EV-01 y EV-02 ([S4_REPRODUCIBILIDAD.md](S4_REPRODUCIBILIDAD.md) §5 y §6).
- **(d)** El plan del próximo corte es el del paquete Paper, **INC-00 a INC-09** (`docs/02_Plan_Incremental_Critico.md` dentro de ese paquete, fuera de este repositorio). Este capítulo lo referencia y no lo copia.

El perfil del MVP es Windows 11 x64, un monitor, DPI 100 % y un tema, con los flujos del protocolo reducido ([S4_INSTALACION_Y_REVERSION.md](S4_INSTALACION_Y_REVERSION.md) §10).

**Reevaluación de cada casilla el 15 de octubre de 2026**, contra la evidencia de `origin/main` (tras la fusión de #35). Sólo se marcan las casillas cuyos criterios se cumplen con esa evidencia; cada una lleva su evidencia debajo. En la versión 1.0.53, de la misma fecha, se marca la casilla 5 y se actualizan las casillas 2, 7 y 10 con el borrado al terminar la sesión (SEC-07) y la prueba diferencial.

- [x] Captura, detector/OCR, MCP y actuador reales funcionan en el perfil acordado; no hay éxito producido sólo por fixtures.
  - *Evidencia:* en el perfil reducido la cadena completa corre en vivo: captura DXGI real ([S3_CORRIDAS_SUPERVISADAS.md](S3_CORRIDAS_SUPERVISADAS.md) §8.1), detector `icon_detect_v3` en DirectML y OCR de Windows es-ES ([S4_ACEPTACION_REDUCIDA.md](S4_ACEPTACION_REDUCIDA.md) §2), MCP por stdio (`observe_screen`, `execute_action`) y `SendInput`, con `effect_observed` verificado por el sistema. Cuatro flujos puntuados lo confirman sin falsos éxitos: E2E-01 (10 de 10, cuarta ronda), E2E-02 (10 de 10, serie de la décima ronda), S4R-02 (10 de 10, décima ronda) y S4R-03 (10 de 10, séptima ronda, continuación). Las comprobaciones son del sistema (título, carpeta, bytes de `nota.txt` y resultado de la Calculadora), no fixtures.
  - *Lo que esta casilla no cubre:* IT-01 e IT-03 siguen en parte (la rotación y la pérdida de acceso de la captura; el recibo de `SendInput` y el foco de `type` sin ejercer), e IT-07 no se ha ejecutado (DPI distinto de 100 %).
- [ ] Requisitos RF y RNF trazados a evidencia; regresiones críticas y escenarios BDD implementados pasan en el runtime.
  - *Falta:* la trazabilidad existe (DOC-01 comprueba que cada requisito tiene propietario, pruebas y evidencia), y los 39 escenarios BDD pasan en `cargo test --workspace`. Pero los 28 requisitos siguen en `PENDIENTE_IMPLEMENTACION_O_VALIDACION`, y no hay evidencia ejecutada para RNF-02 (EV-01, fuera del MVP), RNF-06 (PERF-03 sin ejecutar), RNF-07 (IT-07, fuera del MVP), RNF-05 (incumplido y aceptado) ni RF-18 (E2E-03 con 3 de 10). Trazar no es cumplir.
- [ ] Precisión y cobertura cumplen juntas; éxito global y por aplicación se mide con verificador independiente.
  - *Falta:* precisión y cobertura (RNF-02, EV-01) no se miden: pasan al próximo corte. El éxito por aplicación con verificador independiente existe en los cinco flujos del protocolo reducido (E2E-01, E2E-02, E2E-03, S4R-02 y S4R-03, cada uno 10 de 10). Los DPI de 125 % y 150 % y el tema oscuro no se han medido.
- [ ] Sesión/snapshot/revisión/epoch y policy gate bloquean todos los casos inválidos de la suite; ninguna redirección silenciosa.
  - *Falta:* las pruebas de Rust y los BDD sobre dobles cubren los casos (UT-38 a UT-43, CT-01 a CT-09, BDD-16 a BDD-37), y en vivo se vio la vigencia (`STALE_SNAPSHOT` con `input_delivered: false`). Pero SEC-05 sólo está en parte: el cambio de primer plano no se pudo provocar ([S3_CORRIDAS_SUPERVISADAS.md](S3_CORRIDAS_SUPERVISADAS.md) §3.6); IT-05 está en parte (la caída ambigua no se probó), y el lado del actuador real de BDD-28 no tiene prueba.
- [x] Diferenciador optimizado coincide con la referencia y conserva regiones pendientes tras fallos.
  - *Evidencia:* la prueba diferencial `ut_08_differential_tile_diff_matches_scalar_reference_on_generated_frames` (`src/domain/src/incremental.rs`, 1.0.53) compara `dirty_tiles`, que usa `tile_changed`, con una referencia escalar independiente (un recorrido por píxeles con coordenadas explícitas) en 16 formas de fotograma: tamaños impares y de 1 píxel, tiles de 1 a 64 píxeles y relleno de fila de 0 a 13 bytes. Cubre cada byte de los fotogramas pequeños (cada canal, el alfa, el relleno, la última fila y columna), los bytes de borde de los grandes y cambios múltiples aleatorios, con generador de semilla fija y reducción del caso fallido. Sus valores esperados son los de OR-08 a OR-10 y OR-48: el alfa y el relleno no marcan tiles, y cada píxel alterado marca el suyo. Cuatro defectos sembrados en `tile_changed` (alfa comparado, filas impares omitidas, último píxel de cada fila omitido, paso ignorado) la hacen fallar, y cada caso se reduce a un fotograma de 1×1, 1×9 o 7×5; el código actual no tiene ninguno. La conservación de regiones pendientes tras un fallo está en UT-13 a UT-15 (UT-14, `ut_14_failed_observation_keeps_base_and_pending_change`).
  - *Lo que esta casilla no cubre:* la referencia está escrita en Rust, no es el oráculo Python: los OR-04 a OR-12 no se ejecutan contra Rust, y un OR aprobado no marca su UT como pasado (AGENTS.md §5, regla 11). Una versión SIMD futura debe pasar la misma prueba (capítulo 05 §5.2).
- [ ] Rendimiento medido en release con hardware y pesos fijados; se publican p50/p95/p99, cold/warm y fallos.
  - *Falta:* el núcleo sí cumple la medición: PERF-01 publica p50/p95/p99, en frío y en caliente, en release y con el hardware de S0 ([S0_RESULTADOS.md](S0_RESULTADOS.md) §11). La percepción no: PERF-02 sigue en parcial. §10 publica medianas y p95 (refresco completo en caliente 98 ms; una región 102 ms; dos regiones 153 ms), sin p99; y la matriz pide además las variantes de caché y la igualdad de modelos, que §10 no mide.
- [ ] Límite de memoria/caché/colas y soak completados; ninguna fuga observada se convierte en afirmación universal de ausencia de fugas.
  - *Falta:* PERF-03 (RSS, VRAM, colas y soak de 2 h, RNF-06) no se ha ejecutado. `tools/perf/soak.py` está listo desde 1.0.47 y sin ejecutar (sesión B de [S4_SESIONES_PENDIENTES.md](S4_SESIONES_PENDIENTES.md)). RNF-06 todavía no fija umbral, y el responsable debe fijarlo antes de medir.
- [ ] Parada, permisos, denegación por defecto, redacción y borrado de datos funcionan.
  - *Falta:* SEC-02 sólo en parte (la parada se ejerció en vivo; no se probó en todas las etapas de una secuencia). El borrado existe desde 1.0.53: al terminar la sesión MCP el motor borra su snapshot, evidencia, cuadro base, regiones pendientes y diario de `request_id` (SEC-07, BDD-39; capítulo 08 §8.4), sin tocar la configuración ni los archivos del operador. Faltan el borrado bajo demanda del operador, sin terminar la sesión, y que el cierre forzado borre algo: el proceso termina sin ejecutar el borrado. Los permisos y la denegación por defecto se ejercieron en vivo (SEC-01 ejecutada), y la redacción está probada en Rust sobre dobles (IT-06, SEC-03, SEC-04).
- [x] Lockfiles, hashes, licencias, instalación, rollback y límites de compatibilidad están documentados.
  - *Evidencia:* lockfiles (`Cargo.lock` de la raíz, `tools/s4/testapp/Cargo.lock` y `tools/s4/testeditor/Cargo.lock`) y su comprobación ([S4_REPRODUCIBILIDAD.md](S4_REPRODUCIBILIDAD.md) §1); hashes de modelos, runtimes y configuraciones ([S0_RESULTADOS.md](S0_RESULTADOS.md) §2; [S4_REPRODUCIBILIDAD.md](S4_REPRODUCIBILIDAD.md) §2); licencias del código y de los documentos (`LICENSE-MIT`, `LICENSE-APACHE` y `LICENSE-DOCS`, decididas el 15 de octubre de 2026) y de los componentes de terceros que no se versionan ([S4_REPRODUCIBILIDAD.md](S4_REPRODUCIBILIDAD.md) §3); instalación, verificación, conexión de un cliente MCP, reversión y límites de compatibilidad ([S4_INSTALACION_Y_REVERSION.md](S4_INSTALACION_Y_REVERSION.md) §§2 a 10), con las pruebas `rel_01_the_install_guide_points_at_files_that_exist` y `tools/repro/test_install_guide.py`.
  - *No verificado (la casilla pide documentación):* nadie ha seguido la guía en una máquina limpia (sesión A). Esa ejecución es la evidencia de REL-01, que sigue `PENDIENTE_PRODUCTO`.
- [ ] Todo incumplimiento está resuelto o aceptado explícitamente mediante cambio de alcance; no se cambia un checkbox sin evidencia.
  - *Aceptado por cambio de alcance (15 de octubre de 2026):* RNF-05 (a); EV-01, EV-02, IT-07 y el corpus de REL-01 (c).
  - *No resuelto ni aceptado, y por tanto bloquea esta casilla:* PERF-02 (p99 y variantes de caché de la percepción); PERF-03 (soak); el borrado bajo demanda del operador (§8.4); SEC-02 y SEC-05 en parte; IT-01 a IT-05 en parte; y la instalación en una máquina limpia. Sólo el responsable puede aceptar alguno de ellos por cambio de alcance, y no se ha hecho. Ya resueltos en 1.0.53 y fuera de esta lista: el borrado al terminar la sesión (SEC-07) y la prueba diferencial del diferenciador (casilla 5).

Los párrafos siguientes registran el estado del 14 y el 15 de octubre (versiones 1.0.35 y 1.0.47). Se conservan como historia; la reevaluación de arriba es la que vale.

**Evidencia de aplicaciones a 14 de octubre de 2026 (ninguna casilla cambia).** La casilla «éxito global y por aplicación con verificador independiente» tiene hoy evidencia puntuada en dos aplicaciones, ambas del protocolo reducido de S4: la aplicación de prueba propia (E2E-01, parcial) y el Bloc de notas (S4R-03, diez de diez corridas, sólo clics). No la tiene la Calculadora (S4R-02, sin ejecutar), ni el editor con escritura y guardado (E2E-02), ni el explorador de archivos (E2E-03); tampoco hay corridas a 125 y 150 % de DPI, con otro tema ni las 450 del diseño. Por eso la casilla sigue sin marcar.

**Evidencia sin entrada real ni pantalla a 14 de octubre de 2026 (ninguna casilla cambia).** La versión 1.0.35 añade pruebas de Rust para IT-06, SEC-03, SEC-04, SEC-06, PERF-01 y DOC-01, y las comprobaciones automáticas de REL-01. Casilla por casilla: *«Requisitos RF y RNF trazados a evidencia…»*: cada requisito está ligado a pruebas, evidencia y ADR que existen (DOC-01), pero para la mayoría la evidencia es «sin ejecutar» o parcial, y los requisitos siguen `PENDIENTE_IMPLEMENTACION_O_VALIDACION`; trazar no es cumplir. *«Rendimiento medido en release con hardware y pesos fijados…»*: PERF-01 publica p50/p95/p99 en frío y en caliente del núcleo en la máquina de referencia (`S0_RESULTADOS.md` §11), pero la casilla pide también los pesos y la percepción completa (PERF-02 sigue parcial; RNF-05 no se cumple). *«Parada, permisos, denegación por defecto, redacción y borrado de datos funcionan»*: la redacción y la denegación por defecto pasan sobre dobles; el borrado de datos no tiene prueba. *«Lockfiles, hashes, licencias, instalación, rollback y límites de compatibilidad están documentados»*: lockfiles, toolchain, hashes y licencias sí (`S4_REPRODUCIBILIDAD.md`); no había guía de instalación ni de rollback (desde la versión 1.0.47 hay una, sin ejecutar: [`S4_INSTALACION_Y_REVERSION.md`](S4_INSTALACION_Y_REVERSION.md)). Por eso ninguna casilla se marca.

**Preparación de la evidencia que exige la máquina del responsable (15 de octubre de 2026, versión 1.0.47; ninguna casilla cambia).** Para que REL-01 (instalación y reversión), PERF-03 (recursos y soak de 2 h, RNF-06) e IT-07 (DPI 100/125/150 %, RNF-07) se puedan producir cada una en una sola sesión supervisada, el repositorio incluye ahora la guía [`S4_INSTALACION_Y_REVERSION.md`](S4_INSTALACION_Y_REVERSION.md), las herramientas de sólo lectura `tools/perf/soak.py` y `tools/perf/dpi_probe.py` (con pruebas sobre dobles) y el documento [`S4_SESIONES_PENDIENTES.md`](S4_SESIONES_PENDIENTES.md) con lo que cada sesión necesita del responsable y lo que registra. No se ejecutó ninguna sesión y no hay ninguna cifra nueva: las tres filas de la matriz (`PENDIENTE_PRODUCTO`) y las casillas siguen como estaban.

## 6.8 Roadmap condicionado por evidencia

**v1.1, accesibilidad híbrida:** fusionar UIAutomation y luego AX/AT-SPI mediante los puertos previstos. Comparar ahorro de inferencias con precisión y cobertura. No prometer “70% menos GPU” sin medir. El árbol accesible puede ser incompleto; sigue existiendo la ruta visual.

**v1.2, iconos y privacidad:** banco revisado de prototipos, versiones de contexto y fallback semántico opcional. Persistencia cifrada o local restringida y evaluación de open-set. No hay aprendizaje confiable automático a partir de cualquier caption.

**v1.3, rendimiento:** shapes/batching según coste, buffers GPU, SIMD verificado, streaming de cambios y memoria incremental. Cada optimización debe mantener propiedades del core. La reducción del área procesada no se usa como sustituto del tiempo medido.

**v2.0, expansión de dominio:** macOS/Linux, multimonitor, RTL, más aplicaciones y modelos. Incorporar retrieval léxico/semántico y delta snapshots sólo con paginación/base/expiración claros; medir tokens con tokenizador real y cobertura de targets.

**v3.0, flujos locales:** grafo de estados y macros de acciones con precondiciones y verificación en cada paso. No ejecutar especulativamente acciones con consecuencias. El objetivo es reducir round-trips, no garantizar cien milisegundos para tareas cuya aplicación tarda más.

## 6.9 Gestión de cambios y decisiones abiertas

Cada cambio importante registra contexto, alternativas, consecuencia y criterio de reversión en un ADR. Las doce ADR, entre ellas la plataforma Windows inicial ([ADR-0008](../adr/0008-one-real-platform-first.md)), quedaron aprobadas el 14 de octubre de 2026. Quedan pendientes de aprobación: cliente MCP específico, aplicaciones de prueba, pesos, hardware de evaluación, corpus y políticas de uso. Las licencias las decidió el responsable el 15 de octubre de 2026: código `MIT OR Apache-2.0` y documentos CC BY 4.0 ([ADR-0023](../adr/0023-licence-and-reduced-mvp-scope.md)). El alcance reducido del MVP de esa misma fecha está en el apartado 6.7 y en ese ADR. Los documentos contienen una propuesta coherente para poder empezar S0, no una atribución de aprobación al usuario.
