# 00. Auditoría técnica y control de cambios

**Proyecto:** Ommatix. **Versión documental:** 1.0 revisada. **Fecha de revisión:** 13 de octubre de 2026. **Estado:** propuesta de ingeniería con validación algorítmica parcial; no es una certificación de un MVP desplegado.

## 0.1 Dictamen

La idea central es razonable: separar la percepción de la planificación, mantener geometría local y ofrecer al agente identificadores de elementos. Se conservan Rust, la exposición MCP local, la fusión espacial, la percepción incremental y el caché de iconos como ejes del diseño [F0, pp. 1–5]. Sin embargo, el PDF mezcla decisiones de diseño con resultados que no demuestra y presenta una demostración de estado precargado como si fuera un controlador funcional.

La revisión no atribuye errores del formato exportado al código original sin evidencia. Algunas líneas y genéricos del código aparecen truncados o deformados en el PDF. Esto impide certificar su compilación mediante una transcripción literal. Los fallos lógicos identificados abajo son independientes de esos daños de presentación.

**Recomendación:** conservar la arquitectura general, sustituir la identidad por tracking persistente, introducir transacciones de observación/acción, hacer conservadora la accionabilidad y medir antes de convertir objetivos de latencia en compromisos. La decisión sobre modelos se mantiene abierta hasta un spike de compatibilidad, clases y licencias.

## 0.2 Convención de evidencia

[F0, p. n] identifica el PDF proporcionado, que se conserva fuera del repositorio. [S01]–[S24] son fuentes externas primarias consultadas para esta revisión. [D] identifica una deducción o contraejemplo propio; [P], una propuesta nueva. [V] se reserva a verificaciones realmente ejecutadas y registradas en `referencia_ejecutable/resultados/`. Un test sintético aprobado no acredita calidad visual en aplicaciones reales.

## 0.3 Hallazgos críticos

### A-01. execute_action no ejecuta acciones reales

**Severidad: bloqueante.** En las pp. 43–45 se precargan cajas y OCR de demostración. La rama `execute_action` busca un ID y construye una respuesta con `status: ok` y coordenadas, pero no llama a un actuador del SO. Tampoco hay captura o inferencia en cada observación [F0, pp. 43–45]. No es un MVP de control de escritorio, aunque sirva como demostración de serialización. Corrección: estados `simulated`, `input_delivered`, `effect_observed` y `indeterminate`, y un gate E2E obligatorio con captura y entrada reales. SendInput informa cuántos eventos insertó, no si la aplicación completó la intención [S07].

### A-02. El ID espacial no es estable en los bordes

**Severidad: bloqueante.** `floor(x/64)` cambia entre x=63 y x=64: basta un píxel para cambiar la semilla del hash. No existe inmunidad general al jitter de ±32 píxeles. Dos controles con igual rol y etiqueta dentro de la misma celda comparten exactamente la semilla y, por tanto, el ID [F0, pp. 13–15]. Corrección [P]: IDs monotónicos sin reutilización durante una sesión, asociados por tracking entre snapshots; la posición participa en el matching, no en el identificador.

### A-03. Los cinco dígitos introducen colisiones materiales

**Severidad: bloqueante.** El módulo 90.000 limita el espacio a 90.000 valores. Para n=300 identificadores uniformes e independientes, la probabilidad de al menos una colisión es `1 - product(1 - k/90000, k=0..299)`, aproximadamente 39,3%. La independencia no está garantizada y las semillas iguales del hallazgo anterior producen colisión segura [D]. Un `HashMap.insert` puede sobrescribir un elemento. El oráculo calcula la probabilidad y construye casos deterministas; no se presenta esta aproximación como medición de una UI real.

### A-04. Reanclar por etiqueta y cercanía puede dirigir un clic a otro objeto

**Severidad: bloqueante.** El fallback de 150 px puede confundir botones “Eliminar” de filas virtualizadas, ventanas o documentos distintos [F0, p. 15]. Se elimina el redireccionamiento silencioso. El MVP exige observación vigente, revisión del elemento y contexto de ventana iguales. Una asociación ambigua devuelve rechazo y pide reobservar. Persistir un ID no autoriza actuar sobre contenido nuevo que ocupa la misma caja.

### A-05. La invariancia del dHash propuesto no se cumple en general

**Severidad: alta.** Con el bit `I(delta >= 3)`, un gradiente plano produce 0 antes y después de invertir la luminancia; no produce el complemento. Repitiendo ocho veces la fila `[0,10,10,0,0,10,10,0,0]`, los hashes original e invertido difieren en 32 bits: la fórmula original retorna 32, no un match a umbral 8 [D; F0, pp. 12–14]. Corrección [P]: descriptor de signo y máscara de gradientes válidos, comparación sólo sobre soporte común y rechazo de soporte insuficiente. No se afirma invariancia a cambios arbitrarios de tema, renderizador o escala.

