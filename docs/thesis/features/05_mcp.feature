# language: es
Característica: Contrato MCP versionado
  El motor ofrece observaciones y acciones dentro del alcance acordado.
  Estos escenarios requieren step definitions productivas; no están ejecutados en este paquete.

  @BDD-22 @RF-15 @CT-05
  Escenario: BDD-22 Petición moderna
    Dado un cliente del perfil 2026-07-28
    Cuando envía tools/list con los metadatos por petición
    Entonces se procesa según el perfil moderno sin exigir initialize

  @BDD-23 @RF-15 @CT-08
  Escenario: BDD-23 Compatibilidad legacy
    Dado un adaptador legacy 2025-11-25 habilitado explícitamente
    Cuando el cliente completa initialize y notifications/initialized
    Entonces se usan las reglas legacy sin mezclar las del perfil moderno

  @BDD-24 @RF-15 @CT-07
  Escenario: BDD-24 Notification
    Dado una notification válida sin id de solicitud
    Cuando el adaptador la procesa
    Entonces no emite una respuesta JSON-RPC para esa notification

  @BDD-25 @RF-11 @CT-10
  Escenario: BDD-25 Coordenadas no admitidas
    Dado una solicitud execute_action con coordenadas x e y adicionales
    Cuando se valida el payload
    Entonces se rechaza por incumplir el contrato de IDs

  @BDD-26 @RF-10 @CT-02
  Escenario: BDD-26 Truncamiento explícito
    Dado más elementos que el límite solicitado
    Cuando se devuelve una página de observación
    Entonces se informa truncated y un cursor vinculado al mismo snapshot
