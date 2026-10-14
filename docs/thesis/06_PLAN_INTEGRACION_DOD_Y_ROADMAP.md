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

## 6.7 Definition of Done del producto

- [ ] Captura, detector/OCR, MCP y actuador reales funcionan en el perfil acordado; no hay éxito producido sólo por fixtures.
- [ ] Requisitos RF y RNF trazados a evidencia; regresiones críticas y escenarios BDD implementados pasan en el runtime.
- [ ] Precisión y cobertura cumplen juntas; éxito global y por aplicación se mide con verificador independiente.
- [ ] Sesión/snapshot/revisión/epoch y policy gate bloquean todos los casos inválidos de la suite; ninguna redirección silenciosa.
- [ ] Diferenciador optimizado coincide con la referencia y conserva regiones pendientes tras fallos.
- [ ] Rendimiento medido en release con hardware y pesos fijados; se publican p50/p95/p99, cold/warm y fallos.
- [ ] Límite de memoria/caché/colas y soak completados; ninguna fuga observada se convierte en afirmación universal de ausencia de fugas.
- [ ] Parada, permisos, denegación por defecto, redacción y borrado de datos funcionan.
- [ ] Lockfiles, hashes, licencias, instalación, rollback y límites de compatibilidad están documentados.
- [ ] Todo incumplimiento está resuelto o aceptado explícitamente mediante cambio de alcance; no se cambia un checkbox sin evidencia.

Estas casillas permanecen pendientes en el dossier porque el producto no fue ejecutado. El informe local sólo acredita la parte documental y los oráculos realmente probados.

## 6.8 Roadmap condicionado por evidencia

**v1.1, accesibilidad híbrida:** fusionar UIAutomation y luego AX/AT-SPI mediante los puertos previstos. Comparar ahorro de inferencias con precisión y cobertura. No prometer “70% menos GPU” sin medir. El árbol accesible puede ser incompleto; sigue existiendo la ruta visual.

**v1.2, iconos y privacidad:** banco revisado de prototipos, versiones de contexto y fallback semántico opcional. Persistencia cifrada o local restringida y evaluación de open-set. No hay aprendizaje confiable automático a partir de cualquier caption.

**v1.3, rendimiento:** shapes/batching según coste, buffers GPU, SIMD verificado, streaming de cambios y memoria incremental. Cada optimización debe mantener propiedades del core. La reducción del área procesada no se usa como sustituto del tiempo medido.

**v2.0, expansión de dominio:** macOS/Linux, multimonitor, RTL, más aplicaciones y modelos. Incorporar retrieval léxico/semántico y delta snapshots sólo con paginación/base/expiración claros; medir tokens con tokenizador real y cobertura de targets.

**v3.0, flujos locales:** grafo de estados y macros de acciones con precondiciones y verificación en cada paso. No ejecutar especulativamente acciones con consecuencias. El objetivo es reducir round-trips, no garantizar cien milisegundos para tareas cuya aplicación tarda más.

## 6.9 Gestión de cambios y decisiones abiertas

Cada cambio importante registra contexto, alternativas, consecuencia y criterio de reversión en un ADR. La plataforma Windows inicial quedó aprobada el 14 de octubre de 2026 ([ADR-0008](../adr/0008-one-real-platform-first.md)). Quedan pendientes de aprobación: cliente MCP específico, aplicaciones de prueba, pesos, licencias, hardware de evaluación, corpus y políticas de uso. Los documentos contienen una propuesta coherente para poder empezar S0, no una atribución de aprobación al usuario.