### A-06. El caché no descubre la semántica de un icono desconocido

**Severidad: alta.** La similitud visual no prueba significado. Un mismo símbolo puede significar “cerrar”, “quitar filtro” o “eliminar”. Un clasificador de clases cerradas no incorpora nuevas clases semánticas sólo por ver otro icono [F0, pp. 12, 27; S21]. Corrección: prototipos curados con contexto, umbral de rechazo, margen frente al segundo candidato y procedencia de la etiqueta. Lo desconocido permanece `unknown`; el fallback semántico es opcional, explícito y fuera de la ruta rápida.

### A-07. Texto detectado no implica elemento clicable

**Severidad: bloqueante.** Convertir todo OCR no reclamado en un nodo accionable crea falsos positivos sobre títulos, párrafos, mensajes y datos [F0, pp. 6, 11]. Tampoco demuestra recall del 100%: el OCR puede omitir texto. Corrección: preservar `text_node` informativo con `actionability: unknown`; sólo habilitar acciones con evidencia positiva y política compatible.

### A-08. Hay cambios reales que el diferenciador no ve

**Severidad: bloqueante.** El salto `y += 2` omite filas impares; `chunks_exact(8)` sin procesar el resto omite un último píxel en anchos impares. Un umbral de veinte píxeles descarta cambios pequeños legítimos. La primera versión cuenta dos píxeles si cambia cualquiera de un par; la versión final corrige ese conteo, pero conserva otras omisiones [F0, pp. 16–18, 37–38]. Corrección: referencia exacta con todas las filas, stride real y bordes parciales; cualquier optimización debe conservar la detección de cambios relevantes bajo pruebas diferenciales.

### A-09. Actualizar el frame base antes del commit puede perder cambios

**Severidad: bloqueante.** Si la inferencia falla después de copiar el framebuffer de referencia, el siguiente frame puede parecer limpio aunque su región nunca haya entrado al estado publicado [F0, pp. 18–20]. El estado corregido distingue frame capturado, regiones pendientes y último snapshot confirmado. Una región sólo se considera limpia al completar el commit, no al calcular la diferencia.

### A-10. Halo Snapping no está cerrado transitivamente

**Severidad: alta.** El código calcula una sola caja de consulta y la reutiliza aunque la región se expanda. Un segundo elemento alcanzado por la expansión puede quedar cortado. Tampoco fusiona necesariamente regiones que pasan a solaparse [F0, pp. 19, 38]. Corrección: cierre hasta punto fijo, fusión de regiones y nueva consulta; límites de trabajo con fallback de pantalla completa. Un halo nunca garantiza cobertura de un elemento nuevo que aún no está en el índice: se añaden detector de contacto con borde y refrescos completos.

## 0.4 Hallazgos de arquitectura, evidencia y especificación

### A-11. La supuesta dilatación no corresponde al BFS

La vecindad Chebyshev de radio 1 une celdas sucias adyacentes, no dos celdas sucias separadas por una limpia. Se distinguen explícitamente dilatación y componentes conexos. La supresión del caret no se deduce de la cantidad de píxeles cambiados [F0, pp. 16–18].

### A-12. Invalidar sólo centroides no equivale a invalidar evidencia

Una etiqueta externa, un contenedor o una relación puede verse afectada sin que el centro del control caiga dentro del recorte. Se invalidan entidades por soporte visual y dependencias; no se destruye el estado visible antes de tener reemplazo [F0, pp. 19–20].

### A-13. El ordenamiento OCR puede ser no transitivo

El comparador que alterna entre X y Y según una tolerancia por pareja no define necesariamente un orden total. Con alturas 10 y centros Y de 0, 4 y 8, los pares vecinos pueden ordenarse por X y los extremos por Y. Con X decreciente se obtiene una contradicción de transitividad [D]. Se agrupan renglones primero y luego se ordena cada grupo con una clave total. Las cajas no finitas se rechazan antes de ordenar.

### A-14. claimed y la contención dependen del orden de entrada

Si un contenedor grande reclama un texto antes que el botón interno, el botón pierde su etiqueta. Un icono dentro de un botón no siempre debe absorberse: podría ser una acción independiente. Se selecciona propietario específico por jerarquía y evidencias; se usa asignación uno-a-uno con abstención cuando exista ambigüedad. Una etiqueta puede agrupar controles y no ser propiedad exclusiva de uno [F0, pp. 5–10].

### A-15. El puente de oclusión es demasiado amplio

La unión AABB de texto y control incluye ambos extremos, no sólo el espacio intermedio. Puede bloquear relaciones correctas por elementos hijos o un panel de fondo. Se usa un corredor entre bordes enfrentados, contexto de grupo y exclusión de ancestros. La oclusión geométrica de cajas no sustituye z-order ni hit-testing del SO [F0, pp. 10–11].

