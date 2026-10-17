# 04. Especificación de los algoritmos nucleares

## 4.1 Contrato general y niveles de implementación

Se conservan los nombres de los tres algoritmos de la conversación: (1) Tile Differencing y Halo Snapping; (2) fusión espacial IoC/anisotrópica; (3) caché perceptual e identidad [F0, pp. 5–20]. En esta revisión el tercero contiene dos módulos desacoplados: reconocimiento de apariencias e identidad temporal. Los parámetros numéricos se identifican como iniciales [P] y deben calibrarse.

La referencia ejecutable en Python usa algoritmos sencillos para probar invariantes; **no implementa R*-tree, detector, OCR, SIMD, MCP ni entrada nativa**. Es un oráculo de corrección, no el runtime elegido. Una implementación Rust optimizada debe compararse con ese comportamiento cuando el caso esté dentro de su dominio. El plan TDD define qué pruebas faltan para cubrir la especificación completa.

## 4.2 Algoritmo 1: incrementalidad sin pérdida de evidencia

### Entradas, salidas y precondiciones

Entradas: frame actual, último frame base confirmado, regiones sucias pendientes y soportes visuales del último snapshot. Salida: plan de regiones a procesar y razón de selección, sin mutar aún el snapshot publicado. Precondiciones: dimensiones positivas y acotadas, stride válido, buffer suficiente, formato/epoch compatibles. Un cambio incompatible fuerza procesamiento completo.

Se usa tamaño inicial de tile 64×64 para mantener la nomenclatura original. No se vincula ese tamaño a IDs. Los bordes derecho e inferior tienen tamaño parcial; se visitan todos sus píxeles. Para un frame 1920×1080 hay `ceil(1920/64)*ceil(1080/64)=510` celdas [D].

### Referencia exacta

Para cada tile, comparar RGB/BGR de cada píxel y marcarlo sucio ante cualquier diferencia. No comparar padding de fila como si fuera imagen y no tratar cambios sólo del alfa como cambios visibles bajo la hipótesis BGRA de captura. `epsilon=0` es el baseline. El coste peor es O(W·H); el early-exit reduce el trabajo en tiles muy modificados, pero una pantalla estática obliga a comprobarla o a confiar en metadatos nativos.

```text
validate(current, base)
if geometry_changed or no_committed_base:
    return FULL_REFRESH
for every valid tile:
    dirty[tile] = any_color_pixel_diff(current, committed_base)
dirty = dirty OR pending_dirty
return plan_regions(dirty, previous_evidence)
```

Los metadatos DXGI pueden evitar un recorrido completo; se procesan los movimientos antes de las regiones dirty y se dispone de la imagen completa como respaldo [S06]. Nunca se confunden los índices del frame capturado con los del frame ya incorporado al estado.

### Cursor, caret y cambios pequeños

El cursor del mouse y el caret no son equivalentes. Un backend puede proporcionar cursor por separado; eso permite excluirlo de percepción sin eliminar pixels de la aplicación. El caret sólo se enmascara si su localización y temporalidad están justificadas. Por defecto no hay supresión por “menos de veinte píxeles”. Un check, un punto decimal o un indicador de estado pequeño pueden importar.

Una optimización que descarta cambios menores debe acumular evidencia contra el último estado realmente procesado; comparar siempre contra el frame anterior puede perder una modificación incremental lenta. La primera implementación privilegia exactitud y luego evalúa el coste de máscaras de ruido controladas.

### Regiones y halo

Primero se forman componentes conexos de tiles sucios con vecindad de ocho direcciones. Si se desea unir regiones separadas por huecos, se aplica una operación de dilatación distinta y explícita; el BFS por sí solo no hace esa dilatación. Cada componente pasa a una caja con halo inicial de 32 px, configurable por escala y contexto.

Después se calcula un cierre: consultar soportes que intersecten R; unirlos a R; consultar las dependencias de etiquetas/grupos necesarias; repetir hasta punto fijo. Se fusionan regiones solapadas y se vuelve a comprobar cierre. La implementación productiva puede usar R*-tree; el oráculo usa un recorrido lineal independiente del orden de entrada.

```text
R = clip(pad(component_bbox, halo))
repeat:
    previous = R
    R = union(R, visual_supports_intersecting(R))
    R = union(R, supports_of_required_relations(R))
until R == previous OR budget_exceeded
if budget_exceeded: return FULL_REFRESH
```

