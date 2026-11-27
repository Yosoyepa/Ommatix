# language: es
Característica: Varios agentes sobre un motor con un arrendamiento de entrada
  Varios agentes comparten un motor a través del intermediario del adaptador MCP (ADR-0039,
  propuesta). Un solo arrendamiento cubre el ciclo de observar y actuar: se pide con el
  argumento `lease` de `observe_screen` y `_meta` dice dónde está cada cliente. Sólo quien lo
  tiene entrega. Mientras un cliente lo tiene, sólo su observación captura y publica, y los
  demás reciben la página ya publicada; cuando nadie lo tiene, la observación de cualquiera
  publica. Cada cliente actúa bajo su propia concesión (ADR-0040, propuesta). Los escenarios
  corren sobre el escritorio simulado y un doble del actuador. Desde BDD-68, los agentes se
  conectan con el saludo de un tique (ADR-0043, propuesta) por flujos en memoria y sus
  concesiones salen del archivo de concesiones del operador, leído en cada llamada. El ejecutable
  sirve el intermediario por un tubo con nombre sólo en Windows, compilado con `broker-pipe` y con
  el interruptor encendido; el tubo se prueba en el adaptador MCP y en el ejecutable, no aquí.

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

  @BDD-68 @RF-20 @IT-23
  Escenario: BDD-68 Un agente conectado con un tique actúa bajo su concesión
    Dado un motor con el intermediario y un archivo de concesiones que da a un tique permiso para entregar y leer valores
    Cuando un agente se conecta presentando ese tique y observa la pantalla pidiendo valores
    Entonces la página muestra los valores
    Y su acción real se entrega

  @BDD-69 @RF-20 @IT-24
  Escenario: BDD-69 Un agente conectado sin tique observa sin valores y no entrega
    Dado un motor con el intermediario y un archivo de concesiones que da a un tique permiso para entregar y leer valores
    Cuando un agente se conecta sin tique y observa la pantalla pidiendo valores
    Entonces la página no muestra los valores
    Y su clic real se rechaza con POLICY_DENIED sin entregar ningún evento

  @BDD-70 @RF-20 @IT-25
  Escenario: BDD-70 Un tique presentado dos veces
    Dado un motor con el intermediario y un archivo de concesiones que da a un tique permiso para entregar y leer valores
    Y un agente conectado con ese tique
    Cuando otro agente presenta el mismo tique
    Entonces su conexión se rechaza sin respuesta ni concesión
    Y el tique sigue rechazado después de que el primero se va

  @BDD-71 @RF-20 @IT-23
  Escenario: BDD-71 Una concesión retirada del archivo entre dos acciones
    Dado un motor con el intermediario y un archivo de concesiones que da a un tique permiso para entregar y leer valores
    Y un agente conectado con ese tique que ha entregado una acción real
    Cuando el operador retira la concesión del archivo
    Entonces su siguiente observación no muestra valores
    Y su acción real sobre esa página se rechaza con POLICY_DENIED sin entregar ningún evento nuevo
