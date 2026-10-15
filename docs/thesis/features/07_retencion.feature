# language: es
Característica: Retención y borrado de estado
  El motor guarda su estado sólo en memoria y lo borra al terminar la sesión, o cuando el
  operador lo pide con su archivo de purga. Tras borrarlo no emite identificadores que ya
  emitió (capítulo 08 §8.4).

  @BDD-39 @RF-16 @SEC-07
  Escenario: BDD-39 Fin de sesión sin estado conservado
    Dado un agente con una observación publicada y un elemento confirmado
    Cuando la sesión MCP termina
    Entonces el motor no conserva ninguna observación y rechaza una acción sobre el elemento anterior sin entregar nada
    Y una observación posterior no reutiliza ningún identificador de elemento ni de snapshot anterior

  @BDD-53 @RF-16 @SEC-07
  Escenario: BDD-53 Borrado bajo demanda del operador sin terminar la sesión
    Dado un agente con una observación publicada, un elemento confirmado y el archivo de purga del operador configurado
    Cuando el operador crea el archivo de purga y el agente envía su siguiente petición
    Entonces el motor borra su estado y su caché, y el archivo de purga queda donde el operador lo dejó
    Y la acción sobre el elemento anterior se rechaza con STALE_SNAPSHOT y no entrega ningún evento
    Y una observación posterior no reutiliza ningún identificador de elemento ni de snapshot anterior
