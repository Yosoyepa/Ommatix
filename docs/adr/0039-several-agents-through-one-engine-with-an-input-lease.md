# 0039. Serve several agents through one engine, with a single lease over the observe-and-act cycle

- **Status:** Proposed
- **Date:** 2026-11-22; revised 2026-11-24
- **Approval:** Pending. Draft written by an AI agent at the owner's request on 2026-11-22. On
  2026-11-24 the draft was revised against the code, under the owner's delegation, by an AI agent
  (Claude): the findings and the decisions taken on them are recorded in
  [the review of that day](../reviews/2026-11-24-adr-0039-0040-against-the-code.md). Acceptance is
  still the owner's. Nothing in it is wired into the product, and no measurement supports it yet.

## Context

**What the MCP adapter already does.** `McpServer` is `Clone` over one `Arc<Mutex<Engine>>`, and
its clones share that engine (`src/adapters/mcp/src/server.rs`, lines 18–28). `serve` is generic
over any byte stream (`AsyncRead` and `AsyncWrite`, lines 68–82); `serve_stdio` is that method on
the process's standard input and output (lines 62–66). Each served stream is its own `Connection`,
with its own protocol profile, over the shared engine (`protocol.rs`, lines 70–90). A tool call
holds the engine's lock for the whole of the engine's work on it (`tools.rs`, lines 108–179), so
calls are already serialised. What the adapter does not have is any notion of which connection a
call came from.

**Today one connection is the session.** The executable serves one connection
([ADR-0002](0002-mcp-stdio-with-explicit-profiles.md),
[ADR-0015](0015-mcp-transport-on-rmcp-wire-types.md)): `serve` in `src/app/src/main.rs` builds one
engine and calls `serve_stdio` (lines 36–70). The process has one session identifier
(`wiring::session_id`), one registry, one snapshot counter and one request journal. When any
served connection ends, however it ends, `serve` purges the engine (`server.rs`, lines 79–81;
`Engine::purge`, chapter 08 §8.4; pinned by `src/adapters/mcp/tests/sec_07_deletion.rs`). With two
connections on one engine, the first to leave would delete the state the other is using. An agent
host that wants a second agent on the same desktop can today only start a second process.

**Two processes on one desktop break a guarantee the engine gives today.** Inside one process, any
delivery spends the snapshot: a reference to an earlier observation is refused with
`STALE_SNAPSHOT` (chapter 07; `identity.rs`,
`a_reference_to_an_earlier_observation_never_reaches_the_revision_check`; at the level of the
engine, `two_agents_on_one_engine_never_deliver_twice_on_one_snapshot`, in which two agents share
one engine in a generated order and no snapshot ever authorises two deliveries). That rule is what
makes an action a compare-and-swap on the state the agent saw. It is a property of one counter.
With two processes, a delivery by the first does not spend the snapshot of the second, and the
second acts on a screen it has not seen. The last look before a delivery (the engine captures the
screen again and refuses if it differs from the committed base) and the age bound of
[ADR-0025](0025-bound-authorisations-by-snapshot-and-evidence-age.md) narrow that gap; they were
not designed to close it.

The desktop itself is one resource: one pointer, one keyboard focus, one foreground window. Several
agents can share what they see. They cannot share input without an order.

**Every observation publishes.** In the current engine every `observe_screen` without a cursor
publishes a new snapshot (`Engine::observe_screen`, `src/application/src/engine.rs`, lines
231–251), and only the latest unspent snapshot authorises an action, a dry run included
(`act.rs`, lines 294–298). If two agents shared one engine with no other rule, an agent that only
looks would invalidate the reference another agent is about to use. A reader would cancel a
writer, and two active agents could starve each other. There is also no public way to serve the
published page again without observing: `first_page` and `next_page` are `pub(super)`
(`src/application/src/engine/view.rs`, lines 166–197), and a cursor binds the effective
`include_values` (line 185).

