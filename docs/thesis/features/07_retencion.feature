# language: es
Característica: Retención y borrado de estado
  El motor guarda su estado sólo en memoria y lo borra al terminar la sesión. Tras borrarlo
  no emite identificadores que ya emitió (capítulo 08 §8.4).

  @BDD-39 @RF-16 @SEC-07
  Escenario: BDD-39 Fin de sesión sin estado conservado
    Dado un agente con una observación publicada y un elemento confirmado
    Cuando la sesión MCP termina
    Entonces el motor no conserva ninguna observación y rechaza una acción sobre el elemento anterior sin entregar nada
    Y una observación posterior no reutiliza ningún identificador de elemento ni de snapshot anterior
