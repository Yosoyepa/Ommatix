# language: es
Característica: Fusión espacial sin accionabilidad inventada
  El motor ofrece observaciones y acciones dentro del alcance acordado.
  Estos escenarios requieren step definitions productivas; no están ejecutados en este paquete.

  @BDD-07 @RF-06 @UT-16
  Escenario: BDD-07 Texto pequeño dentro de botón
    Dado texto de 25 por 14 píxeles dentro de un botón de 300 por 40 píxeles
    Cuando se calcula la contención del texto
    Entonces la IoC es 1 y el texto puede asociarse al botón

  @BDD-08 @RF-06 @UT-18
  Escenario: BDD-08 Panel y control anidado
    Dado un panel y un botón interno que contienen la misma caja OCR
    Cuando se asigna propietario al texto
    Entonces se prefiere el control específico sin depender del orden de entrada

  @BDD-09 @RF-06 @UT-20
  Escenario: BDD-09 Renglones estables
    Dado palabras OCR de varias líneas en orden arbitrario
    Cuando se determina el orden de lectura
    Entonces se agrupan líneas antes de ordenar horizontalmente

  @BDD-10 @RF-06 @UT-23
  Escenario: BDD-10 Labels ambiguos
    Dado dos etiquetas igualmente plausibles para un campo
    Cuando se resuelve la asociación externa
    Entonces se informa ambigüedad en vez de seleccionar la primera

  @BDD-11 @RF-07 @UT-24
  Escenario: BDD-11 Texto informativo
    Dado un título detectado por OCR sin affordance confirmada
    Cuando el agente intenta hacer clic en su ID
    Entonces se rechaza la acción por accionabilidad insuficiente

  @BDD-12 @RF-07 @UT-25
  Escenario: BDD-12 Placeholder desconocido
    Dado texto interno cuya naturaleza no está confirmada
    Cuando se publica el campo
    Entonces text_kind permanece unknown y no se inventa un valor
