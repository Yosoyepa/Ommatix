# 09. Protocolo experimental y benchmark reproducible

## 9.1 Objetivo y unidad de comparación

El experimento evalúa incrementalidad y grounding, no una competición de marcas de LLM. La unidad primaria es una secuencia de UI con ground truth y cambios temporales. Se comparan pipelines sobre los mismos frames, modelos, hardware y políticas. La evaluación E2E agrega un planificador fijo y debe reportarse aparte.

## 9.2 Baselines y ablaciones

**B0:** full-frame con el mismo detector y OCR, sin reutilización perceptiva. **B1:** incremental con halo y commit, manteniendo fusión e identidad iguales a B0. **B2:** B1 con caché perceptual contextual. **B3:** sistema completo con tracking/precondiciones. **B4 opcional:** parser externo configurado exactamente y con costes de captioning declarados. **B5 opcional:** accesibilidad-only en aplicaciones que la expongan.

Para atribuir mejoras se modifica un componente a la vez. Las versiones defectuosas de FNV/dHash sirven como contraejemplos de corrección, no como baseline competitivo artificialmente debilitado. Un modo sin gates puede medirse sólo offline, sin inyectar entrada peligrosa.

B0 se implementa desde 1.0.54 como interruptor experimental, `[engine] experimental_full_frame`, apagado por defecto: el planificador de Algoritmo 1 calcula su diferencia de teselas como siempre y luego fuerza el refresco completo en cada observación, con el mismo detector, OCR, accesibilidad, pesos, precisión, concurrencia, política y estado inicial que B1. Es una línea de base experimental y nunca un modo de producción (ADR-0024). El protocolo de comparación está en el §9.9.

## 9.3 Diseño del corpus

Estratos: cambios pequeños, menú, scroll, animación extensa, notificación, sustitución de contenido, iconos repetidos/desconocidos, formularios densos, targets pequeños, escalas DPI y temas. Etiquetar área sucia antes/después del halo, identidad de objeto y accionabilidad. Separar familias de pantallas entre calibración y test y evitar frames vecinos en ambos conjuntos.

El corpus está **por construir**. Desde el 15 de octubre de 2026 pasa al próximo corte del MVP (capítulo 06 §6.7). Se entregan plantillas, no capturas supuestamente etiquetadas. Para el piloto se propone comenzar con 30 secuencias y dos anotadores; ampliar según diversidad y precisión de intervalos. Los conjuntos públicos [S15–S16] orientan evaluación, pero no se declara que se hayan descargado ni ejecutado.

## 9.4 Instrumentación

Guardar por muestra: run_id, sequence_id, frame_id, semilla, commit, hash de configuración/modelos/corpus, versión de SO/driver/EP, temperatura o condiciones térmicas cuando sea posible, dimensiones/DPI y modo cold/warm. Tiempos con reloj monotónico, unidades explícitas y sincronización del fin de inferencia.

Spans: capture_wait, capture_copy, dirty_plan, preprocess, detector, OCR, icon_match, fusion, tracking, commit, serialization, action_validate, input_delivery y effect_wait. Si detector/OCR se solapan, no sumar sus duraciones y llamarlo wall-clock. Medir el total directamente. La suma de p95 de etapas tampoco es p95 del pipeline.

Desde 1.0.54 los tramos de una observación son los de `_meta` (ADR-0024): `capture` (espera y copia de la captura), `plan`, `detect`, `recognize` y `accessibility` (concurrentes, sin sumar), `fuse`, `track`, `commit` (publicación), `response_build` (construcción de la primera página, después de publicar) y `total`, que va de la captura al snapshot publicado. La serialización y el transporte no caben en el mensaje que los contiene: se miden en el cliente como ida y vuelta. Las medidas anteriores a 1.0.54 tienen `total` hasta el fin del *tracking* y no se comparan con las nuevas.

Medir RSS total y VRAM asignada/reservada, no excluir pesos para parecer ligero. Registrar CPU/GPU utilization con método y frecuencia definidos. El coste de serialización y el tamaño final en bytes se miden sobre mensajes realmente emitidos; los tokens se computan con el tokenizador del cliente y su representación efectiva.