La expansión no demuestra que un elemento nuevo, aún no indexado, quepa entero. Detecciones que toquen el borde del crop y relaciones incompletas disparan una ampliación o full refresh. El fallback también aplica por TTL, cambio de ventana o demasiadas regiones. El umbral inicial de área agregada del 40% y de cuatro regiones es una propuesta que se reemplazará por un modelo de coste medido.

### Regla de coste adaptativa (INC-07, ADR-0030)

El modelo de coste medido es, por ahora, una regla de una sola cantidad. Cada región es una inferencia completa del detector (entrada de 1280×1280), y el fotograma entero es una sola; con dos o más regiones el plan regional cuesta más inferencias que el refresco completo, y además el detector ve los recortes a otra escala que el fotograma reducido y pierde elementos ([`S0_RESULTADOS.md`](S0_RESULTADOS.md) §13.3, §13.10 y §13.11). Por eso, después de las comprobaciones de presupuesto (cierre, número máximo de regiones y fracción de área), el plan se convierte en refresco completo si tiene `adaptive_min_regions` regiones o más:

```text
if regions >= adaptive_min_regions and adaptive_min_regions > 0:
    return FULL_REFRESH(reason = adaptive_cost)
```

La regla **sólo adelanta el respaldo que el algoritmo ya tiene**: el refresco completo es el fallback explícito de este algoritmo, y los presupuestos conservan su razón (`closure_budget`, `too_many_regions`, `area_budget`) cuando son ellos los que se disparan. No retira garantías: el conjunto de teselas sucias y pendientes es el mismo que el del plan regional, de modo que una observación fallida deja pendiente lo mismo. `adaptive_min_regions = 0` la apaga; `[engine] adaptive_min_regions` vale 2 por defecto (la configuración del producto), mientras que el planificador de referencia (`PlanParams::default`) la lleva apagada. La razón se registra como `adaptive_cost` en `io.ommatix/plan` y en el registro de investigación. La línea de base B0 (`experimental_baseline`) y la falta de base confirmada (`no_committed_base`) tienen precedencia sobre ella.

**Oráculo.** La referencia ejecutable (`referencia_ejecutable/oraculo/core.py`; por ejemplo OR-12 para el cambio de época y OR-14 a OR-17 para el cierre y la fusión) cubre la diferencia de teselas, el cierre del halo y la fusión; no cubre los respaldos de `plan_regions` (número máximo de regiones, fracción de área), de modo que la regla no contradice ninguna prueba del oráculo y éste no cambia. Las pruebas de la regla están en Rust (`inc_07_*`, dominio, aplicación y configuración); no hay prueba del oráculo equivalente, y por eso no se marca ninguna UT como pasada por ella.

### Commit y reproyección

El motor construye candidatos regionales sin borrar primero el estado útil. Cada caja se devuelve a píxeles físicos deshaciendo padding y escalado. Se actualiza el soporte de evidencia; se invalidan las entidades y relaciones alcanzadas, incluso si su centro no estaba dentro de R. Se hace matching temporal y publicación atómica de registro e índice.

La representación de las teselas pendientes del motor se recorta a la cuadrícula del fotograma cuya observación falla: como máximo `ceil(ancho / tesela) × ceil(alto / tesela)` entradas; un cambio geométrico conserva el respaldo completo y la época aumenta incluso al volver a un tamaño anterior (evidencia sin conexión de 1.0.87).

Si hay error, ni la base confirmada ni las regiones pendientes avanzan como si el trabajo hubiera terminado. Una estrategia simple y segura bloquea acciones hasta completar una reobservación consistente. Un índice parcialmente actualizado jamás se presenta como snapshot completo.

### Casos límite obligatorios

Ancho impar, stride con padding, cambio en una sola fila impar, último píxel, sólo alfa, primer frame negro, error de tamaño, cambio de resolución de igual tamaño de buffer, scroll, fallo de inferencia, halo transitivo y orden de candidatos diferente. Los benchmarks comparan exacto escalar contra SIMD y metadatos DXGI, usando los mismos resultados esperados.

## 4.3 Algoritmo 2: reconciliación espacial y asociación

### Validación y normalización

Rechazar coordenadas NaN/infinito, cajas degeneradas y textos vacíos no significativos. Conservar el polígono OCR cuando aporte información; la AABB es una aproximación declarada para las consultas iniciales. Normalizar texto con Unicode NFC y colapsar espacios sin fusionar palabras: “a b” y “ab” no deben convertirse en la misma etiqueta. La etiqueta visible original se conserva.

