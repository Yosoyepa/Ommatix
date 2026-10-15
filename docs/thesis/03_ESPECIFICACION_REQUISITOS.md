# 03. Requisitos, alcance y aceptación

## 3.1 Regla de lectura

Los criterios siguientes son propuestos para el MVP, no resultados medidos. La matriz CSV conserva sus identificadores y relación con pruebas. Antes de iniciar S1, el responsable del proyecto debe ratificar plataforma, corpus, modelos y perfil de hardware en S0. Cambiar metas requiere un ADR y no puede hacerse después de mirar el test para ocultar un fallo.

## 3.2 Requisitos funcionales

### RF-01. Captura real

Capturar pixels físicos, stride, formato y tiempo; detectar cambio de epoch. Trazabilidad: IT-01. Hito: S0/S3.

### RF-02. Diferenciación exacta

Detectar todos los cambios RGB en fixtures, incluidos bordes y filas impares. Trazabilidad: UT-01..UT-08. Hito: S1.

### RF-03. Plan de regiones

Cierre halo transitivo, fusión de solapes y fallback completo acotado. Trazabilidad: UT-09..UT-12. Hito: S1.

### RF-04. Transacción regional

Fallo/cancelación no confirma frame base ni borra evidencia publicada. Trazabilidad: UT-13..UT-15. Hito: S1.

### RF-05. Percepción real

Detector y OCR ejecutados con modelos/versiones/transformaciones registrados. Trazabilidad: IT-02. Hito: S0/S3.

### RF-06. Fusión

Propiedad específica de texto, orden estable y asociación conservadora. Trazabilidad: UT-16..UT-23. Hito: S1.

### RF-07. Texto y affordance

No tratar huérfanos o placeholders como acciones/valores confirmados. Trazabilidad: UT-24..UT-26. Hito: S1.

### RF-08. Iconos

Soporte, rechazo y contexto; nunca autoetiquetar desconocidos como certeza. Trazabilidad: UT-27..UT-31. Hito: S2.

### RF-09. Identidad

IDs de sesión sin colisión/reutilización; tracking uno-a-uno y revisión. Trazabilidad: UT-32..UT-37. Hito: S2.

### RF-10. Observación

Snapshot completo versionado; orden estable, paginación, límites y expiración explícitos. Trazabilidad: CT-01..CT-04. Hito: S2.

### RF-11. Acción segura

Validar sesión/snapshot/revisión, política, ventana/foco y frescura. Vigencia (ADR-0025): la captura del snapshot y la lectura de accesibilidad que confirma el objetivo no pueden tener más de `[engine] max_snapshot_age_ms` (10 000 ms por defecto), medidos en el reloj inyectado; la comprobación se hace en la puerta, también en simulación, y justo antes de cada entrada real, sin reutilizar lecturas antiguas al publicar un snapshot nuevo. Vencido el plazo, `STALE_SNAPSHOT` y observar de nuevo, sin reanclar ni enviar entrada. Trazabilidad: UT-38..UT-43 y UT-46..UT-54; BDD-38. Hito: S2/S3.

### RF-12. Actuación real

Click/type/scroll con permiso; reportar eventos entregados sin falso éxito. Trazabilidad: IT-03. Hito: S3.

### RF-13. Postcondiciones

Separar entrega, cambio y efecto; timeout o incertidumbre sin retry ciego. Trazabilidad: IT-04. Hito: S3.

### RF-14. Deduplicación

request_id repetido no duplica input; conflicto y caída exigen reconciliación. Trazabilidad: IT-05. Hito: S3.

### RF-15. MCP

Perfil moderno y compatibilidad legacy sólo si se prueba explícitamente. Trazabilidad: CT-05..CT-09. Hito: S2.

### RF-16. Parada y privacidad

Actuación deshabilitada al inicio; parada local y telemetría redactada; estado y caché borrados al terminar la sesión. Trazabilidad: SEC-01..SEC-07. Hito: S3.

### RF-17. Observabilidad

Métricas por etapa y motivos de rechazo; ninguna captura/texto crudo por defecto. Trazabilidad: IT-06. Hito: S3.

### RF-18. Demo funcional

Ejecutar flujos reales verificables en 3 aplicaciones permitidas. Trazabilidad: E2E-01..E2E-03 y, para el protocolo reducido de S4, S4R-02 y S4R-03 (no sustituyen a E2E-02 ni a E2E-03: el editor con escritura y guardado y el explorador de archivos siguen pendientes). Hito: S4.