## 9.5 Métricas de calidad

Reportar precisión/recall de detección con definición del umbral, calidad de label y rol, precisión de accionabilidad, tasa de asociaciones de identidad incorrectas y estabilidad bajo ausencia de cambio semántico. El grounding se evalúa por target correcto y región válida, no sólo por distancia al centro.

Para decisiones con abstención se reportan precisión aceptada y cobertura juntas. Un sistema que rechaza todo no aprueba. La estabilidad de IDs también tiene que detectar sustituciones: conservar un ID para una fila que ahora representa otro archivo es un error, no un éxito de estabilidad.

Las tareas se comprueban con postcondiciones externas: contenido del archivo temporal, valor del formulario o carpeta mostrada. Se documenta cuánto depende el verificador de acceso privilegiado y se evita usar la propia salida del motor como verdad.

## 9.6 Procedimiento de medición

Congelar test y configuración antes de medir. Efectuar warm-up declarado, reinicios para cold-start y pruebas separadas de caché caliente/fría. Alternar aleatoriamente el orden B0/B1/B2/B3 por secuencia, conservando semillas. Recoger al menos 10.000 muestras temporales para caracterizar colas, distribuidas entre secuencias; aumentar si p99 es inestable.

La repetición masiva de una sola pantalla no demuestra rendimiento general. Los frames de una misma secuencia no son independientes: usar bootstrap por secuencia para intervalos de latencia y métodos apropiados para tasas agrupadas. Publicar número de secuencias, frames, tareas, fallos, censuras y exclusiones con justificación previa.

## 9.7 Análisis y reglas de conclusión

Comparar razones/diferencias emparejadas de latencia, con intervalos del 95%, y verificar no inferioridad de precisión/cobertura según el plan de tesis. Reportar estratos además del agregado. Conservar outliers y timeouts; no publicar sólo p50 de respuestas exitosas.

Para eventos de seguridad con cero fallos observados, reportar el denominador y un límite de incertidumbre. Como aproximación bajo ensayos independientes, cero eventos en n pruebas da un límite superior cercano a 3/n al 95%; si existe dependencia temporal, no aplicar esa regla sin ajuste [D, aproximación binomial]. Cero en fixtures no demuestra inmunidad en uso abierto.

Una hipótesis se considera no sustentada si su intervalo o cobertura no cumple el criterio. No se reemplazan resultados negativos por cambios retrospectivos de umbral. Las conclusiones delimitan hardware, versiones, aplicaciones y modos evaluados.

**Umbral de RNF-06 (15 de octubre de 2026, versión 1.0.62).** Para el soak de 2 horas de RNF-06, el responsable fija dos criterios, medidos después de los primeros 10 minutos sobre el proceso del motor con la pendiente por hora de mínimos cuadrados de `tools/perf/soak.py`: el conjunto de trabajo no crece más de **10 MiB por hora**, y el recuento de manejadores no crece (su pendiente no es positiva). Son los valores por defecto de `--max-rss-growth-mib-per-hour` y `--max-handle-growth-per-hour`, y la herramienta publica cada valor medido y si cumple. El criterio de manejadores es estricto a propósito: cualquier pendiente positiva lo incumple; si el responsable quiere tolerancia, la fija antes de la siguiente corrida. La cota de la cola de regiones pendientes no está fijada y no tiene valor por defecto. Con observaciones correctas la cola vale cero por construcción ([S0 §12](S0_RESULTADOS.md)), así que una corrida en la que ninguna observación falla no la pone a prueba. Cumplir los criterios no demuestra ausencia de fugas: sólo una pendiente en esas condiciones.

Ese umbral se fijó **después** de leer la pendiente de la corrida de [S0 §12](S0_RESULTADOS.md) (+1,6 MiB/h, publicada en 1.0.55). Por eso el veredicto sobre esa corrida no es una prueba preregistrada. La repetición sobre el código actual con `[models] accessibility = "uia"` se juzga con estos mismos valores, fijados antes de leerla.

## 9.8 Tablas de resultados previstas

