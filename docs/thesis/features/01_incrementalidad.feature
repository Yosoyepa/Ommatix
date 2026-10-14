# language: es
Característica: Percepción incremental consistente
  El motor ofrece observaciones y acciones dentro del alcance acordado.
  Estos escenarios requieren step definitions productivas; no están ejecutados en este paquete.

  @BDD-01 @RF-02 @UT-03
  Escenario: BDD-01 Píxel pequeño en fila impar
    Dado un frame base confirmado con ancho impar
    Cuando cambia un único píxel de color en una fila impar
    Entonces el tile correspondiente queda sucio y no se descarta como caret

  @BDD-02 @RF-02 @UT-05
  Escenario: BDD-02 Stride de captura
    Dado un framebuffer con padding al final de cada fila
    Cuando cambia un píxel real sin cambiar el padding
    Entonces se detecta el cambio usando el stride del frame

  @BDD-03 @RF-03 @UT-09
  Escenario: BDD-03 Halo transitivo
    Dado una región sucia que alcanza un control conectado a otro soporte
    Cuando se calcula el cierre contextual
    Entonces la región incluye ambos soportes completos

  @BDD-04 @RF-04 @UT-14
  Escenario: BDD-04 Fallo de inferencia
    Dado una región pendiente y un snapshot confirmado
    Cuando la inferencia de la región falla
    Entonces se conserva la base confirmada y se bloquea la actuación con evidencia obsoleta

  @BDD-05 @RF-04 @UT-15
  Escenario: BDD-05 Resultado fuera de orden
    Dado dos candidatos construidos desde la misma revisión
    Cuando se publica el primero y después termina el segundo
    Entonces se rechaza el commit del segundo por obsoleto

  @BDD-06 @RF-03 @UT-11
  Escenario: BDD-06 Presupuesto de regiones
    Dado regiones cuyo cierre supera el presupuesto configurado
    Cuando se selecciona el plan de percepción
    Entonces se elige refresco completo sin omitir cambios
