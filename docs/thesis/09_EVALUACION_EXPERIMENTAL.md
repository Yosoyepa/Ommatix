# 09. Protocolo experimental y benchmark reproducible

## 9.1 Objetivo y unidad de comparación

El experimento evalúa incrementalidad y grounding, no una competición de marcas de LLM. La unidad primaria es una secuencia de UI con ground truth y cambios temporales. Se comparan pipelines sobre los mismos frames, modelos, hardware y políticas. La evaluación E2E agrega un planificador fijo y debe reportarse aparte.

## 9.2 Baselines y ablaciones

**B0:** full-frame con el mismo detector y OCR, sin reutilización perceptiva. **B1:** incremental con halo y commit, manteniendo fusión e identidad iguales a B0. **B2:** B1 con caché perceptual contextual. **B3:** sistema completo con tracking/precondiciones. **B4 opcional:** parser externo configurado exactamente y con costes de captioning declarados. **B5 opcional:** accesibilidad-only en aplicaciones que la expongan.

Para atribuir mejoras se modifica un componente a la vez. Las versiones defectuosas de FNV/dHash sirven como contraejemplos de corrección, no como baseline competitivo artificialmente debilitado. Un modo sin gates puede medirse sólo offline, sin inyectar entrada peligrosa.

## 9.3 Diseño del corpus

Estratos: cambios pequeños, menú, scroll, animación extensa, notificación, sustitución de contenido, iconos repetidos/desconocidos, formularios densos, targets pequeños, escalas DPI y temas. Etiquetar área sucia antes/después del halo, identidad de objeto y accionabilidad. Separar familias de pantallas entre calibración y test y evitar frames vecinos en ambos conjuntos.

El corpus está **por construir**. Desde el 15 de octubre de 2026 pasa al próximo corte del MVP (capítulo 06 §6.7). Se entregan plantillas, no capturas supuestamente etiquetadas. Para el piloto se propone comenzar con 30 secuencias y dos anotadores; ampliar según diversidad y precisión de intervalos. Los conjuntos públicos [S15–S16] orientan evaluación, pero no se declara que se hayan descargado ni ejecutado.

## 9.4 Instrumentación

Guardar por muestra: run_id, sequence_id, frame_id, semilla, commit, hash de configuración/modelos/corpus, versión de SO/driver/EP, temperatura o condiciones térmicas cuando sea posible, dimensiones/DPI y modo cold/warm. Tiempos con reloj monotónico, unidades explícitas y sincronización del fin de inferencia.

Spans: capture_wait, capture_copy, dirty_plan, preprocess, detector, OCR, icon_match, fusion, tracking, commit, serialization, action_validate, input_delivery y effect_wait. Si detector/OCR se solapan, no sumar sus duraciones y llamarlo wall-clock. Medir el total directamente. La suma de p95 de etapas tampoco es p95 del pipeline.

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

## 9.8 Tablas de resultados previstas

Las plantillas CSV contienen encabezados para resultados por ejecución, métricas de calidad y resumen por estrato. No contienen mediciones ficticias. La tabla productiva deberá mostrar baseline, modo, N, p50/p95/p99, precisión, cobertura, éxito, RSS y VRAM. El informe del oráculo se mantiene fuera de esas tablas para que sus milisegundos de unittest no se confundan con percepción real.