**Evidencia puntuada a 14 de octubre de 2026 (RF-18 sigue sin cumplirse).** Hay corridas puntuadas, con verificador independiente del motor y sin falsos éxitos, en dos aplicaciones: la aplicación de prueba propia (E2E-01, parcial: una tarea, un DPI y un tema) y el Bloc de notas (S4R-03: abrir y cerrar «Editar», diez de diez corridas, sólo clics). Falta la Calculadora (S4R-02: no se ha ejecutado, porque el cambio al modo Estándar no se había logrado), falta el editor con escritura y guardado de un archivo temporal (E2E-02) y el explorador de archivos (E2E-03), y faltan los DPI, los temas y las 450 corridas del diseño.

**Alcance del MVP (15 de octubre de 2026).** Para este MVP, la evidencia de RF-18 es el protocolo reducido de S4: E2E-01, E2E-02, E2E-03, S4R-02 y S4R-03, cada uno con diez de diez corridas puntuadas y cero falsos éxitos. En `main`, E2E-01 (cuarta ronda, con la parcialidad que registra la matriz: una tarea, un DPI y un tema), E2E-02 (serie de la décima ronda), S4R-02 (décima ronda) y S4R-03 (séptima ronda, continuación) cumplen ese criterio. E2E-03 tiene tres de diez (undécima ronda, continuación 2) y su serie sigue. RF-18 no se marca hasta que E2E-03 llegue a diez de diez. S4R-02 y S4R-03 no sustituyen a E2E-02 ni a E2E-03, que son flujos distintos y siguen en el protocolo. Lo que el protocolo reducido sustituye, para este MVP, son las 450 corridas de EV-02 (capítulo 06 §6.7).

## 3.3 Requisitos no funcionales

### RNF-01. Corrección del core

100% de regresiones críticas aprobadas; cero referencia obsoleta ejecutada en suite. Evidencia requerida: UT/SEC.

### RNF-02. Grounding útil

Precisión aceptada >=98% y cobertura >=85% en corpus de aceptación. Evidencia requerida: EV-01.

**Alcance del MVP (15 de octubre de 2026).** EV-01 pasa al próximo corte: el corpus de 300 capturas etiquetadas no existe (capítulo 09 §9.3) y no se versiona (ADR-0012). Sin él no hay precisión ni cobertura medidas, y este requisito no se cumple en el MVP.

### RNF-03. Éxito de tareas

Éxito global de al menos 90% y por aplicación de al menos 85% en el protocolo definido. Evidencia requerida: EV-02.

**Alcance del MVP (15 de octubre de 2026).** Para este MVP, la evidencia de éxito de tareas es el protocolo reducido de S4 (RF-18): diez de diez corridas con cero falsos éxitos en cada uno de sus cinco flujos. Las 450 corridas de EV-02 pasan al próximo corte, y con ellas el criterio numérico de este requisito (al menos 90 % global y 85 % por aplicación). Este requisito no se marca como cumplido.

### RNF-04. Latencia core

p95 <=10 ms para 300 controles y 600 spans; sólo core, hardware fijado S0. Evidencia requerida: PERF-01.

**Evidencia a 14 de octubre de 2026 (PERF-01, 1.0.35).** Medido en `--release` en la máquina de S0 sobre una escena sintética de 300 controles y 600 fragmentos de texto, con 10 000 muestras por situación: el p95 del núcleo (diferencia de tiles, fusión, identidad y *commit*) es de **1,81 ms** en la primera observación, **3,67 ms** en caliente sin cambios y **3,97 ms** en caliente con una región, y el de `Engine::observe_screen` entero, de 4,61 ms como máximo; el criterio (≤ 10 ms) se cumple en esa medición. Antes de corregir la asociación de identidad (que normalizaba cada etiqueta una vez por par de pista y observación) el p95 en caliente era de 75 a 79 ms. La medición no incluye detector, OCR, accesibilidad, captura ni transporte, la escena es sintética y el estado del requisito sigue pendiente hasta el informe de aceptación (capítulo 06 §6.6). Detalle, defecto y método en [`S0_RESULTADOS.md`](S0_RESULTADOS.md) §11.

### RNF-05. Latencia percepción