## Decision

1. **One engine per desktop session stays the only owner** of capture, perception state and input.
   Several agents reach it through a broker inside the engine process: a module of `adapter-mcp`,
   the adapter that owns the wire format, built on the existing `McpServer` and `Connection`, which
   calls the `Engine` directly as the stdio server does. Agents never start a second engine on the
   same desktop. The operator, or an agent host's launcher acting with the operator's authority,
   starts the one engine process with the broker on; an agent-side process never starts an
   engine, because SEC-04 forbids `Command::new` in production code and an engine started from
   the agent's side would put authority on the wrong side. A second engine on the same desktop is
   refused by the transport itself: the pipe's name is fixed per user session and created as the
   first instance, so a second `serve` with the broker on cannot create it and exits
   ([ADR-0043](0043-local-named-pipe-transport-for-several-clients.md), decision 4).
2. **The broker serialises every call into the engine**, as the adapter already does. A check and
   the action it guards stay in one critical section. This is why no fencing token has to reach
   the operating system: the component that checks is the one that acts.
3. **A client is a connection.** The broker gives each connection a `client_id`. The agent does not
   choose it and never has to quote a secret. The session identifier stays one per process.
4. **One lease covers the observe-and-act cycle.** At most one client holds it. While it is held:
   - only the holder's `observe_screen` publishes a new snapshot;
   - only the holder's live `execute_action` reaches the gate; any other client's live action is
     refused before the gate with `POLICY_DENIED`, delivers no input, and the reason goes on the
     research record, never into the message;
   - any other client's `observe_screen` returns the page already published, built again for that
     client (its own `include_values`, its own grant), which authorises nothing that client could
     not do anyway. The page has the same `structuredContent` shape as the holder's, each client's
     page being built from the same snapshot with its own `include_values` and its own grant, so
     two clients may receive different elements and values. That the page is shared, and its age
     on the engine's clock, depend on the run, so by engineering rule 1 they go in `_meta`, never
     in `structuredContent`. This needs an engine method that serves the published
     snapshot again, per client, without capturing; the engine has none today.
   - **A dry run needs no lease.** A client without the lease may dry-run on the shared page. A
     dry run delivers nothing, reads no policy and spends nothing (`act.rs`, lines 329–331), so
     refusing it would only hide what the client already knows. It still meets every check that
     comes before that line today: the snapshot must be the current, unspent one (lines 294–298),
     and the target's age bound of ADR-0025 applies (line 327). Once the holder delivers, or the
     lease passes to another client, the shared page is spent and a dry run on it is refused with
     `STALE_SNAPSHOT`, as now.

   With several clients connected and no holder, any client's observation publishes a snapshot,
   as today, and no client delivers: a live action always needs the lease.
5. **How the lease is asked for (formerly O1).** An optional `lease` argument of `observe_screen`,
   with the values `acquire` and `release`. The contract keeps its three tools; the input schema of
   `observe_screen` in chapter 07 gains the argument when this ADR is accepted. Whether the client
   holds the lease, waits, or has no lease is reported with the page; it depends on the other
   clients, so it goes in `_meta` like the page's age. A fourth tool was rejected (see
   Alternatives), and a lease assigned only from the operator's side would make every orchestrator
   need the operator's harness for the ordinary case.
6. **The refusal code (formerly O2) is `POLICY_DENIED`.** Chapter 07 §7.7 (line 59) classes
   `POLICY_DENIED` as a decision code and `PERMISSION_DENIED` as an adapter code; a missing lease
   is the engine's decision, not an adapter's failure. No new code is added.