Si el proveedor sólo sabe “región interactuable”, no asignar `input` por intuición del motor. El rol puede ser `unknown`. La accionabilidad se calcula separada del rol, con evidencia y política explícitas.

### Propiedad del texto interno

Construir candidatos de contención con `IoC >= 0.75` como parámetro inicial. Para cada texto, comparar propietarios válidos y preferir el contenedor accionable específico frente al panel que lo contiene, usando jerarquía, tipo y tamaño. Si dos controles iguales reclaman el texto sin resolución, no depender del orden de las detecciones: devolver ambigüedad.

No ejecutar NMS indiscriminado entre clases distintas. Un icono contenido en un botón puede ser decoración o una acción separada; se requiere evidencia del detector/jerarquía para absorberlo. Mantener relación `contains` puede ser más correcto que borrar un nodo.

### Renglones y orden de lectura

Primero agrupar textos por compatibilidad de línea; después ordenar las líneas por Y y dentro de ellas por X, con desempates estables. Una implementación conservadora puede usar bandas por ancla de línea para evitar cadenas transitivas accidentales. En textos inclinados o multilínea se usan las métricas del OCR y se permiten saltos de línea en la representación interna.

No usar un comparador que decida entre X/Y con una tolerancia calculada por pareja. Ese procedimiento no es necesariamente transitivo y puede dar salidas diferentes al cambiar el orden de entrada. Para entradas LTR, la clave de orden final puede ser `(line_id, xmin, ymin, stable_evidence_id)`.

### Labels externos y valores

Para inputs y dropdowns se buscan etiquetas arriba o a la izquierda; para checkboxes, etiquetas laterales. Son priors del dominio LTR, no leyes universales. Las ventanas y distancias se expresan en una escala local —altura de texto o control— y no sólo en píxeles absolutos. El idioma RTL se deja fuera del MVP y se conserva como parámetro futuro.

El coste usa distancia de bordes, desalineación y contexto. El corredor de oclusión es el espacio entre bordes enfrentados; excluye el propio control, su texto y ancestros de grupo. Un panel detrás de ambos no debe actuar como bloqueador. Oclusión real requiere información de visibilidad/z-order adicional; las cajas no bastan.

Se resuelve la asignación entre labels y controles por grupos, con coste de no asignación. Para componentes ambiguos se puede usar matching bipartito mínimo; para un MVP conservador se aceptan sólo mejores candidatos mutuos y claramente separados del segundo candidato. Ambas opciones deben estar declaradas en configuración y evaluadas. No se promete resolver sin errores todos los formularios de dos columnas.

El contenido interno de un input se publica como texto observado. `text_kind` vale `value`, `placeholder` o `unknown`; la geometría por sí sola sólo justifica el último. Un label externo no se consume ciegamente si funciona como encabezado de un grupo.

### Huérfanos y puntos de acción

El OCR no reclamado permanece como `text_node` informativo. No se elimina: puede ayudar al agente a interpretar el estado o a solicitar una región. Pero no se promueve automáticamente a clicable. Se requiere affordance confirmada para habilitar `click`.

El punto candidato se calcula dentro de la parte visible y válida del control, alejado del borde y consistente con el soporte detectado. Un centroide ponderado hacia texto no garantiza seguridad en controles grandes. Para un shape complejo o parcialmente tapado se abstiene o se consulta un backend de hit-testing, cuando exista.

### Coste y pruebas

Una implementación ingenua cuesta O(N·M). El índice reduce la enumeración de candidatos en cargas favorables, con coste dependiente de intersecciones devueltas K; no garantiza O(M log N) en cualquier distribución [S19]. Para cientos de cajas se compara también un bucle vectorizado/cache-friendly. El criterio es tiempo observado y equivalencia de resultados, no preferencia estética por el árbol.

Un enumerador experimental de candidatos con R*-tree ([ADR-0027](../adr/0027-candidate-enumeration-with-an-rstar-index.md), `[engine] candidate_search = "rstar"`, desactivado por defecto) construye una vez por fusión un índice sobre los controles ya reconciliados y sólo enumera candidatos: los propietarios de una palabra, con la caja de la palabra como ventana, cuando el umbral es positivo; y los obstáculos del corredor, con el hueco del corredor como ventana. Cada candidato pasa por los mismos predicados, orden y abstenciones del recorrido. Con umbral cero se recorre todo, porque no se puede probar que ningún control quede fuera. No se indexan las palabras, la reconciliación con accesibilidad ni el cierre de soportes. La medición y la decisión están en [`INDICE_A2_RESULTADOS.md`](INDICE_A2_RESULTADOS.md): el recorrido sigue siendo la referencia y el valor por defecto.

