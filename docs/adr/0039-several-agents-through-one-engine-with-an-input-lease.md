# 0039. Serve several agents through one engine, with a single lease over the observe-and-act cycle

- **Status:** Proposed
- **Date:** 2026-11-22
- **Approval:** Pending. Draft written by an AI agent at the owner's request; nothing in it is
  implemented, and no measurement supports it yet.

## Context

One engine process serves one connection: `McpServer::serve_stdio` reads the process's standard
input until it closes ([ADR-0002](0002-mcp-stdio-with-explicit-profiles.md),
[ADR-0015](0015-mcp-transport-on-rmcp-wire-types.md)). The process has one session identifier
(`wiring::session_id`), one registry, one snapshot counter and one request journal. An agent host
that wants a second agent on the same desktop can only start a second process.

Two processes on one desktop break a guarantee the engine gives today. Inside one process, any
delivery spends the snapshot: a reference to an earlier observation is refused with
`STALE_SNAPSHOT` (chapter 07; `identity.rs`,
`a_reference_to_an_earlier_observation_never_reaches_the_revision_check`; at the level of the
engine, `two_agents_on_one_engine_never_deliver_twice_on_one_snapshot`, in which two agents share
one engine in a generated order and no snapshot ever authorises two deliveries). That rule is what makes
an action a compare-and-swap on the state the agent saw. It is a property of one counter. With two
processes, a delivery by the first does not spend the snapshot of the second, and the second acts on
a screen it has not seen. The last look before a delivery (the engine captures the
screen again and refuses if it differs from the committed base) and the age bound of
[ADR-0025](0025-bound-authorisations-by-snapshot-and-evidence-age.md) narrow that gap; they were not designed to close it.

The desktop itself is one resource: one pointer, one keyboard focus, one foreground window. Several
agents can share what they see. They cannot share input without an order.

A second fact shapes the design. In the current engine every `observe_screen` publishes a new
snapshot, and only the latest snapshot authorises an action. If two agents shared one engine with no
other rule, an agent that only looks would invalidate the reference another agent is about to use.
A reader would cancel a writer, and two active agents could starve each other.

## Decision

1. **One engine per desktop session stays the only owner** of capture, perception state and input.
   Several agents reach it through a broker inside the engine process: a module of `adapter-mcp`,
   the adapter that owns the wire format, which calls the `Engine` directly as the stdio server
   does. They never start a second engine on the same desktop; a second process refuses to start
   with live input while the first holds the desktop.
2. **The broker serialises every call into the engine**, as the message loop does today. A check and
   the action it guards stay in one critical section. This is why no fencing token has to reach the
   operating system: the component that checks is the one that acts.
3. **A client is a connection.** The broker gives each connection a `client_id`. The agent does not
   choose it and never has to quote a secret. The session identifier stays one per process.
4. **One lease covers the observe-and-act cycle.** At most one client holds it. While it is held:
   only the holder's `observe_screen` publishes a new snapshot; only the holder's `execute_action`
   reaches the gate; any other client's `observe_screen` returns the page already published, marked
   as shared and with its age, which authorises nothing for that client; any other client's
   `execute_action` is refused before the gate and delivers no input. A dry run needs no lease.
   With several clients connected and no holder, any client's observation publishes a snapshot, as
   today, and no client delivers: a live action always needs the lease.
5. **The lease is bounded.** It has a term measured on the engine's monotonic clock and a number
   that grows with every grant, kept on the research record. It ends when the term ends, when the
   holder releases it or disconnects, on the operator's stop and on a purge
   ([ADR-0028](0028-operator-purge-file.md)). Waiting clients are served in arrival order, and a holder
   cannot renew while another client waits. Granting the lease spends the current snapshot: a new
   holder observes before it acts.
6. **With one client connected the broker is transparent**: that client holds the lease implicitly
   and every response is the one the engine gives today.
7. **The journal is kept per client.** The key is the pair of client and request identifier, so two
   agents may use the same request identifier. The total bound of
   [ADR-0034](0034-propose-fail-closed-retained-state-budgets.md) stays, split by a quota per
   client so that one agent cannot evict the entries of another.
8. **Local transport only.** On Windows, a named pipe that rejects remote clients, is restricted to
   the current user and refuses to start when the name is already taken. An `attach` mode of the
   executable relays standard input and output to the pipe, so an MCP host keeps starting a stdio
   server as it does now. The number of clients is bounded, and each connection keeps the message
   bounds of ADR-0015.

Open points for the owner, because each changes the dossier:

- **O1. How a client asks for the lease.** Proposed: an optional argument of `observe_screen`
  (`lease`: `acquire` or `release`) and a `lease` object in the page. The contract keeps its three
  tools; the schemas of chapter 07 change. The alternative is a lease assigned only from the
  operator's side (see [ADR-0040](0040-task-scoped-grants-narrower-than-the-local-policy.md)).
- **O2. The refusal code for a client without the lease.** Proposed: `POLICY_DENIED`, with the
  reason on the research record. A new code would be clearer and changes chapter 07.
