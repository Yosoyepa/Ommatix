# language: es
Característica: Enumeración de candidatos equivalente al recorrido
  El índice R*-tree reduce la búsqueda de candidatos sin cambiar la semántica de la fusión ni
  autorizar acciones (ADR-0027). El recorrido completo es la referencia y el valor por defecto.
  Los escenarios comparan ambas variantes sobre la misma entrada; el índice sólo cubre
  propietarios y obstáculos del corredor, no la búsqueda de palabras ni el cierre de soportes.

  @BDD-41 @RF-06 @IDX-03
  Escenario: BDD-41 Un texto pertenece al control específico
    Dado un panel con un botón y una palabra contenida en ambos
    Cuando comparo la fusión por recorrido con la variante indexada
    Entonces ambas asignan la palabra al botón
    Y mantienen las mismas affordances

  @BDD-42 @RF-06 @IDX-02
  Escenario: BDD-42 El umbral cero conserva el contrato
    Dado un umbral IoC igual a cero y controles disjuntos de la palabra
    Cuando selecciono propietarios con la variante indexada
    Entonces utilizo el recorrido completo para esa consulta
    Y obtengo la misma decisión de referencia

  @BDD-43 @RF-06 @IDX-01
  Escenario: BDD-43 Un borde compartido no bloquea el corredor
    Dado un control que sólo toca el borde del corredor
    Cuando el índice lo devuelve como candidato
    Entonces el predicado de solapamiento de área positiva lo rechaza

  @BDD-44 @RF-06 @IDX-04
  Escenario: BDD-44 Los propietarios equivalentes conservan la abstención
    Dado dos controles de áreas iguales que superan el umbral
    Cuando comparo ambas variantes
    Entonces ambas se abstienen de asignar propietario

  @BDD-45 @RF-06 @IDX-05
  Escenario: BDD-45 Un segundo candidato impide una etiqueta confiada
    Dado dos etiquetas válidas cuyos costes no alcanzan el margen de separación
    Cuando consulto los candidatos
    Entonces no trunco la consulta al vecino más cercano
    Y ambas variantes devuelven ambigüedad

  @BDD-46 @RF-06 @IDX-06
  Escenario: BDD-46 El panel contenedor no es un obstáculo
    Dado un panel que contiene al control y atraviesa su corredor
    Cuando evalúo obstáculos mediante el índice
    Entonces aplico las exclusiones originales
    Y el panel no bloquea la etiqueta

  @BDD-47 @RF-10 @IDX-09
  Escenario: BDD-47 La versión de evidencia no coincide
    Dado un índice construido sobre una revisión anterior
    Cuando intento resolver posiciones sobre la evidencia nueva
    Entonces rechazo esa vista o la reconstruyo antes de consultarla
    Y no publico referencias mezcladas

  @BDD-48 @RF-04 @IDX-10
  Escenario: BDD-48 Fallo de la observación candidata
    Dado un snapshot confirmado y una nueva observación con fallo
    Cuando abandono el candidato
    Entonces conservo coherentes la evidencia y el índice confirmado
    Y mantengo pendientes y bloqueo de acciones conforme al contrato

  @BDD-49 @RF-06 @IDX-08
  Escenario: BDD-49 Una etiqueta alta no desaparece
    Dado una palabra válida cuya altura supera la del control
    Cuando se consulta el propietario del texto con el índice
    Entonces la caja de la palabra es la ventana de consulta y el control se devuelve
    Y conservo el IoC exacto del recorrido

  @BDD-50 @RF-03 @IDX-11
  Escenario: BDD-50 El presupuesto de cierre conserva su significado
    Dado una cadena de soportes sensible al orden bajo presupuesto finito
    Cuando se evalúe el cierre de soportes con su presupuesto
    Entonces comparo tanto el cierre como la razón de fallback contra la referencia
    Y no considero suficiente alcanzar el mismo punto fijo sin presupuesto

  @BDD-51 @RF-03 @IDX-12
  Escenario: BDD-51 Las relaciones se activan por sus extremos
    Dado una región en el hueco entre extremos de una relación
    Cuando consulto los soportes de la relación
    Entonces no activo la relación sólo por su envolvente

  @BDD-52 @RF-06 @IDX-13
  Escenario: BDD-52 El solapamiento extremo no justifica perder candidatos
    Dado muchos controles superpuestos
    Cuando el índice no logra reducir la búsqueda
    Entonces evalúo todos los candidatos requeridos o uso la referencia
    Y no sacrifico equivalencia para cumplir una meta de tiempo
