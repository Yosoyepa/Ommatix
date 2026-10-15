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