Metas iniciales: incremental p95 <=150 ms; full warm p95 <=700 ms. Evidencia requerida: PERF-02.

**Decisión de alcance del MVP (15 de octubre de 2026): incumplimiento aceptado y documentado, no cumplido.** Con las etapas concurrentes y la red en float16 ([`S0_RESULTADOS.md`](S0_RESULTADOS.md) §10), en la máquina de referencia y con la aplicación de prueba, el refresco completo en caliente tiene p95 de 98 ms (cumple) y el incremental con una región p95 de 102 ms (cumple). El incremental con dos regiones tiene p95 de 153,0 ms con 440 observaciones (150,3 ms en una repetición de 197): **no cumple**, por unos 3 ms. Con una animación continua de 33 fotogramas por segundo, una región tampoco cumple (p95 156 ms). El responsable acepta este resultado como incumplimiento documentado para el MVP, según el apartado 3.6: RNF-05 **no se marca como cumplido**, el informe lo muestra y la decisión de alcance es explícita. La medición de percepción sigue en parcial (PERF-02): la matriz pide también el caso de caché y la igualdad de modelos, y §10 no publica p99. Cerrar el caso de dos regiones (quitar entre 3 y 5 ms a la inferencia por región, o evitar la enumeración de ventanas) queda para el próximo corte.

### RNF-06. Recursos

Medir RSS/VRAM totales, cola acotada y sin crecimiento sostenido en soak de 2 h. Evidencia requerida: PERF-03.

**Umbral del responsable (15 de octubre de 2026, [CHANGELOG 1.0.62](CHANGELOG.md)).** Para el soak de 2 horas, medido sobre el proceso del motor después de los primeros 10 minutos: la pendiente por hora del conjunto de trabajo (RSS) no supera **10 MiB/h**, y la pendiente por hora del recuento de manejadores **no es positiva**. Son los valores por defecto de `tools/perf/soak.py` ([capítulo 09 §9.7](09_EVALUACION_EXPERIMENTAL.md)). La cota de la cola de regiones pendientes no está fijada (decisión 9 del [capítulo 06 §6.7](06_PLAN_INTEGRACION_DOD_Y_ROADMAP.md)), y la memoria dedicada de GPU y los bytes privados se publican sin umbral. La corrida de [S0 §12](S0_RESULTADOS.md) cumple los dos criterios fijados en sus condiciones, pero ese resultado no es un veredicto sobre RNF-06: no hay cota de cola, la corrida es de `fe35d80` y no tuvo accesibilidad. El requisito no se marca como cumplido.

### RNF-07. Compatibilidad

Win11 x64, monitor seleccionado y DPI 100/125/150%; rechazo explícito fuera de perfil. Evidencia requerida: IT-07.

**Alcance del MVP (15 de octubre de 2026).** IT-07 pasa al próximo corte: exige cambiar la escala del monitor a mano con el responsable presente, y el motor aún no rechaza las escalas fuera de perfil (sólo rechaza una pantalla rotada y un formato distinto de BGRA8; `tools/perf/dpi_probe.py` registra ese hallazgo). El perfil del MVP es Windows 11 x64, un monitor y DPI 100 %, y así lo documenta [`S4_INSTALACION_Y_REVERSION.md`](S4_INSTALACION_Y_REVERSION.md) §10. Este requisito no se marca como cumplido fuera de ese perfil.

### RNF-08. Reproducibilidad

Commit, lockfiles, modelos con hash/licencia, hardware y corpus congelados. Evidencia requerida: REL-01.

**Alcance del MVP (15 de octubre de 2026).** El corpus de aceptación congelado pasa al próximo corte, junto con EV-01 y EV-02. Lockfiles, hashes, licencias, instalación y reversión están documentados ([`S4_REPRODUCIBILIDAD.md`](S4_REPRODUCIBILIDAD.md), [`S4_INSTALACION_Y_REVERSION.md`](S4_INSTALACION_Y_REVERSION.md)); la ejecución de la instalación en una máquina limpia sigue pendiente (sesión A de [`S4_SESIONES_PENDIENTES.md`](S4_SESIONES_PENDIENTES.md)). Este requisito no se marca como cumplido.

### RNF-09. Protocolo acotado

Máx. solicitud 64 KiB; resultado 64 KiB; campos de texto con límites. Evidencia requerida: CT-10.

### RNF-10. Trazabilidad