### A-16. value y placeholder no se distinguen por geometría

Texto dentro de un input no prueba que sea un valor introducido. Se conserva `text_kind: unknown` cuando no hay evidencia adicional; no se inventa un valor ni se trata un marcador visual como dato del usuario [F0, pp. 6, 9].

### A-17. La inferencia recortada no cuesta necesariamente menos

Si cada recorte se redimensiona al mismo tensor, por ejemplo 640×640, varias regiones pueden requerir más inferencias que una pantalla completa. Se introduce un selector de estrategia basado en coste medido, número de regiones y shape. La reproyección necesita deshacer letterboxing, no sólo sumar offsets [F0, pp. 16–20; S09].

### A-18. No hay garantía de tiempo real por elegir Rust

Rust sigue siendo la decisión de runtime por control de memoria, interfaces nativas y mantenibilidad. No implica hard real-time, inicio en 10 ms, empaquetado estático de todo ONNX ni una mejora universal de 5–15×. Python puede usar kernels nativos y operaciones NumPy que liberan el GIL [F0, pp. 3–5; S22]. Se separa la referencia de pruebas en Python del runtime productivo propuesto en Rust.

### A-19. MCP/HTTP no son “inviables” para toda comunicación interna

Conviene evitar serializar framebuffers en JSON. Esto no vuelve inviables HTTP o sockets para pequeños mensajes de control. Se elige un monolito modular con buffers compartidos por propiedad; no Redis ni IPC externo en el MVP. La necesidad de zero-copy entre GPU/CPU se prueba, no se deduce de `mmap` [F0, pp. 3–4].

### A-20. La revisión del protocolo estaba desactualizada

El ejemplo declara 2024-11-05. La especificación consultada 2026-07-28 usa metadatos por petición y elimina el handshake `initialize/initialized`; el perfil 2025-11-25 mantiene ese ciclo [S01–S05]. El dossier define un perfil moderno y un adaptador legacy opcional, sin mezclar reglas ni prometer compatibilidad universal. Los JSON del paquete documentan el contrato; no acreditan un servidor MCP ejecutándose.

### A-21. Los modelos y las licencias no se pueden asumir

OmniParser distingue detector y captioner, y las licencias cambian entre artefactos: la documentación actual diferencia `icon_detect_v3` MIT y detectores previos Ultralytics AGPL [S11–S12]. Screen2AX publica modelos de elementos/grupos para macOS, incluyendo YOLOv11l; una versión nano destilada no aparece creada por el PDF [S13–S14]. Es obligatorio fijar hash, revisión, clases, licencia y procedimiento de exportación. No se incluyen pesos de terceros.

### A-22. Falta una política de errores, permisos y coordenadas

Una UI puede cambiar de ventana, monitor, DPI o sesión mientras se razona. Hace falta gestionar permisos, bloqueo del escritorio, pérdida de DXGI, entradas parciales, oclusión y cancelación [S06–S08]. Se agregan estados degradados, epochs de geometría, un interruptor de parada y ejecución deshabilitada por defecto.

### A-23. Las cifras del PDF son hipótesis, no resultados

Sub-10 ms, 98% de reducción, 92% menos tokens, 95% de iconos repetidos, 99% de resolución normal y recall del 100% no están respaldados por un dataset, hardware y trazas medidos en la conversación [F0, pp. 1–3, 11–12, 20–27]. Se conservan como historial, no como conclusiones. La suma de percentiles por etapa tampoco determina el percentil end-to-end.

### A-24. El DoD estaba marcado como cumplido sin evidencia

El generador incluye casillas `[x]` de tests y latencias, pero no registra ejecución. Cinco escenarios Gherkin sin step definitions no equivalen a cinco tests BDD ejecutados [F0, pp. 25–31; S24]. El nuevo DoD parte pendiente. Sólo el informe de validación local marca como aprobadas las comprobaciones que realmente se corrieron.

## 0.5 Cambios aceptados para este dossier, pendientes de aprobación del proyecto

Se conservan los tres bloques algorítmicos originales, pero el bloque de iconos/IDs pasa a tener dos submódulos independientes. Se sustituye FNV cuantizado como identidad; el dHash se mantiene como baseline y se propone un descriptor enmascarado; la incrementalidad se hace transaccional. La geometría no infiere accionabilidad absoluta. La accesibilidad completa continúa fuera del core visual inicial, pero se reservan puertos para integrarla y se usan metadatos mínimos de ventana/foco para seguridad.

El MVP funcional queda especificado para una sola plataforma inicial y requiere adaptadores reales. La entrega de hoy es el paquete de investigación y diseño con oráculos de regresión, contratos, escenarios y matrices. No se declara terminado el producto ni se afirma que la tesis tenga resultados empíricos sobre escritorios reales.