- **O3. The term and the bound on clients.** Proposed defaults: the validity window of ADR-0025 for
  the term, eight clients.

## Alternatives considered

- **Several engine processes and a lock file between them:** the check (the lock) and the act (the
  input) would live in different processes, so a process that lost the lock while it was paused
  could still deliver. Fencing fixes that only when the resource checks the token, and the input
  interface of the operating system checks nothing.
- **One engine and no lease, relying on `STALE_SNAPSHOT` alone:** safe, because each delivery still
  spends the snapshot, but not live: observers invalidate actors, and two agents under contention
  can retry for ever. Optimistic validation suits rare conflicts; a shared desktop under two active
  agents is not that case.
- **Keep the last k snapshots valid:** it would let a reader and a writer coexist, and it gives up
  the rule that only the current observation authorises an action, which the preprint states and
  the model proves.
- **A fourth tool for the lease:** the contract has exactly three tools; this needs a stronger
  reason than convenience.
- **A TCP port on the loopback interface:** it is a network socket, which SEC-04 forbids in
  production code, and any local user could reach it.

## Consequences and cost

- Agents can cooperate on one desktop with one order of deliveries, and an orchestrator can decide
  who acts.
- The engine gains state that outlives a request: the lease, the waiting list, the clients. Each
  needs a bound and a test, in the manner of ADR-0034.
- A non-holder plans on a page that may be as old as the lease term. The page says so.
- A named pipe is new attack surface. The source check of SEC-04 must say exactly which calls are
  allowed, and [ADR-0016](0016-bounded-unsafe-for-windows-capture-and-input.md) decides whether the pipe needs
  `unsafe` or a safe wrapper.
- The broker is transport-independent, so all of its logic is tested on in-memory streams and the
  simulated desktop. No test injects real input.

## Required validation

New planned tests, with identifiers to be assigned in the matrix when the ADR is accepted. All run
on the simulated desktop with a fake clock and fixed seeds.

- Two clients, one engine: after the first client's delivery, the second client's request that cites
  the earlier snapshot is refused with `STALE_SNAPSHOT`, and the simulated actuator records nothing.
- While one client holds the lease, another client's `observe_screen` returns the shared page, the
  snapshot counter does not move, and the holder's next action still passes.
- A live action from a client without the lease delivers no input event and its refusal carries no
  text from the screen ([ADR-0021](0021-pin-every-loaded-artefact-and-keep-text-out-of-errors.md)).
- The term ends: another client gets the lease and the former holder's action is refused.
- A disconnect, the operator's stop and a purge each end the lease.
- Generated interleavings of observe, act, acquire and release by several clients keep three
  invariants: at most one holder; every delivery came from the holder and cited the current
  snapshot; the deliveries are those of some sequential run.
- With one client, the broker's responses equal the stdio server's on the acceptance scenarios.
- A client beyond the bound is refused; a full journal quota evicts only its owner's entries.
- Supervised, on Windows: the pipe rejects a remote client and another user.
- The state machine of the lease is added to the formal model
  ([ADR-0042](0042-formal-model-as-a-third-oracle.md)). A first model of the rules of this draft
  exists outside the repository (`MultiAgente.lean`, delivered to the owner with this draft; Lean 4
  core, no `sorry`). It proves
  that in any trace with any number of clients each snapshot authorises at most one delivery
  (`one_delivery_per_snapshot`), that a delivery is recorded exactly when the holder cited the
  current, unspent snapshot, that an observer without the lease changes nothing, and that a new
  holder must observe first. It does not model time, the waiting order or the transport.

## Reversibility

The broker is a module of the MCP adapter behind a switch that stays off. With the switch off the
executable serves stdio exactly as today. Removing the module removes the feature; the domain and the
application crates change only where the journal takes a client in its key. A failure of any
invariant above on generated interleavings would be a reason to withdraw it.

## Provenance

The owner's request of 2026-11-22 to connect several agents and let orchestrators direct them.
Refines ADR-0002, ADR-0006 and ADR-0015; relies on ADR-0025 and ADR-0034. The reasoning uses
optimistic concurrency control (Kung and Robinson, 1981), leases (Gray and Cheriton, 1989),
linearizability (Herlihy and Wing, 1990), the argument that a fencing token is only sound when the
resource checks it (Kleppmann, 2016) and time-of-check-to-time-of-use races (Bishop and Dilger,
1996). None of these is in chapter 11 yet; adding them is part of accepting this ADR.

## Resumen (ES)

Varios agentes se conectan a un único motor por escritorio a través de un intermediario interno, y
un solo arrendamiento, acotado en el tiempo, cubre el ciclo de observar y actuar. La regla actual
(toda entrega gasta la instantánea) ya da la seguridad entre agentes si el contador es uno solo; el
arrendamiento añade orden y evita que un agente que sólo mira invalide al que actúa. Con un cliente
el comportamiento es el de hoy. Quedan tres puntos abiertos que tocan el dosier.
