# Contratos Ommatix 1.0

Estos archivos son especificaciones y ejemplos sintéticos, no capturas de un servidor en funcionamiento. Los schemas validan el payload de las herramientas; no sustituyen toda la especificación MCP.

Perfil moderno: MCP 2026-07-28, `_meta` dentro de `params`, sin initialize; resultados con `resultType: complete`. Perfil legacy opcional: 2025-11-25 con initialize/initialized, habilitado por separado y probado de forma independiente. Los JSONL muestran ambos perfiles y no deben concatenarse como una sola sesión.

`validacion_casos.json` identifica ejemplos válidos e inválidos deliberados. `ejemplos_invalidos/` no son errores de generación: sirven para probar rechazos del schema.

JSON Schema `default` sólo es una anotación. El runtime debe aplicar `dry_run=true` e `include_values=false` cuando se omitan. Ningún default concede permiso de actuación.

La unicidad de target_id, la igualdad entre total_elements y el número real, la vigencia del snapshot, el foco y los permisos requieren validación semántica adicional. El schema no puede acreditarlos.
