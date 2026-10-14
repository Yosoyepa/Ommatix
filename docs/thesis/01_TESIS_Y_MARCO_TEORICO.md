# 01. Base investigativa y borrador de tesis

## 1.1 Título y naturaleza del documento

**Percepción visual incremental y grounding temporal seguro para agentes de escritorio: fusión espacial, caché perceptual y control por identificadores mediante MCP.**

Este es un borrador de investigación aplicada y diseño experimental, no una tesis con resultados de campo ya obtenidos. Ommatix es el nombre del proyecto. Los apartados de problema, teoría y metodología se desarrollan; los resultados de rendimiento y calidad perceptiva quedan pendientes. La validación local del anexo cubre propiedades de algoritmos, no tareas reales de escritorio.

## 1.2 Resumen

Se propone un motor local que convierte observaciones visuales de una interfaz en elementos estructurados y temporalmente identificables. Un agente decide qué elemento usar sin generar coordenadas físicas. El motor mantiene la geometría, valida vigencia y restricciones de ejecución, inyecta entrada mediante un adaptador del sistema operativo y observa el efecto. La ruta rápida combina cambios de pantalla, detección/OCR regionales, reconciliación geométrica y un caché perceptual de iconos conocidos.

La contribución a evaluar no es reemplazar toda comprensión visual por geometría. Es reducir trabajo perceptivo redundante sin degradar la precisión del grounding ni aceptar identidades ambiguas. La hipótesis se somete a comparación emparejada contra procesamiento completo con los mismos modelos y a ablaciones de incrementalidad, fusión, caché y tracking. Los errores de identidad, abstenciones y latencia extrema son resultados principales, no excepciones omitidas.

## 1.3 Problema de investigación

El control de interfaces requiere resolver al menos cuatro preguntas: qué se observa, qué significa, si sigue siendo el mismo elemento y si es seguro actuar ahora. Mezclarlas en una llamada monolítica complica atribuir errores y optimizar componentes. Una representación con IDs puede reducir la obligación del planificador de emitir coordenadas, pero no corrige automáticamente detecciones equivocadas, botones repetidos, textos ambiguos o cambios de contenido.

La conversación identifica tres oportunidades: redundancia temporal, asociación de textos con controles y reutilización de apariencias de iconos [F0, pp. 1–20]. Esas oportunidades son dependientes de la carga: un vídeo, una animación, un scroll o un cambio de ventana pueden invalidar grandes áreas. Por ello, el sistema debe poder elegir percepción completa y no asumir que toda interacción altera sólo el 5% de la pantalla.

**Pregunta principal:** ¿en qué cargas y configuraciones un motor incremental estructurado reduce la latencia de percepción respecto al procesamiento completo, manteniendo la precisión de acciones aceptadas y una cobertura útil?

Preguntas derivadas: ¿cuánto aporta cada algoritmo? ¿Cómo cambia el resultado con DPI, temas, formularios densos y virtualización? ¿Qué umbrales de abstención evitan asociaciones erróneas sin volver inútil el agente? ¿Cuál es el coste total hasta verificar una acción, incluyendo al planificador y la aplicación?

## 1.4 Objetivos

**Objetivo general:** diseñar e implementar un núcleo de percepción y grounding local, desacoplado del proveedor del agente, y evaluar su corrección temporal, calidad perceptiva y rendimiento en un dominio de escritorio explícito.

**Objetivos específicos:** formalizar contratos de frames, elementos, snapshots y acciones; desarrollar los tres bloques originales con las correcciones de auditoría; construir una integración real en Windows; comparar contra baselines reproducibles; publicar artefactos, configuraciones, fallos y limitaciones. La ausencia de un VLM generativo en la ruta rápida se trata como restricción del diseño inicial, no como prueba de superioridad universal.

## 1.5 Antecedentes y posición de la propuesta

**OmniParser.** Lu y colaboradores estudian el parsing de pantallas mediante detección de regiones interactuables y descripción semántica especializada [S11]. Es un antecedente directo de separación de percepción y grounding. Ommatix propone reutilización temporal y contratos de acción versionados; no presenta la descomposición detector/OCR/semántica como invención propia. La implementación y pesos concretos se examinan por separado [S12].

**Screen2AX.** Este trabajo reconstruye metadatos jerárquicos de accesibilidad a partir de capturas de macOS [S13–S14]. Aporta un contraste con la representación plana del PDF. Se conserva un modelo interno de relaciones aunque el payload del agente sea compacto. Sus resultados en macOS no se transfieren a Windows sin evaluación; tampoco se asume que su detector reconozca todos los roles del MVP.

