# Glossary / Glosario

| English | Español | Meaning / Significado |
|---|---|---|
| Frame | Frame / cuadro | One RGBA8 capture of the screen. / Una captura RGBA8 de la pantalla. |
| Cell (ommatidium) | Celda (omatidio) | Fixed-size square of the grid, fingerprinted independently. / Cuadrado de tamaño fijo de la cuadrícula, con huella independiente. |
| Grid signature | Firma de cuadrícula | Per-cell hashes of one frame. / Hashes por celda de un frame. |
| Dirty cell | Celda sucia | Cell whose hash changed since the previous frame. / Celda cuyo hash cambió respecto al frame anterior. |
| Dirty region | Región sucia | Bounding box of 4-connected dirty cells, expanded to whole elements. / Caja envolvente de celdas sucias conectadas, expandida a elementos completos. |
| Detection | Detección | Raw detector output (bounds + kind), no identity yet. / Salida cruda del detector, aún sin identidad. |
| Element | Elemento | Detection with a stable id living in the scene. / Detección con id estable dentro de la escena. |
| Scene | Escena | Current set of elements plus their spatial index. / Conjunto actual de elementos y su índice espacial. |
| Spatial index | Índice espacial | Uniform-grid hash from screen buckets to elements. / Hash de cuadrícula uniforme de cubetas de pantalla a elementos. |
| Port | Puerto | Trait the application needs the outside world to implement. / Trait que la aplicación exige al mundo exterior. |
| Adapter (driving / driven) | Adaptador (conductor / conducido) | Code that calls the core (MCP) / that the core calls (capture, vision). / Código que invoca al núcleo / que el núcleo invoca. |
| Tick | Tick | One capture → diff → detect → update cycle. / Un ciclo captura → diff → detección → actualización. |
