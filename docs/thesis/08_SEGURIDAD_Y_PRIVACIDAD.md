# 08. Modelo de amenazas y controles

## 8.1 Activos y fronteras

Los activos son pixels, texto OCR, datos de formularios, archivos accesibles, autoridad para inyectar entrada y trazas del agente. El contenido de la pantalla puede ser malicioso; los clientes MCP no deben poder modificar la política local. Los pesos y prototipos también son entradas de la cadena de suministro. Este modelo de amenazas es una ampliación propuesta [P] del PDF, no una propiedad ya implementada.

Fronteras: agente / adaptador MCP; aplicación / captura/percepción; core / actuador; configuración/modelos / runtime; almacenamiento/logs / otros procesos. La interfaz stdio reduce superficie de red, pero no autoriza confiar en cualquier proceso que lance el motor.

## 8.2 Controles mínimos

**Consentimiento y alcance:** ejecución deshabilitada por defecto, indicador de control activo, allowlist de aplicaciones y ventana, límites de acciones y parada local. La parte de ventana (ADR-0020, propuesta) es opcional y está apagada por defecto: `[safety] allowed_window_classes` lista, por aplicación, las únicas clases de ventana que reciben entrada real (para `explorer.exe`, `CabinetWClass`, y no el escritorio ni la barra de tareas que el mismo proceso también posee) y `[safety] window_pin_file` registra la única ventana permitida; el motor comprueba la ventana en primer plano y la ventana bajo el punto de la acción en la puerta y de nuevo justo antes de entregar, y rechaza lo que el actuador no pueda atestiguar. El agente no puede habilitar su propia autorización ni convertir un campo `confirmation:true` en permiso de usuario. El MVP no admite operaciones destructivas o con consecuencias externas fuera del entorno de prueba.

**Integridad temporal:** revisión y snapshot obligatorios, epochs geométricos, revalidación de foco/visibilidad, rechazo de targets desconocidos y no reutilización de IDs. La política no reancla por parecido. Los efectos indeterminados se reconcilian antes de otro intento.

**Entradas no confiables:** límites de tamaño y profundidad, JSON Schema, enums estrictos, enteros acotados y rechazo de coordenadas no finitas. Texto de pantalla se entrega como dato con procedencia; nunca se ejecuta como código, shell o regla. Las anotaciones de herramientas son descriptivas, no sustituyen la autorización.

**Privacidad:** no guardar screenshots ni texto de `type` por defecto. Logs con IDs locales, códigos y tiempos; redacción antes de persistir. Datos usados en demos son ficticios. Un agente remoto puede recibir labels y valores; se requiere política de minimización y opción de ocultar contenidos sensibles. Percepción local no asegura residencia total de datos.

**Cadena de suministro:** hash y versión exactos de modelos, librerías y caché; origen verificado; licencias separadas de código y pesos. No cargar artefactos externos arbitrarios como modelos confiables. Cambiar normalización o modelo invalida las entradas afectadas del caché.

## 8.3 Riesgos residuales

El SO puede cambiar entre precheck y evento; el proceso no puede congelar toda la UI. Un detector puede confundir dos controles similares; un score alto no garantiza seguridad. Una respuesta de entrada entregada puede no producir el efecto esperado. Un proceso con privilegios del usuario podría acceder a su pantalla por otros medios; el alcance no es un sandbox completo del SO.

Se reduce exposición mediante tareas de bajo riesgo, allowlist estrecha, tiempos de frescura, abstención y supervisión. No se declara “cero riesgo” porque una suite tenga cero incidentes.

## 8.4 Retención y respuesta a incidentes

Predeterminado propuesto: buffer de frames sólo en memoria y sobrescritura al cerrar; caché persistente opcional sin crops crudos; logs técnicos rotativos con TTL de siete días, ajustable a política del usuario. No hay subida de telemetría. El usuario debe poder borrar estado y caché.

Ante entrada inesperada: parada inmediata, deshabilitar actuador, conservar sólo trazas mínimas autorizadas, registrar versión/snapshot/política, identificar si existió entrega y revisar el fallo antes de reactivar. Ante fallo de verificación no se ejecuta una compensación automática que pudiera empeorar el estado.

## 8.5 Casos de prueba de seguridad

Se prueban inyección de instrucciones mediante OCR, UI cubierta por popup, ventana equivocada, ID de otra sesión, solicitud sobredimensionada, target retirado, doble envío del mismo request_id, modelo alterado, cambio de DPI, salida del cliente y parada durante una secuencia. La matriz de riesgos asigna mitigación, prueba y dueño pendiente de designación.