**OSWorld y ScreenSpot-Pro.** El primero proporciona una referencia de evaluación de tareas completas en entornos de ordenador [S15]; el segundo aborda grounding en interfaces profesionales de alta resolución [S16]. Sirven para distinguir acierto de un punto, continuidad de identidad y éxito de un flujo. No se citan cifras históricas como estado actual del arte. El estudio inicial usará un subconjunto propio controlado y documentará por qué no representa esos benchmarks completos.

**R-tree y R*-tree.** Guttman desarrolla una estructura dinámica para búsqueda espacial de objetos con extensión; Beckmann y colaboradores estudian la estrategia R*-tree [S17–S18]. Se usa el índice para enumerar candidatos geométricos, no para decidir semántica. La implementación candidata `rstar` soporta consultas y carga masiva [S19]. El solapamiento y la cantidad de resultados forman parte del coste; no hay una garantía universal de microsegundos.

**Tracking y hashing.** SORT es un antecedente de asociación temporal basada en detecciones [S20]. Su dominio y métricas no validan identidades de GUI, donde el contenido de una fila puede cambiar sin moverla. Los hashes perceptuales permiten recuperar apariencias parecidas [S21]; el significado depende además de contexto o etiquetas externas confiables. El descriptor enmascarado de este dossier es una propuesta para preservar información de soporte, no una nueva familia validada de reconocimiento semántico.

## 1.6 Modelo del sistema

Se representa un frame como `F_t=(pixels, width, height, row_stride, pixel_format, captured_at, geometry_epoch)`. `geometry_epoch` cambia ante transformaciones incompatibles. El estado confirmado es `S_t=(snapshot_id, captured_at, elements, relations, dirty_pending, context)`. Cada elemento tiene ID de sesión, revisión, bbox, evidencias y accionabilidad. Las cajas usan intervalos semiabiertos `[xmin,xmax) × [ymin,ymax)`.

Una observación produce un snapshot; una acción referencia ese snapshot y una revisión. El agente no conserva autoridad indefinida sobre el ID. El motor sólo permite acciones si la entidad sigue vigente, el contexto coincide y la evidencia suficiente supera los filtros de seguridad. La geometría determina candidatos, la política determina permisibilidad y la observación posterior aporta evidencia de efecto.

**Límite epistemológico:** cajas y texto no equivalen a intención, semántica ni affordance. Un elemento puede observarse correctamente y aun así no ser clicable. Una marca de confianza del detector no es automáticamente una probabilidad calibrada de que un clic logre el objetivo.

## 1.7 Fundamentos matemáticos

### Percepción incremental

Para cada píxel válido p se define `d_t(p)=I(max_c |F_t(p,c)-F_b(p,c)| > epsilon)`, con c limitado a los canales de color y b al estado base confirmado. Una celda está sucia si contiene algún píxel modificado que no pertenezca a una máscara confiable y explícita. `epsilon=0` es la referencia conservadora. Los umbrales y máscaras se evalúan como optimizaciones, no como axiomas de irrelevancia.

Para una región inicial R, el cierre contextual itera `R_next = clip(expand_or_union(R, supports_intersecting(R)))` hasta que no crezca. Debe incluir las dependencias textuales/visuales de las relaciones y fusionar regiones solapadas. Si el cierre o el número de regiones supera el presupuesto, se procesa la pantalla completa. La ejecución fallida conserva las regiones pendientes.

### Contención y asignación

Se mantiene el nombre IoC del documento para `IoC(T,U)=area(T intersect U)/area(T)`, si el texto tiene área positiva. Para texto 25×14 contenido en botón 300×40, IoC=1, mientras IoU=350/12000≈0,029 [D; F0, p. 5]. Esta diferencia justifica usar una métrica asimétrica para pertenencia; no convierte en seguro un umbral universal de 0,75.

Para una etiqueta externa se propone `C(U,T)=w_d*d_edge/s + w_a*align/s + w_g*group_penalty + w_o*occlusion_penalty`, donde s es una escala local positiva. Se rechazan direcciones incompatibles y se contempla una opción de no asignación. Los pesos se calibran en desarrollo; se conserva una muestra de test no usada para elegirlos. Se evita que el orden de entrada determine las asociaciones.

### Descriptor perceptual y soporte

Para cada una de las 64 transiciones de la imagen 9×8 se guarda un signo `b_i=I(delta_i>0)` y una máscara `m_i=I(|delta_i|>=tau)`. Con `M=m_q AND m_p`, `n=popcount(M)` y `h=popcount((b_q XOR b_p) AND M)`, la distancia normalizada a inversión ideal es `min(h,n-h)/n`, siempre que haya soporte común suficiente y máscaras compatibles. Si n=0 o el soporte es escaso, el resultado es desconocido.

