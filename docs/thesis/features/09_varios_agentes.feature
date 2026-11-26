# language: es
Característica: Varios agentes sobre un motor con un arrendamiento de entrada
  Varios agentes comparten un motor a través del intermediario del adaptador MCP (ADR-0039,
  propuesta). Un solo arrendamiento cubre el ciclo de observar y actuar: se pide con el
  argumento `lease` de `observe_screen` y `_meta` dice dónde está cada cliente. Sólo quien lo
  tiene entrega. Mientras un cliente lo tiene, sólo su observación captura y publica, y los
  demás reciben la página ya publicada; cuando nadie lo tiene, la observación de cualquiera
  publica. Cada cliente actúa bajo su propia concesión (ADR-0040, propuesta). Los escenarios
  corren sobre el escritorio simulado y un doble del actuador; el ejecutable todavía no sirve el
  intermediario.

  @BDD-63 @RF-19 @IT-10
  Escenario: BDD-63 Dos agentes y un arrendamiento
    Dado dos agentes conectados al mismo motor con concesiones que permiten entregar
    Y el primero tiene el arrendamiento y ha observado la pantalla
    Cuando el segundo observa la pantalla
    Entonces recibe la página ya publicada, que _meta marca como compartida con su edad, sin que el motor capture
    Y su acción real se rechaza con POLICY_DENIED sin entregar ningún evento
    Y la acción real del primero sobre su página se entrega

  @BDD-64 @RF-19 @IT-12
  Escenario: BDD-64 El arrendamiento pasa al siguiente agente, que observa antes de actuar
    Dado dos agentes conectados al mismo motor con concesiones que permiten entregar
    Y el primero tiene el arrendamiento y ha observado la pantalla
    Y el segundo pide el arrendamiento con su observación y queda primero en la espera
    Cuando el primero libera el arrendamiento con su observación
    Entonces la acción real del segundo sobre la página anterior se rechaza con STALE_SNAPSHOT sin entregar nada y _meta le dice que tiene el arrendamiento
    Y tras observar de nuevo su acción real se entrega

  @BDD-65 @RF-19 @IT-14
  Escenario: BDD-65 Un agente sin concesión observa sin valores y no entrega
    Dado un agente conectado sin concesión al intermediario
    Cuando observa la pantalla pidiendo valores y solicita un clic real
    Entonces la página no muestra valores
    Y el clic se rechaza con POLICY_DENIED sin entregar ningún evento

  @BDD-66 @RF-19 @IT-16
  Escenario: BDD-66 El reintento de una petición entregada después de perder el arrendamiento
    Dado dos agentes conectados al mismo motor con concesiones que permiten entregar
    Y el primero ha entregado un clic real y luego ha perdido el arrendamiento
    Cuando el primero repite la misma petición con el mismo request_id
    Entonces recibe el mismo resultado entregado y no se entrega ningún evento nuevo
    Y una petición nueva suya se rechaza con POLICY_DENIED

  @BDD-67 @RF-19 @IT-19
  Escenario: BDD-67 Para un agente solo no cambia nada
    Dado un agente solo conectado al intermediario con una concesión igual a la política local
    Cuando observa la pantalla y actúa sin enviar el argumento lease
    Entonces cada respuesta es, byte a byte, la del servidor stdio
    Y ninguna lleva el arrendamiento en _meta