7. **The lease is bounded.** It has a term and a number that grows with every grant, kept on the
   research record. **The term (formerly O3)** is the validity window of ADR-0025
   (`[engine] max_snapshot_age_ms`, 10 000 ms by default, `src/app/src/config.rs` line 144): it
   covers one observe-and-act cycle, which ADR-0025 already bounds. At most eight clients are
   connected at once; a ninth is refused. The lease ends when its term ends, when the holder
   releases it or disconnects, on the operator's stop and on a purge
   ([ADR-0028](0028-operator-purge-file.md)). Waiting clients are served in arrival order. Granting
   the lease spends the current snapshot: a new holder observes before it acts.
8. **When the lease is evaluated.** Validity is evaluated once, when a call enters the critical
   section, on the engine's injected clock. A delivery already in progress completes: a two-phase
   `type` ([ADR-0018](0018-two-phase-type-with-element-level-focus-confirmation.md)) polls the clock
   between its click and its keys (`act.rs`, lines 557–600), and must not lose the lease between
   them. A holder renews by asking for the lease again, and only when nobody waits; a renewal keeps
   the lease's number, starts a new term and spends nothing.
9. **With one client connected the lease is transparent.** That client holds the lease implicitly,
   without asking for it. When a second client connects, the implicit lease becomes an ordinary
   lease whose term starts at that moment: an attach neither cancels a running agent at once nor
   leaves it an unbounded lease. The transparency covers the lease only. With the broker on, the
   grants of [ADR-0040](0040-task-scoped-grants-narrower-than-the-local-policy.md) govern every
   client, a single one included: a client without a valid grant observes without values and
   delivers nothing. With the broker off nothing changes.
10. **A client's own state goes when it leaves; the engine is purged when the last one leaves.** A
    connection that ends deletes that client's state: its lease, its place in the queue, its
    entries in the journal. The engine itself is purged, as `serve` does today, when the last
    connection ends. With one client this is today's behaviour, so SEC-07 keeps its meaning; with
    several, one agent leaving cannot erase what the others are using. What a client that left had
    observed stays in the engine until the last client leaves or the operator purges: perception
    state belongs to the desktop, not to a client, and the operator's purge file
    ([ADR-0028](0028-operator-purge-file.md)) still deletes it at any request boundary. Chapter 08
    §8.4 will say so when the broker is wired.
11. **The journal is kept per client.** The key becomes the pair of client and request identifier,
    so two agents may use the same request identifier. What is bounded today is only the evictable
    part: `JOURNAL_CAPACITY = 1024` results that delivered no input
    (`src/application/src/engine/journal.rs`, lines 7–20); delivered and indeterminate results are
    kept for the session without bound, keyed today by `request_id` alone (lines 92–99), and
    [ADR-0034](0034-propose-fail-closed-retained-state-budgets.md) was accepted as documentation
    only, with no budget. A quota per client can therefore only split the evictable part, so that
    one agent cannot evict another's results that delivered nothing; it bounds nothing that is not
    bounded today.
12. **Transport: in-memory streams first.** The broker is built and tested on in-memory streams
    (`serve` already accepts any stream) and on the simulated desktop. The local transport is a
    separate spike and a separate decision, which must meet these constraints as the code stands:
    - the SEC-04 source check forbids `tokio::net`, `OpenOptions`, `File::create` and
      `Command::new` in production code (`src/acceptance/tests/repository.rs`, lines 142–173);
      `tokio`'s named pipes live in `tokio::net`;
    - the SEC-04 lockfile check forbids `mio` and `socket2` in every lockfile (lines 228–262), and
      `tokio`'s `net` feature depends on both;
    - [ADR-0016](0016-bounded-unsafe-for-windows-capture-and-input.md) allows `unsafe` only in the
      capture and input adapters; the MCP adapter and the app keep `forbid`.

    The intended shape stays a Windows named pipe that rejects remote clients, is restricted to the
    current user and refuses to start when the name is taken, with an `attach` mode of the
    executable that relays its standard input and output to the pipe, so that an MCP host keeps
    starting a stdio server. Each connection keeps the message bounds of ADR-0015.

    **The transport is decided in its own ADR.** A spike and an experiment run on 2026-11-24,
    kept outside the repository, support a Windows named pipe through the `interprocess` crate's
    synchronous API, with no `unsafe` of our own; it is proposed as
    [ADR-0043](0043-local-named-pipe-transport-for-several-clients.md), which carries its
    constraints, costs, open points and required validation, and is built after the broker. The
    `attach` mode only connects: when no engine listens it fails with a clear error and starts
    nothing. Nothing about the pipe is promised by this ADR; if ADR-0043's validation refutes it,
    the fallback is our own `unsafe` under a new exception in the manner of ADR-0016, under its
    own ADR.