Las plantillas CSV contienen encabezados para resultados por ejecución, métricas de calidad y resumen por estrato. No contienen mediciones ficticias. La tabla productiva deberá mostrar baseline, modo, N, p50/p95/p99, precisión, cobertura, éxito, RSS y VRAM. El informe del oráculo se mantiene fuera de esas tablas para que sus milisegundos de unittest no se confundan con percepción real.

## 9.9 Protocolo B0/B1: incrementalidad aislada (INC-02)

**Pregunta.** Con la misma secuencia, el mismo binario, los mismos pesos, la misma precisión, la misma concurrencia, la misma política y el mismo estado inicial, ¿cuánto reduce B1 (incremental) el costo de observación frente a B0 (refresco completo)? Y ¿en qué medida coinciden las detecciones de ambas? Esa coincidencia es un acuerdo entre dos variantes, no precisión frente a anotaciones (§9.5).

**Variables fijas.** Un solo ejecutable (`ommatix.exe`) para B0 y B1; pesos y manifiestos de modelo; una configuración por variante que sólo difiere en `[engine] experimental_full_frame` (`true` en B0, `false` en B1); la aplicación de prueba `tools/s4/testapp` con sus banderas ocultas `--animate`, `--animate-labels` y `--animate-flash`, sin entrada de ningún tipo; política y allowlist iguales. La herramienta rechaza cualquier otra diferencia entre variantes: la única declaración permitida es `--allow-difference engine.experimental_full_frame`.

**Congelación.** Antes de medir, el commit y el árbol limpio (`tools/repro/environment.py`), el SHA-256 del ejecutable, de cada configuración y de la aplicación de prueba. El manifiesto del informe los registra. Cualquier cambio posterior exige otro `--experiment-id`.

**Orden y apareamiento.** Por caso (`full`, `incr1`, `incr2`) y por índice de proceso, B0 y B1 se ejecutan intercalados contra la misma instancia de la aplicación de prueba, con orden rotatorio. La unidad de apareamiento es el proceso (`run_id`, `sequence_id`): los dos procesos de un mismo índice comparten `sequence_id`. Los fotogramas de dos procesos apareados no son idénticos, porque es captura en vivo; el apareamiento es por secuencia y por instante de ejecución, y así se declara.

**Comando exacto** (desde la raíz del repositorio, con `CONFIG_B0.toml` y `CONFIG_B1.toml` del operador):

```text
python tools/perf/measure_latency.py --experiment-id EXPERIMENT ^
    --variant B0=target\release\ommatix.exe,CONFIG_B0.toml ^
    --variant B1=target\release\ommatix.exe,CONFIG_B1.toml ^
    --allow-difference engine.experimental_full_frame --paired B0:B1 --out chunk-N.json
```

Para unir los trozos: `python tools/perf/measure_latency.py --combine chunk-1.json chunk-2.json --paired B0:B1 --out all.json`. Cada trozo dura unos minutos, porque la aplicación de prueba necesita el primer plano durante toda la corrida. Ni el comando ni la herramienta envían entrada: la aplicación de prueba se anima sola, y `ommatix demo` sólo observa y ejecuta una acción en seco.

**Registro mínimo por observación.** Cada observación se escribe como una línea `ommatix-observation` con un objeto JSON de esquema `ommatix.observation/1`: los tiempos de cada tramo en microsegundos enteros, el plan (`work`, `full_refresh_reason`, `region_count`, `region_area_px`, `inference_count`, `input_shapes`, `accessibility_truncated` y los recuentos de evidencia) y una firma de 16 dígitos hexadecimales por elemento (FNV-1a de rol, etiqueta y accionabilidad; ninguna etiqueta se escribe). La herramienta añade a cada muestra `experiment_id`, `run_id`, `process_id`, `sequence_id`, `observation_id`, `variant` y `case`, sin aplanar. El registro mínimo del plan del paper se cubre así:

| Campo del registro mínimo | En esta versión |
|---|---|
| `schema_version` | `schema` del registro (`ommatix.observation/1`) y del informe (`ommatix.perf/2`) |
| `experiment_id`, `run_id`, `process_id`, `sequence_id`, `observation_id`, `variant` | Sí, en cada muestra |
| `seed` | No hay semilla aleatoria: el orden de variantes y el índice de proceso son el orden de la corrida; la semilla del bootstrap es 20261015 |
| `code_sha` | `code_commit` del manifiesto |
| `build_hash`, `model_hash`, `config_hash`, `corpus_hash` | `exe_sha256` y `config_sha256` por variante; `test_app_sha256` hace de corpus. El hash del manifiesto del modelo no se registra: va dentro del hash de la configuración |
| `work_kind`, `full_refresh_reason`, `region_count`, `region_area_px` | `plan.work`, `plan.full_refresh_reason`, `plan.region_count`, `plan.region_area_px` |
| `input_shapes`, `inference_count` | `plan.input_shapes`, `plan.inference_count`; `null` cuando el detector no cuenta sus inferencias (no medido, nunca cero) |
| `geometry_epoch`, `controls_n`, `words_n`, `supports_n`, `relations_n` | Sí |
| `uia_truncated` | `plan.accessibility_truncated` |
| `index_build_ns`, `index_query_ns`, `exact_filter_ns`, `order_ns`, `candidates_n`, `exact_predicates_n`, `closure_rounds` | No existen en este corte (no hay índice ni A1): `null` |
| `observe_wall_ns`, `commit_ns`, `response_build_ns` | `timings_us.total`, `timings_us.commit`, `timings_us.response_build`, en microsegundos enteros (la unidad de `_meta`; el paper pide nanosegundos y esta diferencia se declara aquí) |
| `outcome`, `exclusion_reason` | `outcome`, y `error_code` para las observaciones con error, que se conservan |
| `paired_key` | (`run_id`, `sequence_id`, `observation_id`) |

**Análisis.** Cada observación cae en una clase decidida por su registro, nunca por un tiempo redondeado: `first` (la observación 1), `second` (la 2, que no entra en las estadísticas de latencia), `warm_full` y `warm_regions` (las observaciones 3 y siguientes que sí analizaron), `idle` (las que no tenían nada que analizar: se cuentan, no se miden) y `error` (se conservan con su código y no entran en los tiempos). Las medianas y los p95 se dan en milisegundos por clase, con su n. Ninguna etapa concurrente se suma a otra ni se suman p95.

La diferencia apareada se calcula por proceso: la mediana de `total` de las observaciones cálidas de B1 menos la de B0 (Δ = B1 − B0; un Δ negativo significa que B1 es más rápida), y su razón. Los intervalos del 95 % son intervalos percentiles de bootstrap sobre procesos, con 10 000 remuestreos y semilla 20261015; nunca se remuestrean fotogramas. Un proceso sin observaciones cálidas en una de las dos variantes se excluye y se cuenta.

El acuerdo se calcula por observación apareada (mismo proceso y mismo número): el coeficiente de Dice de los multiconjuntos de firmas de elementos, que vale 1 cuando las dos detecciones coinciden y 0 cuando no comparten ningún elemento. Si los dos conjuntos están vacíos, la observación no se mide. Se informan la media por proceso con su intervalo bootstrap y la proporción de observaciones con multiconjuntos idénticos. Se llama acuerdo (`agreement_dice`), no precisión.

**Límites.** (1) Es un experimento en vivo: la animación avanza con el tiempo, así que dos procesos apareados ven pantallas parecidas, no idénticas, y el acuerdo depende de ese ritmo. (2) Que B0 y B1 coincidan no prueba que ninguna sea correcta (§9.5). (3) La UIA de cada proceso es viva, no un replay. (4) Las observaciones inactivas de B1 no entran en la comparación de latencia; se reportan aparte. (5) Esta sección describe el protocolo; sus resultados de la primera ejecución están en el apartado 9.9.1, que no altera el protocolo. La medición la programa el coordinador, con el protocolo congelado y el cupo del responsable.

### 9.9.1 Resultados de la primera ejecución (1.0.56)

