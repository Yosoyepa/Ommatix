# 11. Referencias y procedencia

## 11.1 Fuente de requisitos

**[F0]** Conversación aportada por el usuario: *Arquitectura de Computación Visual para Agentes*, PDF de 45 páginas. Se conserva íntegra fuera del repositorio. No es una publicación revisada por pares ni una medición independiente. Las páginas citadas permiten contrastar cada cambio.

## 11.2 Fuentes públicas primarias

Consulta: **13 de octubre de 2026**. Se indican los límites de lo que cada fuente respalda. Las páginas de documentación son mutables; fijar versiones y artefactos antes de implementar. No se descargan ni redistribuyen papers, pesos o código de terceros dentro de este dossier.

### [S01] Especificación MCP 2026-07-28: cambios

Model Context Protocol. 2026. https://modelcontextprotocol.io/specification/2026-07-28/changelog

**Uso y límite:** Protocolo moderno sin initialize; metadatos por petición. No prueba compatibilidad de un cliente concreto.

### [S02] Transporte stdio, revisión 2026-07-28

Model Context Protocol. 2026. https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio

**Uso y límite:** Encuadre por líneas y separación stdout/stderr; no aporta una latencia universal.

### [S03] Versioning and Compatibility

Model Context Protocol. 2026. https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning

**Uso y límite:** Distinción entre clientes modernos y legacy; error de versión no soportada.

### [S04] Tools y Schema Reference

Model Context Protocol. 2026. https://modelcontextprotocol.io/specification/2026-07-28/server/tools

**Uso y límite:** Descubrimiento de herramientas y resultados estructurados. Complemento: https://modelcontextprotocol.io/specification/2026-07-28/schema

### [S05] Lifecycle, revisión 2025-11-25

Model Context Protocol. 2025. https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle

**Uso y límite:** Contrato legacy con initialize/initialized; no mezclar con el moderno.

### [S06] Desktop Duplication API

Microsoft. 2021. https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api

**Uso y límite:** Dirty rects, move rects, cursor y rotación. Los movimientos se procesan antes que las regiones sucias.

### [S07] SendInput function

Microsoft. 2021. https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput

**Uso y límite:** Entrega de eventos y restricciones UIPI; no confirma efectos de negocio.

### [S08] High DPI Desktop Application Development on Windows

Microsoft. 2026. https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows

**Uso y límite:** Espacios de coordenadas y Per-Monitor V2. La conversión debe probarse.

### [S09] DirectML Execution Provider

ONNX Runtime. s. f.. https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html

**Uso y límite:** Restricciones de sesión, shapes y estado de mantenimiento de DirectML; WinML es una vía de evolución.

### [S10] Execution Providers

ONNX Runtime. s. f.. https://onnxruntime.ai/docs/execution-providers/

**Uso y límite:** Abstracción del proveedor de ejecución; no garantiza cero copias ni aceleración de cualquier modelo.

### [S11] OmniParser for Pure Vision Based GUI Agent

Lu, Y.; Yang, J.; Shen, Y.; Awadallah, A.. 2024. https://arxiv.org/abs/2408.00203

**Uso y límite:** Detección de regiones interactuables y descripción semántica especializadas. Antecedente, no una prueba del motor propuesto.

### [S12] OmniParser: repositorio oficial y licencias de pesos

Microsoft. s. f.. https://github.com/microsoft/OmniParser

**Uso y límite:** Distingue icon_detect_v3, basado en YOLOv9 MIT, de detectores Ultralytics anteriores con AGPL. Verificar el artefacto exacto.

### [S13] Screen2AX: Vision-Based Approach for Automatic macOS Accessibility Generation

Muryn, V. y colaboradores. 2025. https://arxiv.org/abs/2507.16704

**Uso y límite:** Reconstrucción visual de jerarquías de accesibilidad en macOS; no es un detector nano universal de Windows.

### [S14] Screen2AX: implementación y modelos

MacPaw. s. f.. https://github.com/MacPaw/Screen2AX