### Open points

None is left in this ADR. The points that remain open about the transport (a pipe name taken
before the engine starts, and how the engine refuses to start the broker when elevated) are
ADR-0043's.

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
- **A `serve` per connection that never purges:** simpler, and it drops the deletion at the end of
  the session that SEC-07 pins. Purging on the last disconnect keeps it for the one-client case.
- **Refusing dry runs without the lease:** it would hide only what the client already has.
- **A new refusal code for a missing lease:** clearer, and it changes chapter 07 for no safety gain.
- **A TCP port on the loopback interface:** it is a network socket, which SEC-04 forbids in
  production code, and any local user could reach it.

## Consequences and cost

- Agents can cooperate on one desktop with one order of deliveries, and an orchestrator can decide
  who acts.
- The broker is smaller than the first draft suggested: the shared engine, the serialisation and
  the stream-generic `serve` exist. What is new is the client identity, the lease, the per-client
  journal key, the per-client page and the purge on the last disconnect.
- The engine gains state that outlives a request: the lease, the waiting list, the clients. Each
  needs a bound and a test, in the manner of ADR-0034.
- A non-holder plans on a page that may be as old as the lease term. Its `_meta` says so.
- A client may dry-run but may not act; a dry run on a spent shared page is refused, as today.
- What a client observed outlives its connection while other clients remain; only the last
  disconnect or the operator's purge deletes it.
- A named pipe is new attack surface; its bounds, its dependencies and its licences are
  ADR-0043's.
- No test injects real input.

## Required validation

New planned tests, with identifiers to be assigned in the matrix when the ADR is accepted. All run
on in-memory streams and the simulated desktop, with a fake clock and fixed seeds.

- Two clients, one engine: after the first client's delivery, the second client's request that
  cites the earlier snapshot is refused with `STALE_SNAPSHOT`, and the simulated actuator records
  nothing.
- While one client holds the lease, another client's `observe_screen` returns the published page
  rebuilt for it, the snapshot counter does not move, its `_meta` carries the page's age and its
  `structuredContent` does not, and the holder's next action still passes.
- A live action from a client without the lease is refused with `POLICY_DENIED`, delivers no input
  event and its refusal carries no text from the screen
  ([ADR-0021](0021-pin-every-loaded-artefact-and-keep-text-out-of-errors.md)).
- A client without the lease dry-runs on the current shared page; after the holder's delivery the
  same dry run is refused with `STALE_SNAPSHOT`.
- The term ends: another client gets the lease and the former holder's action is refused.
- A two-phase `type` whose term ends between its click and its keys completes; the next call is
  refused.
- A holder renews while nobody waits, with the same number; it cannot renew while another waits.
- One client holds the lease implicitly; when a second connects, the first keeps it for one term
  and then loses it.
- A disconnect, the operator's stop and a purge each end the lease. A connection that ends while
  others remain leaves the published snapshot and the others' journal entries in place and deletes
  its own; what the departed client had observed stays until the last connection ends or the
  operator's purge file is honoured; the last connection that ends purges the engine.
- Generated interleavings of observe, act, acquire and release by several clients keep three
  invariants: at most one holder; every delivery came from the holder and cited the current
  snapshot; the deliveries are those of some sequential run.
- With one client holding a grant equal to the local policy, the broker's responses equal the stdio
  server's on the acceptance scenarios.
