# language: es
Característica: Caché perceptual con soporte y rechazo
  El motor ofrece observaciones y acciones dentro del alcance acordado.
  Estos escenarios requieren step definitions productivas; no están ejecutados en este paquete.

  @BDD-18 @RF-08 @UT-28
  Escenario: BDD-18 Inversión de luminancia ideal
    Dado un descriptor con suficientes gradientes válidos
    Cuando se compara con su inversión ideal de luminancia
    Entonces la comparación enmascarada admite distancia cero

  @BDD-19 @RF-08 @UT-29
  Escenario: BDD-19 Crop plano
    Dado un crop sin gradientes informativos
    Cuando se intenta resolver un icono
    Entonces se devuelve unknown por soporte insuficiente

  @BDD-20 @RF-08 @UT-31
  Escenario: BDD-20 Conflicto entre prototipos
    Dado dos prototipos semánticos con similitud indistinguible
    Cuando se consulta el caché
    Entonces se rechaza la clasificación automática

  @BDD-21 @RF-08 @UT-31
  Escenario: BDD-21 Icono fuera de contexto
    Dado un símbolo curado para una aplicación diferente
    Cuando se observa en un contexto no validado
    Entonces no se asigna su significado como certeza