Cada RF/RNF tiene propietario, prueba, estado y evidencia; cambios mediante ADR. Evidencia requerida: DOC-01.

## 3.4 Definiciones de las métricas

**Precisión de grounding aceptado:** acciones aceptadas que apuntan al control correcto / acciones aceptadas evaluables. **Cobertura:** solicitudes de grounding con target válido que el motor acepta / solicitudes de grounding con target válido. El rechazo seguro no cuenta como acierto de tarea; se reporta separado. Los targets sin ground truth adjudicado no se usan para inflar resultados y se cuantifican.

**Éxito de tarea:** postcondición independiente cumplida dentro del límite de tiempo y pasos. Un botón que cambió de color no demuestra éxito de la tarea. Se incluyen fallos, timeouts e intervenciones en el denominador de la corrida predefinida.

**Latencia de percepción:** desde disponibilidad/adquisición del frame solicitado hasta snapshot publicado. Se reporta la espera de captura por separado y también la latencia observada por el cliente. **Core:** algoritmos sin detector/OCR/transporte/SO. **Latencia de acción:** validación más entrega; **tiempo hasta efecto:** hasta verificación. **Ciclo del agente:** agrega inferencia del planificador y aplicación.

## 3.5 Corpus de aceptación inicial [P]

Tres familias: formulario HTML local controlado, editor de texto y explorador de archivos con datos desechables. Cinco tareas por familia; diez repeticiones por tarea y por DPI (100%, 125%, 150%): 450 ejecuciones E2E planificadas. Repartir temas claro/oscuro de forma balanceada y registrar los casos no soportados. Las tareas no deben enviar datos, comprar, transferir dinero ni borrar información real.

Para percepción se propone un corpus separado de al menos 300 capturas etiquetadas y secuencias adversariales con cambios de identidad, oclusión y layout. El número es un objetivo operativo inicial, no un cálculo de potencia estadística. La aceptación del MVP no demuestra las hipótesis científicas de no inferioridad; éstas requieren su propio análisis muestral.

**Alcance del MVP (15 de octubre de 2026).** Las 450 ejecuciones planificadas, el corpus de 300 capturas y las pruebas a 125 y 150 % de DPI pasan al próximo corte, con sus motivos: (i) el corpus de capturas etiquetadas no existe, no se versiona (ADR-0012) y exige capturas autorizadas y su etiquetado; (ii) las 450 ejecuciones exigen muchas invocaciones supervisadas con el responsable presente, y cada ronda tiene un cupo de 200 solicitudes reales, mientras que un flujo puntuado consume entre dos y ocho solicitudes por corrida, según el flujo; (iii) el cambio de escala se hace a mano (IT-07). Para este MVP, la evidencia de tareas es el protocolo reducido de S4 (RF-18 del apartado 3.2 y el capítulo 06 §6.7).

## 3.6 Presupuesto y condición de rendimiento

El hardware de medición se identifica por CPU, RAM, GPU/VRAM, driver, versión de Windows, plan de energía y build release. Mientras falte alguno de esos datos, los presupuestos no se presentan como SLA. El conjunto de baja modificación se define por el área sucia después del halo y por número de regiones, no sólo antes de expansión.

El objetivo histórico sub-10 ms end-to-end permanece en el registro como aspiración de evolución. Las metas p95 iniciales más conservadoras permiten evaluar un MVP real. Un resultado CPU-only tiene su propia tabla y no se mezcla con GPU. Si el equipo no cumple la meta, el informe muestra el incumplimiento y una decisión de alcance explícita.

## 3.7 Fuera de alcance y condiciones de rechazo

Sin soporte inicial: control remoto por HTTP, varios monitores simultáneos, HDR, Wayland/macOS, UAC o escritorio seguro, CAPTCHA, juegos, acciones financieras, instalación de software, terminal arbitraria, escritura de secretos, drag-and-drop y aprendizaje autónomo confiable de semántica. El sistema debe detectar o documentar esas condiciones y abstenerse; no presentar silencio como éxito.

## 3.8 Alcance de esta entrega documental

Entregado: especificaciones, marco de investigación, registros de decisión, contratos JSON, Gherkin, matrices, guías y referencia de algoritmos con pruebas locales. Pendiente como producto: runtime Rust compilado, adaptadores de SO/modelos, conformidad MCP ejecutada, benchmark en hardware destino y demo real. Los archivos de estado distinguen estas dos fronteras.