- A ninth client is refused; a full journal quota evicts only its owner's results that delivered
  no input.
- The transport's own tests, the supervised ones included (another user, an elevated process),
  are listed in ADR-0043.
- The state machine of the lease is added to the formal model
  ([ADR-0042](0042-formal-model-as-a-third-oracle.md)). A first model of the rules of the first
  draft exists outside the repository (`MultiAgente.lean`, delivered to the owner with that draft;
  Lean 4 core, no `sorry`). It proves that in any trace with any number of clients each snapshot
  authorises at most one delivery (`one_delivery_per_snapshot`), that a delivery is recorded
  exactly when the holder cited the current, unspent snapshot, that an observer without the lease
  changes nothing, and that a new holder must observe first. It does not model time, the waiting
  order, renewal or the transport, so decisions 7 and 8 are not in it yet.

The first building blocks of the pair of ADRs exist unwired on local branches: the grant of
ADR-0040 (`Grant` in `src/application/src/grant.rs`, commit `dd0579c`, branch `feat/grant-meet`)
and the lease state machine of this ADR (`LeaseTable` in `src/application/src/lease.rs`, commit
`fd53a21`, branch `feat/lease-machine`, on top of the former), pure, with time passed in by the
caller. Nothing calls either yet, and nothing of the broker or the transport exists.

## Reversibility

The broker is a module of the MCP adapter behind a switch that stays off. With the switch off the
executable serves stdio exactly as today, purge on disconnect included. Removing the module removes
the feature; the domain and the application crates change only where the journal takes a client in
its key and where the published page is served again. A failure of any invariant above on
generated interleavings would be a reason to withdraw it.

## Provenance

The owner's request of 2026-11-22 to connect several agents and let orchestrators direct them.
Revised on 2026-11-24 after reading the draft against the code
([review](../reviews/2026-11-24-adr-0039-0040-against-the-code.md)); the decisions on the open
points were taken that day under the owner's delegation. The transport is proposed in ADR-0043. Refines ADR-0002, ADR-0006 and ADR-0015;
relies on ADR-0018, ADR-0025 and ADR-0034. The reasoning uses optimistic concurrency control (Kung
and Robinson, 1981), leases (Gray and Cheriton, 1989), linearizability (Herlihy and Wing, 1990),
the argument that a fencing token is only sound when the resource checks it (Kleppmann, 2016) and
time-of-check-to-time-of-use races (Bishop and Dilger, 1996). None of these is in chapter 11 yet;
adding them is part of accepting this ADR.

## Resumen (ES)

Varios agentes se conectan a un único motor por escritorio a través de un intermediario dentro del
adaptador MCP, que ya comparte un motor tras un mutex y sirve cualquier flujo. Un solo
arrendamiento, acotado por la ventana de vigencia de ADR-0025, cubre el ciclo de observar y actuar:
se pide con un argumento `lease` de `observe_screen`, su ausencia se rechaza con `POLICY_DENIED`, se
evalúa al entrar en la sección crítica y una entrega en curso termina. Quien no lo tiene recibe la
página publicada, con su edad en `_meta`, y puede hacer simulaciones. Con un solo cliente el
arrendamiento es implícito; con el intermediario encendido las concesiones de ADR-0040 rigen
siempre. Lo que observó un cliente que se fue se conserva hasta que se va el último o el operador
purga; el motor se purga al irse el último cliente. El operador arranca el único motor con el
intermediario encendido; `attach` sólo se conecta y nunca arranca un motor, y un segundo motor no
puede crear el tubo, cuyo nombre es fijo por sesión de usuario. Primero flujos en memoria; el
transporte, un tubo con nombre, se propone en ADR-0043. No queda ningún punto abierto en esta ADR;
los del transporte están en ADR-0043. Revisada el 2026-11-24 contra el código; la aceptación es del responsable.
