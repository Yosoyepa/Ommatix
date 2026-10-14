# The engine as implemented / El motor implementado

*What `ommatix-application`'s `Engine` does today (S2b) for each MCP tool, and which port it
calls at each step. A failed observation publishes nothing; an action reports only what is
known.*
*Lo que hace hoy (S2b) el `Engine` de `ommatix-application` con cada herramienta MCP y qué
puerto llama en cada paso. Una observación fallida no publica nada; una acción informa sólo
lo que se sabe.*

Design views: [components](components.md), [action sequence](action-sequence.md),
[snapshot lifecycle](snapshot-state.md). Contract:
[thesis chapter 07](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md);
transport: [ADR-0015](../adr/0015-mcp-transport-on-rmcp-wire-types.md).

## observe_screen

```mermaid
flowchart TD
  REQ[observe_screen<br/>query · limit · cursor · include_values] --> CUR{cursor?}
  CUR -->|yes| PAGE[next page of the snapshot<br/>the cursor names]
  CUR -->|no| CAP[FrameCapturer.capture]
  CAP --> PLAN[Region plan, Algorithm 1<br/>against the committed base<br/>+ pending dirty tiles]
  PLAN --> DET[VisionDetector.detect<br/>planned regions only]
  PLAN --> OCR[TextRecognizer.recognize<br/>planned regions only]
  DET --> EVI[Evidence: keep outside regions,<br/>replace inside]
  OCR --> EVI
  EVI --> FUS[Spatial fusion, Algorithm 2]
  FUS --> ICO[Icon labels, Algorithm 3A<br/>text-less controls of the planned regions,<br/>curated prototypes of the configured context;<br/>others keep their previous label]
  ICO --> REG[Identity registry, Algorithm 3B]
  REG --> STAGE[Stage candidate snapshot]
  STAGE --> COMMIT[Commit: snapshot sN published]
  COMMIT --> FIRST[First page: filter by query,<br/>redact values per PolicyStore,<br/>cursor bound to sN]
  CAP -. port error .-> FAIL[Record failure: nothing published,<br/>tiles stay pending, actions blocked]
  DET -. port error .-> FAIL
  OCR -. port error .-> FAIL
  FIRST --> OUT[structuredContent: page<br/>_meta io.ommatix/timings]
  PAGE --> OUT
```

## execute_action

```mermaid
flowchart TD
  A[execute_action] --> ARGS{arguments<br/>within limits?}
  ARGS -->|no| REJ[rejected]
  ARGS -->|yes| J{journal:<br/>request_id + FNV-1a hash}
  J -->|same id, same hash| REPLAY[stored result, replayed]
  J -->|same id, other hash| CONF[rejected: REQUEST_CONFLICT]
  J -->|new| GATE{snapshot published, not degraded<br/>and not spent; session, epoch,<br/>snapshot, target, revision,<br/>actionability match?}
  GATE -->|no| REJ2[rejected: STALE_SNAPSHOT,<br/>STALE_TARGET, EPOCH_MISMATCH, ...]
  GATE -->|yes| DRY{dry_run?<br/>default true}
  DRY -->|yes| SIM[simulated]
  DRY -->|no| POL{PolicyStore: actuation enabled,<br/>not stopped; OsActuator live?}
  POL -->|no| DEN[rejected: POLICY_DENIED<br/>nothing delivered]
  POL -->|yes| ALLOW{OsActuator attests a foreground<br/>application on allowed_applications?<br/>none can before S3}
  ALLOW -->|no| DEN
  ALLOW -->|yes| FRESH{FrameCapturer.capture:<br/>still the committed base?}
  FRESH -->|no| STALE[rejected: STALE_SNAPSHOT<br/>nothing delivered]
  FRESH -->|yes| DEL[OsActuator.deliver<br/>spends the snapshot]
  DEL --> RCPT{receipt: every submitted<br/>event inserted?}
  RCPT -->|no or error| IND[indeterminate, do not retry]
  RCPT -->|yes| EFF{expected_effect}
  EFF -->|none| DLV[input_delivered]
  EFF -->|visual_change| RECAP[FrameCapturer.capture<br/>compare with the pre-delivery frame]
  RECAP -->|changed| EO[effect_observed]
  RECAP -->|unchanged| IND
  EFF -->|focus, value, scroll| IND
```

The result of every new request that passes the argument checks is recorded in the journal
under its hash; the typed text itself is never stored. Any delivery, whatever its outcome,
spends the snapshot: the next action needs a new observation. `get_engine_metrics` reads the
counters (observations, rejected and delivered actions, pending regions) and reports whether
models are loaded.

El resultado de cada petición nueva que pasa la comprobación de argumentos se guarda en el
diario bajo su hash, nunca el texto tecleado. Cualquier entrega, sea cual sea su resultado,
agota el snapshot: la acción siguiente necesita una observación nueva.
`get_engine_metrics` lee los contadores (observaciones, acciones rechazadas y entregadas,
regiones pendientes) e informa si hay modelos cargados.
