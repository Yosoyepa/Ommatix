# Observe and act / Observar y actuar

*An action names a snapshot, a target and its expected revision. A stale reference is
rejected, a dry run is reported as simulated, and an authorised action reports delivery and
verification separately.*
*Una acción nombra un snapshot, un objetivo y su revisión esperada. Una referencia vieja se
rechaza, un dry run se informa como simulado y una acción autorizada informa por separado la
entrega y la verificación.*

Source / Fuente: [`03_secuencia_accion.mmd`](../thesis/diagramas/03_secuencia_accion.mmd).
Contract: [thesis chapter 07](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md).

```mermaid
sequenceDiagram
  participant A as Agent
  participant M as MCP adapter
  participant S as Snapshot / policy
  participant O as OS actuator
  participant V as Verifier
  A->>M: observe_screen
  M->>S: build and publish snapshot
  S-->>A: session + snapshot + IDs + revisions
  A->>M: execute_action with preconditions
  M->>S: validate context and policy
  alt invalid reference
    S-->>A: rejected / observe_again
  else dry_run
    S-->>A: simulated; no input
  else authorised
    S->>O: validated input
    O-->>S: delivery receipt
    S->>V: check postcondition
    V-->>A: effect_observed or indeterminate
  end
```
