# language: es
Característica: Actuación y verificación honestas
  El motor ofrece observaciones y acciones dentro del alcance acordado.
  Estos escenarios requieren step definitions productivas; no están ejecutados en este paquete.

  @BDD-27 @RF-12 @UT-42
  Escenario: BDD-27 Dry-run
    Dado una instancia sin actuador habilitado en modo simulación
    Cuando el agente solicita un clic válido de prueba
    Entonces se devuelve simulated con input_delivered falso

  @BDD-28 @RF-11 @SEC-05
  Escenario: BDD-28 Foco incorrecto
    Dado un campo de texto válido de una ventana permitida
    Cuando otra ventana obtiene el foco antes de escribir
    Entonces se rechaza la entrada sin escribir en la nueva ventana

  @BDD-29 @RF-13 @IT-04
  Escenario: BDD-29 Verificación indeterminada
    Dado una entrada que el actuador reporta como entregada
    Cuando vence el plazo sin comprobar la postcondición
    Entonces se informa indeterminate y no se reintenta automáticamente

  @BDD-30 @RF-14 @IT-05
  Escenario: BDD-30 Solicitud repetida
    Dado un request_id cuyo resultado ya fue registrado
    Cuando se repite exactamente la misma intención
    Entonces se devuelve el resultado registrado sin duplicar eventos

  @BDD-31 @RF-16 @SEC-03
  Escenario: BDD-31 Instrucción dentro de la pantalla
    Dado texto OCR que pide desactivar controles de seguridad
    Cuando el agente solicita elevar su permiso
    Entonces la política local permanece sin cambios

  @BDD-32 @RF-16 @SEC-02
  Escenario: BDD-32 Parada del operador
    Dado una secuencia de prueba autorizada
    Cuando el operador activa la parada local
    Entonces no se envían nuevos eventos y se informa cualquier entrega ya realizada

  @BDD-34 @RF-11 @SEC-05
  Escenario: BDD-34 Teclas sólo con el foco confirmado
    Dado un campo de texto accesible de una ventana permitida
    Cuando el agente escribe y el foco del teclado llega al campo después del clic
    Entonces las teclas se envían después del clic y de confirmar el foco en ese campo

  @BDD-35 @RF-13 @SEC-05
  Escenario: BDD-35 Foco no confirmado
    Dado un campo de texto accesible de una ventana permitida
    Cuando el agente escribe y el foco del teclado no llega al campo antes del plazo
    Entonces no se envía ninguna tecla y se informa indeterminate con el clic entregado

  @BDD-36 @RF-11 @UT-45
  Escenario: BDD-36 Ventana fuera del alcance
    Dado una aplicación permitida cuya política local sólo admite ventanas de una clase
    Cuando el escritorio o la barra de tareas de esa aplicación recibe el clic del agente
    Entonces se rechaza la entrada con POLICY_DENIED y no se entrega ningún evento

  @BDD-37 @RF-11 @UT-45
  Escenario: BDD-37 Ventana no registrada
    Dado una aplicación permitida con una ventana registrada por el operador
    Cuando el agente solicita un clic en otra ventana de la misma clase
    Entonces se rechaza la entrada y sólo la ventana registrada recibe clics

  @BDD-38 @RF-11 @UT-46
  Escenario: BDD-38 Observación caducada
    Dado una observación de una ventana permitida
    Cuando pasa la vigencia configurada y el agente solicita un clic de prueba sobre ella
    Entonces se devuelve STALE_SNAPSHOT con observe_again y no se envía ningún evento
    Y una nueva observación vuelve a permitir el clic de prueba

  @BDD-55 @RF-11 @UT-55
  Escenario: BDD-55 Un cambio fuera de la ventana objetivo no rechaza la acción
    Dado un snapshot observado con el alcance de frescura «ventana objetivo»
    Y una ventana sin relación con el objetivo que cambia su contenido antes de la entrega
    Cuando el agente solicita un clic real sobre el objetivo
    Entonces la acción se entrega y el snapshot queda gastado

  @BDD-56 @RF-11 @UT-56
  Escenario: BDD-56 Un cambio dentro de la ventana objetivo sí rechaza la acción
    Dado un snapshot observado con el alcance de frescura «ventana objetivo»
    Y un cursor de texto que parpadea dentro de la ventana objetivo, lejos del objetivo
    Cuando el agente solicita un clic real sobre el objetivo
    Entonces se devuelve STALE_SNAPSHOT con observe_again y no se envía ningún evento

  @BDD-57 @RF-11 @UT-57
  Escenario: BDD-57 Una ventana emergente sobre el objetivo rechaza la acción
    Dado un snapshot observado con el alcance de frescura «ventana objetivo»
    Y una ventana emergente que se solapa con la ventana objetivo aparece antes de la entrega
    Cuando el agente solicita un clic real sobre el objetivo
    Entonces se devuelve STALE_SNAPSHOT con observe_again y no se envía ningún evento

  @BDD-58 @RF-11 @UT-60
  Escenario: BDD-58 Un cambio de geometría rechaza la acción con cualquier alcance
    Dado un snapshot observado con el alcance de frescura «ventana objetivo»
    Y el monitor cambia de escala antes de la entrega
    Cuando el agente solicita un clic real sobre el objetivo
    Entonces se devuelve STALE_SNAPSHOT con observe_again y no se envía ningún evento