Se prueban contenedores anidados, OCR permutado, etiquetas multilineales, paneles de fondo, duplicados, ambigüedad simétrica, controles sin rol, placeholder, coordenadas inválidas y cambios de DPI. Los errores de asociación se clasifican por tipo, no sólo por una media de IoC.

## 4.4 Algoritmo 3A: caché perceptual con rechazo

### Descripción

Se mantienen dos baselines: dHash horizontal de 64 bits y pHash si el coste resulta aceptable. Ninguno es automáticamente mejor en todas las UI. El descriptor corregido propuesto agrega una máscara de soporte a los signos de gradiente. Es de **128 bits lógicos** —64 de signo y 64 de validez—, no se lo publicita como dHash puro de 64 bits.

La normalización del crop evita el recorte interior fijo de 15% como axioma: puede eliminar trazos significativos. Se registra padding y se compara con variantes sin recorte en el corpus. Prototipos se particionan por aplicación/contexto y llevan procedencia y versión de normalización.

### Matching

Calcular `(bits,mask)` a partir de diferencias horizontales. Comparar sobre `common=mask_q & mask_p`, exigir al menos ocho transiciones útiles y un solapamiento de soportes mínimo inicial de 0,70. La distancia de signo permite elegir polaridad normal o invertida. Exigir también separación respecto al segundo candidato; una igualdad devuelve `unknown`.

```text
if common_support < min_support: UNKNOWN
if intersection_support / union_support < min_overlap: UNKNOWN
distance = min(mismatches, common_support-mismatches) / common_support
accept only if distance <= threshold AND margin >= min_margin
```

Estos parámetros no son calibraciones empíricas. El oráculo prueba soporte vacío, inversión ideal y conflicto; no acredita precisión del umbral en iconos reales. Una coincidencia exacta sólo recupera una etiqueta existente; no crea conocimiento semántico nuevo.

### Capa semántica y caché

El resultado contiene etiqueta, source, versión del prototipo, contexto y score de similitud. No interpretar la similitud como probabilidad de intención. Las entradas procedentes de un fallback no confiable no se promueven de inmediato al banco confiable; pasan por revisión o validación independiente.

El caché exacto tiene límite LRU/TTL y key con contexto, versión del descriptor y firma; la búsqueda fuzzy escanea prototipos o un índice adecuado tras medir. Un hash perceptual no debe ser clave única de identidad. Las colisiones se representan como múltiples candidatos, no sobrescribiendo una etiqueta.

## 4.5 Algoritmo 3B: identidad estable por tracking

### Ciclo de vida

La clasificación de un ID retirado se deriva del prefijo de IDs emitidos y de su ausencia entre los tracks actuales; no se almacena un conjunto histórico de retirados ni se reciclan IDs, tampoco tras una purga (1.0.87).

Un track pasa por `new → active → ambiguous/occluded → retired`. Un ID se asigna una sola vez dentro de la sesión, se guarda como cadena opaca y no se recicla. La etiqueta puede cambiar sin renumerar todo el snapshot; una modificación significativa incrementa revisión.

Los candidatos se asocian sólo dentro del mismo contexto permitido. Se combinan rol, grupo, texto discriminante, apariencia, continuidad geométrica e identidad nativa de accesibilidad, descrita a continuación. Para virtualización, cambio de fila o sustitución semántica hay que retirar/renovar el track aunque no cambie la caja.

### Identidad nativa de accesibilidad

Un elemento que aporta la accesibilidad de la plataforma (UI Automation, ADR-0017) lleva como discriminante su `RuntimeId`, acotado por el proveedor (`uia`), el proceso que lo aloja y la ventana del proveedor. Ese alcance es la única comparación válida: el mismo `RuntimeId` en otro proceso, en otra ventana o en otra sesión no designa el mismo objeto. El identificador sólo vale mientras el elemento exista, y sólo se conserva entre observaciones consecutivas: una observación que no lo vuelva a ver retira el track. El `RuntimeId` no sale del motor; el agente sigue recibiendo sólo IDs de sesión (ADR-0026).

Un elemento visual, detectado por contornos u OCR, no tiene discriminante. Su continuidad se decide sólo por rol, texto, apariencia y caja, como antes.

