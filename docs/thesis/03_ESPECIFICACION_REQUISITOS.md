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

Validar sesión/snapshot/revisión, política, ventana/foco y frescura. Trazabilidad: UT-38..UT-43. Hito: S2/S3.

### RF-12. Actuación real

Click/type/scroll con permiso; reportar eventos entregados sin falso éxito. Trazabilidad: IT-03. Hito: S3.

### RF-13. Postcondiciones

Separar entrega, cambio y efecto; timeout o incertidumbre sin retry ciego. Trazabilidad: IT-04. Hito: S3.

### RF-14. Deduplicación

request_id repetido no duplica input; conflicto y caída exigen reconciliación. Trazabilidad: IT-05. Hito: S3.

### RF-15. MCP

Perfil moderno y compatibilidad legacy sólo si se prueba explícitamente. Trazabilidad: CT-05..CT-09. Hito: S2.

### RF-16. Parada y privacidad

Actuación deshabilitada al inicio; parada local y telemetría redactada. Trazabilidad: SEC-01..SEC-06. Hito: S3.

### RF-17. Observabilidad

Métricas por etapa y motivos de rechazo; ninguna captura/texto crudo por defecto. Trazabilidad: IT-06. Hito: S3.

### RF-18. Demo funcional

Ejecutar flujos reales verificables en 3 aplicaciones permitidas. Trazabilidad: E2E-01..E2E-03 y, para el protocolo reducido de S4, S4R-02 y S4R-03 (no sustituyen a E2E-02 ni a E2E-03: el editor con escritura y guardado y el explorador de archivos siguen pendientes). Hito: S4.

**Evidencia puntuada a 14 de octubre de 2026 (RF-18 sigue sin cumplirse).** Hay corridas puntuadas, con verificador independiente del motor y sin falsos éxitos, en dos aplicaciones: la aplicación de prueba propia (E2E-01, parcial: una tarea, un DPI y un tema) y el Bloc de notas (S4R-03: abrir y cerrar «Editar», diez de diez corridas, sólo clics). Falta la Calculadora (S4R-02: no se ha ejecutado, porque el cambio al modo Estándar no se había logrado), falta el editor con escritura y guardado de un archivo temporal (E2E-02) y el explorador de archivos (E2E-03), y faltan los DPI, los temas y las 450 corridas del diseño.

## 3.3 Requisitos no funcionales

### RNF-01. Corrección del core

100% de regresiones críticas aprobadas; cero referencia obsoleta ejecutada en suite. Evidencia requerida: UT/SEC.

### RNF-02. Grounding útil

Precisión aceptada >=98% y cobertura >=85% en corpus de aceptación. Evidencia requerida: EV-01.

### RNF-03. Éxito de tareas

Éxito global de al menos 90% y por aplicación de al menos 85% en el protocolo definido. Evidencia requerida: EV-02.

### RNF-04. Latencia core

p95 <=10 ms para 300 controles y 600 spans; sólo core, hardware fijado S0. Evidencia requerida: PERF-01.

### RNF-05. Latencia percepción

Metas iniciales: incremental p95 <=150 ms; full warm p95 <=700 ms. Evidencia requerida: PERF-02.

### RNF-06. Recursos

Medir RSS/VRAM totales, cola acotada y sin crecimiento sostenido en soak de 2 h. Evidencia requerida: PERF-03.

### RNF-07. Compatibilidad

Win11 x64, monitor seleccionado y DPI 100/125/150%; rechazo explícito fuera de perfil. Evidencia requerida: IT-07.

### RNF-08. Reproducibilidad

Commit, lockfiles, modelos con hash/licencia, hardware y corpus congelados. Evidencia requerida: REL-01.

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

## 3.6 Presupuesto y condición de rendimiento

El hardware de medición se identifica por CPU, RAM, GPU/VRAM, driver, versión de Windows, plan de energía y build release. Mientras falte alguno de esos datos, los presupuestos no se presentan como SLA. El conjunto de baja modificación se define por el área sucia después del halo y por número de regiones, no sólo antes de expansión.

El objetivo histórico sub-10 ms end-to-end permanece en el registro como aspiración de evolución. Las metas p95 iniciales más conservadoras permiten evaluar un MVP real. Un resultado CPU-only tiene su propia tabla y no se mezcla con GPU. Si el equipo no cumple la meta, el informe muestra el incumplimiento y una decisión de alcance explícita.

## 3.7 Fuera de alcance y condiciones de rechazo

Sin soporte inicial: control remoto por HTTP, varios monitores simultáneos, HDR, Wayland/macOS, UAC o escritorio seguro, CAPTCHA, juegos, acciones financieras, instalación de software, terminal arbitraria, escritura de secretos, drag-and-drop y aprendizaje autónomo confiable de semántica. El sistema debe detectar o documentar esas condiciones y abstenerse; no presentar silencio como éxito.

## 3.8 Alcance de esta entrega documental

Entregado: especificaciones, marco de investigación, registros de decisión, contratos JSON, Gherkin, matrices, guías y referencia de algoritmos con pruebas locales. Pendiente como producto: runtime Rust compilado, adaptadores de SO/modelos, conformidad MCP ejecutada, benchmark en hardware destino y demo real. Los archivos de estado distinguen estas dos fronteras.