Bajo la inversión ideal `Y'=255-Y`, las magnitudes se conservan y los signos válidos se invierten; por ello la distancia puede ser cero [D]. Un tema real puede alterar sombras, grosor, antialiasing, contorno o icono: esa propiedad algebraica no demuestra invariancia a todos los temas.

### Identidad temporal

Un track nuevo recibe un ID no reutilizable dentro de la sesión. Para mantenerlo, un matcher combina contexto de ventana/grupo, rol compatible, similitud visual/textual y geometría. La identidad no es una función de la celda. En una tabla virtualizada, la clave semántica de la fila o la detección de sustitución importa más que la posición. La abstención es un resultado correcto ante una asociación no identificable.

## 1.8 Hipótesis falsables y variables

**H1, eficiencia:** con modelos y calidad comparables, el modo incremental reduce al menos 30% la mediana de latencia de percepción en secuencias etiquetadas de baja modificación respecto a full-frame. Es una meta de investigación [P], no un resultado. Se reportará el intervalo de confianza de la diferencia/razón emparejada.

**H2, no inferioridad de grounding:** la diferencia de precisión de acciones aceptadas no es inferior a −1 punto porcentual frente al baseline, y la cobertura no cae más de 3 puntos. El tamaño de muestra se determinará tras un piloto según tasa base y dependencia entre secuencias; no se presume que 150 tareas prueben márgenes estrechos.

**H3, integridad temporal:** la combinación de tracking y precondiciones evita ejecutar referencias obsoletas en la suite adversarial definida. El resultado se expresa como fallos observados y tasa con intervalo, no como inmunidad universal.

**H4, caché:** el caché contextual reduce llamadas de clasificación de apariencias conocidas sin aumentar la aceptación de iconos desconocidos más allá del umbral fijado antes del test. Se evalúan por separado tema ideal invertido, temas reales e iconos fuera de distribución.

Variables independientes: modo de actualización, configuración de halo, asignador, descriptor, DPI, idioma, tema y workload. Variables dependientes: latencia, trabajo procesado, precisión, cobertura, cambios incorrectos de ID, éxito de tarea, memoria y fallos de seguridad. Control: hardware, pesos, versiones, secuencias y configuración del planificador.

## 1.9 Metodología y validez

El proceso combina construcción de artefacto, pruebas de invariantes y evaluación experimental. Primero se reproduce cada defecto lógico en casos mínimos. Luego se verifica equivalencia del núcleo optimizado contra un oráculo exacto. Después se integran modelos y APIs nativas. Sólo al final se mide éxito de tareas con un planificador fijo y un verificador independiente.

La separación de conjuntos se hace por aplicación/familia de pantalla y secuencia, no por frames aleatorios contiguos. Las mismas apariencias no deben filtrarse entre entrenamiento del caché y test de iconos desconocidos. Dos anotadores revisarán roles, labels, bbox y accionabilidad; las discrepancias se adjudicarán y se reportará acuerdo. Los evaluadores no usarán el JSON del propio motor como ground truth.

Se alterna el orden de baselines para reducir sesgos térmicos y de caché. Warm-up, primera carga, caché caliente y reinicio se reportan por separado. Las mediciones de GPU deben sincronizar el final del trabajo; cronometrar sólo el encolado no mide inferencia completa. Para p99 se necesitan muchas observaciones de secuencias diversas; se proponen al menos 10.000 muestras temporales, reconociendo su autocorrelación.

Amenazas principales: corpus demasiado pequeño, anotación ambigua, aplicaciones fuera de dominio, dependencias temporales, degradación por cuantización, cambios del planificador y sesgo de abstención. Se publica precisión junto con cobertura y se conservan fallos y timeouts en los denominadores.

## 1.10 Aporte esperado, límites y resultados pendientes

Aporte esperado [P]: una arquitectura con contratos temporales explícitos; algoritmos incrementales reproducibles; evaluación de cuándo un caché visual es suficiente; y una metodología que separa rendimiento del core, percepción y acción. La novedad científica dependerá de comparación con antecedentes, implementación y resultados, no del hecho de reunir componentes conocidos.

No se afirma resolver cualquier software, reconocer semántica ilimitada, controlar escritorios protegidos ni ejecutar tareas instantáneamente. La latencia de decisión del LLM, la red y la aplicación siguen existiendo. Un sistema local de percepción puede enviar texto sensible a un agente remoto, de modo que “local” no equivale a confidencialidad integral.

**Resultados disponibles:** contraejemplos y pruebas sintéticas descritos en el informe de validación. **Resultados pendientes:** tablas de precisión/recall, curvas precisión-cobertura, p50/p95/p99 en hardware destino, comparaciones de modelos, estudio de consumo y éxito E2E. No se completan esas tablas con valores inventados.