Si la continuidad no está probada, el objeto recibe un ID nuevo, nunca el ID de su vecino más parecido. Esto cubre:

- una sustitución con el mismo texto y la misma caja, que es otro objeto con otro `RuntimeId`: ID nuevo, y el anterior se retira;
- dos gemelos que intercambian su sitio: ninguno conserva su ID y ninguno toma el del otro;
- dos gemelos apilados en la misma caja: cada uno conserva el suyo, porque su identidad los distingue donde la caja no puede;
- un contenedor reciclado para otro contenido (mismo `RuntimeId`, distinto nombre): ID nuevo;
- un elemento que sale de la ventana y vuelve, aunque conserve su `RuntimeId`: ID nuevo.

Cada retirada registra su motivo como código, sin etiquetas, discriminantes, cajas ni identificadores de usuario: `vanished` (ningún elemento lo continúa), `moved` (su identidad aparece fuera del IoU mínimo), `substituted` (otro objeto con el mismo rol, texto y caja ocupa su sitio), `ambiguous` (varios candidatos compiten por la asociación) o `geometry_changed` (cambio de época geométrica). El motivo no sale por el contrato; queda en el registro de la última observación publicada.

Límite: el algoritmo 1 relee la accesibilidad sólo en las regiones que cambian. Una sustitución que no altera ningún píxel de su región no se ve hasta que la región cambia. La revalidación de la identidad justo antes de la entrega corresponde al incremento de vigencia (INC-03).

### Seguridad de ejecución

La petición incluye `session_id`, `snapshot_id`, `target_id` y `expected_revision`. Se valida contra el registro confirmado. Identidad persistente y vigencia no son lo mismo: conservar “e7” no permite ejecutar una petición del snapshot anterior. El MVP rechaza referencias obsoletas de forma conservadora.

No se intenta resolver una referencia desconocida buscando un label parecido a 150 px. Una nueva observación puede permitir al agente seleccionar la entidad correcta de forma explícita. El coste de un round-trip adicional es preferible a ejecutar una acción potencialmente distinta.

### Propiedades

IDs únicos por sesión; ausencia de reutilización; independencia de la posición exacta; matching uno-a-uno; revisión monotónica; snapshots inmutables; rechazo de sesión/epoch incorrectos; baja de elementos desaparecidos; abstención ante asociación indistinguible; identidad nativa acotada por proveedor, proceso y ventana; motivo de cada retirada registrado sin valores. Los contadores persistentes no demuestran identidad por sí solos: esas propiedades también dependen de la calidad del matcher.

## 4.6 Optimización posterior a corrección

Primero se mide una implementación clara. Después se evalúan SIMD, batch de regiones, reutilización de tensores, arena de memoria, strings internados e índices incrementales. Cada optimización necesita una prueba diferencial con el oráculo y medición en release. Una lectura de bloques u64 no se denomina SIMD sin comprobar el código generado o usar instrucciones vectoriales explícitas.

No se afirma “cero asignaciones”: el código original clona Strings y construye Vecs [F0, pp. 13–20]. Se añade un presupuesto y perfil de asignaciones. POPCNT puede acelerar Hamming en plataformas adecuadas, pero no vuelve nanosegundos todo el pipeline de recorte, resize, matching y semántica.

## Frescura antes de una acción (ADR-0032)

La planificación incremental del Algoritmo 1 sigue usando todos los tiles frente a la base comprometida. La comparación de autorización es independiente: por defecto (`screen`) también mira la pantalla entera; con `target_window` aplica la misma comparación exacta sólo a los tiles que intersecan la unión de rectángulos del grupo del objetivo, extendidos por el margen y recortados al frame. Cajas semiabiertas: tocar sólo el borde no cuenta como intersección; un píxel exterior en un tile que interseca sí provoca rechazo. Las lecturas incompletas o inestables hacen volver a la pantalla entera; la geometría y la época conservan su rechazo global. La identidad de ventanas se valida aparte, incluso si los píxeles no cambiaron. Sólo ante un rechazo se cuenta además el diff global para el diagnóstico D0; los éxitos del alcance opcional no recorren los píxeles externos. El efecto de la acción siempre se compara sobre la pantalla completa. UT-55..UT-64 y BDD-55..BDD-58 cubren estas reglas sobre frames sintéticos; no hay oráculo OR de frescura ni evidencia viva de IT-08.
