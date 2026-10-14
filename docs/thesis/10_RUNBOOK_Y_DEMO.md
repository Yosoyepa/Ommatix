# 10. Guía de uso del paquete y demostración del MVP

## 10.1 Lectura recomendada

Empezar por la auditoría 00 y la especificación de alcance 03. Consultar SDD 02 y algoritmos 04 al implementar. Usar ADR, contratos, Gherkin y matrices como control de cambios y aceptación. La tesis 01 y el protocolo 09 sustentan la evaluación; no son un informe de benchmarks ya obtenidos.

Los Markdown de `docs/thesis/` y las ADR de `docs/adr/` son las fuentes editables. La edición consolidada en Word y PDF y la conversación aportada [F0] se conservan fuera del repositorio. Las referencias públicas están catalogadas con fecha y límites de evidencia.

## 10.2 Ejecutar las comprobaciones locales entregadas

Requisito: Python 3.11 o superior. Desde `docs/thesis/`: `python referencia_ejecutable/run_tests.py`. El comando no controla el escritorio ni necesita internet/modelos. Guarda log e informe en `referencia_ejecutable/resultados/`.

`python validar_paquete.py` valida schemas y ejemplos con `jsonschema`, inspecciona trazabilidad y hace un chequeo estructural de Gherkin. La ausencia de una dependencia se informa como validación no disponible; no se convierte en aprobación. No se incluye un servidor MCP productivo ejecutable.

## 10.3 Preparar la demo real, pendiente de implementar

Usar una cuenta de prueba sin datos personales, aplicaciones permitidas y directorio temporal. Registrar hardware, versiones y hashes. Instalar runtime/modelos aprobados. Arrancar primero en dry-run; comprobar permisos, monitor, DPI y modo de transporte. No elevar privilegios para resolver una denegación de entrada.

El operador confirma la allowlist y habilita actuación local. Se muestra un indicador de control y se ensaya la parada antes de ejecutar tareas. Los casos de prueba sólo usan datos ficticios y archivos desechables. Se conserva un estado inicial restaurable entre repeticiones.

## 10.4 Guion de aceptación de extremo a extremo

**Formulario local:** observar campos vacíos y labels; seleccionar por ID; introducir un texto ficticio; seleccionar opción; enviar a una página local sin red y comprobar un resultado explícito. La UI inserta un popup entre observación y acción para comprobar rechazo y reobservación.

**Editor de texto:** escribir una frase de prueba en un documento nuevo, guardar dentro del directorio temporal y verificar el contenido del archivo independientemente del motor. No usar documentos reales ni diálogos que impliquen sobrescritura sin control.

**Explorador:** abrir una carpeta de fixture, navegar a un subdirectorio y seleccionar un archivo sin borrarlo. Cambiar el orden de filas entre observación y acción y verificar que no se confunda el ID. La virtualización que el matcher no pueda resolver debe provocar abstención.

En cada flujo, capturar request_id, snapshot, revisión, motivo de decisión, recibo de entrega y verificación. Redactar valores sensibles antes de conservar evidencias. Repetir según el protocolo de aceptación y contar interrupciones como tales.

## 10.5 Diagnóstico y recuperación

`CAPTURE_UNAVAILABLE`: comprobar permiso, sesión y pérdida de DXGI; reconstruir captura y epoch. `INFERENCE_FAILED`: conservar región pendiente y bloquear acción hasta reobservar. `STALE_SNAPSHOT` o `STALE_TARGET`: observar de nuevo y elegir explícitamente; no reanclar. `POLICY_DENIED`: revisión local por el operador, no autorización del agente. `INDETERMINATE_EFFECT`: inspeccionar estado actual antes de reintentar.

Si una parada ocurre tras entregar eventos, informar posible efecto y pedir inspección. Un rollback del estado interno no deshace un clic ya enviado. Cerrar el proceso limpia buffers y deshabilita nuevos eventos.

## 10.6 Evidencia que debe acompañar una release funcional

Binario y dependencias, manifiesto de hashes/licencias, configuración saneada, instrucciones de instalación, corpus de prueba autorizado, resultados y limitaciones. No marcar versión 1.0 productiva sólo porque exista este dossier. El nombre de paquete `v1.0` se refiere a la documentación revisada.