Ejecutada el 15 de octubre de 2026 con el protocolo de esta sección, sin cambios: identificador `inc02-b0b1-20261015`, 40 procesos por variante y caso, 20 observaciones por proceso, aplicación de prueba sin entrada. Las desviaciones declaradas (B1 con la clave explícita en `false`, rutas absolutas en ambas copias) están en [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §13.1, con las condiciones, las cifras completas y los límites.

- **Una región (`incr1`):** B1 es más rápida. Δ = −18,6 ms [IC 95 %: −18,9; −18,2] (razón 0,79). El ahorro es del detector.
- **Dos regiones (`incr2`):** B1 es más lenta. Δ = +45,8 ms [IC 95 %: +45,4; +46,1] (razón 1,51): cada región es una inferencia completa de 1280×1280.
- **Refresco completo (`full`, control):** la detección y el OCR cuestan lo mismo en ambas variantes. La diferencia de −9,9 ms está en la captura y en la planificación; no es una ventaja de B1, y el control es débil porque B1 sólo analizó 66 observaciones.
- **Acuerdo (coeficiente de Dice; acuerdo, no precisión):** 0,97 con una región y en refresco completo; 0,85 con dos regiones. Con dos regiones, B1 publica una mediana de 67 elementos frente a 85 de B0; el registro no dice cuáles faltan.
- **Observaciones inactivas de B1 (aparte):** 52 en `incr1`, 0 en `incr2` y 654 en `full`.
- **Errores y exclusiones:** ninguno. Los 240 procesos terminaron completos.
- **PERF-01 sin carga (§13.7):** el desplazamiento de p95 de 0,2 a 0,35 ms no se reproduce; persiste una mediana de 0,05 a 0,09 ms mayor, sin atribuir.

La configuración de origen no tiene `[models] accessibility = "uia"`, así que estas cifras son sin accesibilidad; la repetición con `uia` y la investigación de los elementos que faltan están en el apartado siguiente. RNF-05 no cambia de estado. PERF-02 no cambia en `plan_pruebas.csv`: sigue faltando el caso de caché y la igualdad de modelos.

### 9.9.2 Seguimiento: pérdida de elementos y repetición con `uia` (1.0.57)

Sin cambiar el protocolo. Detalle, condiciones y límites en [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §13.10 a §13.13.

- **Elementos que faltan con dos regiones (`incr2`), sin accesibilidad.** No es un defecto del Algoritmo 1: en las observaciones 2 a 8 de un compilado de diagnóstico, ninguna de las 18 a 68 cajas de la base que solapan una región queda cortada por ella (el cierre del halo las contiene todas). La diferencia es lo que el detector devuelve sobre el recorte: unos 20 de 75 controles de B0 no tienen pareja en B1 (IoU ≥ 0,5), de ellos 15 a 17 en las dos regiones (14 iconos de la bandeja del sistema), y los dos recortes juntos devuelven 3 o 4 controles, tres de ellos una caja del tamaño del recorte. Sobre un mismo fotograma, un recorte de 1280×720 reproduce el 67 a 72 % de lo que ve el fotograma completo y uno de 224×151 sólo 4 de 11: el recorte entra a escala 1,0 y el fotograma completo se reduce a 2/3. Es un coste de calidad de la incrementalidad con este modelo; el algoritmo no cambia.
- **Repetición con `[models] accessibility = "uia"`** (`inc02-b0b1-uia-20261015`, 40 procesos por variante y caso, sin entrada). Una región: Δ = +2,4 ms [IC 95 %: +0,8; +4,1] (B1 deja de ser más rápida: la lectura de UIA, ≈ 117 ms, fija el total en las dos variantes). Dos regiones: Δ = +46,6 ms [+44,6; +48,7]; B1 mediana 166,2 ms y p95 186,3 ms [185,6; 186,9]. Refresco completo (control): Δ = −5,4 ms [−7,3; −3,6], en la captura y la planificación. Acuerdo (Dice): 0,97, 0,83 y 0,98. Con dos regiones B1 publica una mediana de 62 elementos frente a 80.
- **RNF-05: sin cambio de estado.** Con `uia` y la animación de 30 ms del protocolo el p95 de dos regiones no llega a 150 ms; lo decide el responsable.
- **`region_area_px` de las observaciones inactivas** se escribía como `-0.0`; corregido con prueba de regresión (§13.12).