**Uso y límite:** Publica detectores YOLOv11l de elementos/grupos. Destilarlos a nano sería trabajo adicional.

### [S15] OSWorld: Benchmarking Multimodal Agents for Open-Ended Tasks in Real Computer Environments

Xie, T. y colaboradores. 2024. https://arxiv.org/abs/2404.07972

**Uso y límite:** Referencia histórica para evaluación de tareas con verificación ejecutable, no leaderboard actual.

### [S16] ScreenSpot-Pro: GUI Grounding for Professional High-Resolution Computer Use

Li, K. y colaboradores. 2025. https://arxiv.org/abs/2504.07981

**Uso y límite:** Evaluación de grounding en interfaces profesionales de alta resolución; no mide estabilidad temporal por sí sola.

### [S17] R-trees: A Dynamic Index Structure for Spatial Searching

Guttman, A.. 1984. https://dl.acm.org/doi/10.1145/971697.602266

**Uso y límite:** Fundamento del índice espacial de rectángulos. DOI: 10.1145/971697.602266.

### [S18] The R*-tree: An Efficient and Robust Access Method for Points and Rectangles

Beckmann, N.; Kriegel, H.-P.; Schneider, R.; Seeger, B.. 1990. https://sigmodrecord.org/1990/06/06/the-r-tree-an-efficient-and-robust-access-method-for-points-and-rectangles/

**Uso y límite:** Optimización del índice R*-tree. DOI: 10.1145/93597.98741.

### [S19] rstar: documentación de RTree

GeoRust. s. f.. https://docs.rs/rstar/latest/rstar/struct.RTree.html

**Uso y límite:** API, carga masiva e indexación. El coste real depende de solapamiento y cantidad de resultados.

### [S20] Simple Online and Realtime Tracking

Bewley, A. y colaboradores. 2016. https://arxiv.org/abs/1602.00763

**Uso y límite:** Antecedente de asociación temporal. No se trasladan sus cifras de rendimiento ni su dominio de objetos al de GUI.

### [S21] ImageHash

Buchner, J. y colaboradores. s. f.. https://github.com/JohannesBuchner/imagehash

**Uso y límite:** Hashing perceptual para similitud visual, no identificación semántica universal.

### [S22] Thread Safety, manual 2.3

NumPy. s. f.. https://numpy.org/doc/2.3/reference/thread_safety.html

**Uso y límite:** Muchas operaciones liberan el GIL; matiza la comparación Python/Rust.

### [S23] rmcp: SDK oficial de Rust

Model Context Protocol / mantenedores del SDK. s. f.. https://docs.rs/rmcp

**Uso y límite:** Candidato al adaptador MCP. Fijar versión y probar interoperabilidad; no se compiló aquí.

### [S24] Gherkin Reference y Localisation

Cucumber. s. f.. https://cucumber.io/docs/gherkin/reference/

**Uso y límite:** Sintaxis BDD. Un archivo feature sin step definitions no es una ejecución automatizada.

## 11.3 Mapa de procedencia de decisiones

El núcleo de requisitos y la intención del usuario proceden de F0 pp. 1–5 y 20–27. Las propuestas algorítmicas originales están en pp. 5–20. Los riesgos del generador se contrastan con pp. 28–45. La revisión de protocolo se apoya en S01–S05; adaptadores Windows en S06–S08; inferencia y licencias en S09–S14; evaluación y teoría en S15–S22.

La probabilidad de colisión, los contraejemplos de celdas/dHash/orden de lectura y las correcciones de commit/halo son análisis propios [D]. Windows como primer MVP, presupuestos de aceptación, corpus y roadmap revisado son propuestas [P]. Los resultados locales [V] aparecen sólo en el informe de validación y sus logs.

## 11.4 Qué no quedó demostrado

No se verificaron latencias del PDF, recall universal, seguridad absoluta, reducción porcentual de tokens, calidad de pesos en Windows, adecuación de un detector nano destilado ni compatibilidad con todos los agentes. La literatura consultada permite situar la idea y corregir contratos; no llena esas lagunas experimentales.
