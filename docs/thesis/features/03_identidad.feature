# language: es
Característica: Identidad persistente y referencias vigentes
  El motor ofrece observaciones y acciones dentro del alcance acordado.
  Estos escenarios requieren step definitions productivas; no están ejecutados en este paquete.

  @BDD-13 @RF-09 @UT-34
  Escenario: BDD-13 Cruce de celda
    Dado un track asociado inequívocamente a un botón en el límite de un tile
    Cuando el botón se desplaza un píxel al tile vecino
    Entonces el registro conserva el ID y aumenta su revisión

  @BDD-14 @RF-09 @UT-36
  Escenario: BDD-14 Dos controles repetidos
    Dado dos controles con el mismo rol y label dentro de una celda
    Cuando se crean sus tracks
    Entonces se asignan IDs distintos y no se sobrescribe ninguno

  @BDD-15 @RF-09 @UT-37
  Escenario: BDD-15 Fila virtualizada
    Dado una fila visible que representa el archivo A
    Cuando la misma posición pasa a representar el archivo B
    Entonces no se conserva ciegamente la identidad accionable del archivo A

  @BDD-16 @RF-11 @UT-38
  Escenario: BDD-16 Snapshot anterior
    Dado una acción preparada con una observación anterior
    Cuando el agente solicita ejecutarla tras un nuevo snapshot
    Entonces se devuelve STALE_SNAPSHOT sin reanclaje silencioso

  @BDD-17 @RF-09 @UT-35
  Escenario: BDD-17 ID retirado
    Dado un ID retirado durante la sesión
    Cuando aparece otro control en el mismo lugar
    Entonces no se reutiliza el ID retirado

  @BDD-40 @RF-09 @UT-37
  Escenario: BDD-40 Identidad nativa sustituida con el mismo texto y la misma caja
    Dado un botón de accesibilidad «Guardar» con identidad nativa A en una caja
    Cuando otro objeto con identidad nativa B, el mismo texto y la misma caja ocupa ese sitio
    Entonces el elemento recibe un ID nuevo y el anterior se retira sin reanclaje
