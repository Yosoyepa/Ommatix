# Glossary / Glosario

| English | Español | Meaning / Significado |
|---|---|---|
| Frame | Frame / cuadro | One RGBA8 capture of the screen. / Una captura RGBA8 de la pantalla. |
| Cell (ommatidium) | Celda (omatidio) | Fixed-size square of the grid, fingerprinted independently. / Cuadrado de tamaño fijo de la cuadrícula, con huella independiente. |
| Grid signature | Firma de cuadrícula | Per-cell hashes of one frame. / Hashes por celda de un frame. |
| Dirty cell | Celda sucia | Cell whose hash changed since the previous frame. / Celda cuyo hash cambió respecto al frame anterior. |
| Dirty region | Región sucia | Bounding box of 4-connected dirty cells, expanded to whole elements. / Caja envolvente de celdas sucias conectadas, expandida a elementos completos. |
| Container | Contenedor | Element that encloses a change without sharing an edge with it (a window around a button); not re-analysed for it. / Elemento que encierra un cambio sin compartir un borde con él (una ventana alrededor de un botón); no se re-analiza por él. |
| Context margin | Margen de contexto | Distance up to which a detector groups elements (the gap between glyphs); a change reaches every element within it. / Distancia hasta la que un detector agrupa elementos (el espacio entre glifos); un cambio alcanza a todo elemento dentro de ella. |
| Flat region | Región plana | Connected pixels whose neighbours differ by at most the tolerance; the detector's unit. / Píxeles conectados cuyos vecinos difieren como mucho la tolerancia; la unidad del detector. |
| Surface | Superficie | Flat stretch covering at least half of a screen edge (wallpaper, maximised or snapped window, taskbar); background, never an element. / Tramo plano que cubre al menos la mitad de un borde de la pantalla (fondo, ventana maximizada o acoplada, barra de tareas); es fondo, nunca un elemento. |
| Detection | Detección | Raw detector output (bounds + kind), no identity yet. / Salida cruda del detector, aún sin identidad. |
| Element | Elemento | Detection with a stable id living in the scene. / Detección con id estable dentro de la escena. |
| Scene | Escena | Current set of elements plus their spatial index. / Conjunto actual de elementos y su índice espacial. |
| Spatial index | Índice espacial | Uniform-grid hash from screen buckets to elements. / Hash de cuadrícula uniforme de cubetas de pantalla a elementos. |
| Port | Puerto | Trait the application needs the outside world to implement. / Trait que la aplicación exige al mundo exterior. |
| Adapter (driving / driven) | Adaptador (conductor / conducido) | Code that calls the core (MCP) / that the core calls (capture, vision). / Código que invoca al núcleo / que el núcleo invoca. |
| Tick | Tick | One capture → diff → detect → update cycle. / Un ciclo captura → diff → detección → actualización. |
| Settled | Estabilizada | Screen with no changed cell for a few consecutive frames after an action. / Pantalla sin celdas cambiadas durante varios frames seguidos tras una acción. |
| Display mapping | Mapeo de pantalla | Conversion from frame pixels to OS input coordinates (HiDPI scale, monitor origin). / Conversión de píxeles del frame a coordenadas de entrada del sistema (escala HiDPI, origen del monitor). |
