# 0040. Give each agent a task-scoped grant that can only narrow the local policy

- **Status:** Proposed
- **Date:** 2026-11-22; revised 2026-11-24
- **Approval:** Pending. Draft written by an AI agent at the owner's request on 2026-11-22. On
  2026-11-24 the draft was revised against the code, under the owner's delegation, by an AI agent
  (Claude): the findings and the decisions taken on them are recorded in
  [the review of that day](../reviews/2026-11-24-adr-0039-0040-against-the-code.md), and the text
  was later brought in step with the code written for it. Acceptance is still the owner's. Its
  policy-shaped part, the grants file, the tickets, and a grant's delivery budget and term exist as
  code on branches that are not merged (see "What exists"); the executable serves the broker only
  when built with the `broker-pipe` feature and configured with `[mcp] transport = "pipe"`
  ([ADR-0043](0043-local-named-pipe-transport-for-several-clients.md)). The kinds of action and the
  region do not exist.

## Context

The local policy is one per process: whether actuation is enabled, whether field values may be
returned, the allowed applications, their window classes, the operator's pin on one window and the
stop (`LocalPolicy`, `src/application/src/ports.rs` lines 358–376,
[ADR-0007](0007-fail-closed-actuation-and-honest-simulation.md),
[ADR-0020](0020-operator-scope-on-windows.md)). The agent has no port to write it, and the engine
reads it on every action and again before each delivery.

**What the policy store can do today.** `PolicyStore::current()` takes no argument: there is one
answer for the whole process (`ports.rs`, lines 378–381). Its implementation, `ConfigPolicy`, fixes
everything at start from the configuration file except three things it reads on every call: the
stop, the window pin file and the purge file (`src/app/src/policy.rs`, lines 106–122 and
214–257). The configuration's `[safety]` section refuses unknown keys
(`#[serde(deny_unknown_fields)]`, `src/app/src/config.rs` lines 178–179). So nothing that names a
client, and nothing that changes while the process runs other than those three files, can reach
the engine today.

**What an observation carries.** An element of a snapshot has an identifier, a revision, a role, a
label, an affordance, an optional value and the age of its accessibility evidence
(`StoredElement`, `src/application/src/engine/observe.rs` lines 198–213); on the wire it has a
zone, not an application (`ElementView`, `src/application/src/contract.rs` lines 216–230). With
the freshness switch of [ADR-0032](0032-freshness-scoped-to-the-target-window.md) on, the
snapshot keeps, for the freshness check only, the handle of the window under each target
(`src/application/src/engine/freshness.rs`, lines 122–127), never its application. The
application is known only at the gate, from the actuator's attestation of the foreground
(`act.rs`, lines 346–354). Today's policy does not scope what is observed by application either.

With several agents ([ADR-0039](0039-several-agents-through-one-engine-with-an-input-lease.md)) one
policy is too coarse. An orchestrator that hands a sub-agent the task of reading one total from one
window hands it, today, everything the operator allowed to anyone. The agent is the component the
threat model trusts least: what it reads on the screen can carry instructions (chapter 08). The
damage a misled agent can do is bounded by the authority it holds, so each agent should hold only
what its task needs.

A second constraint comes from the same threat. A permission the agent must quote, such as a token
in a tool argument, passes through the model's context, and a model that can be talked into acting
can be talked into repeating a token. Authority should be attached to the connection, where the
model cannot read it.

## Decision

1. **A grant narrows the policy and never widens it.** The engine acts under the meet of the local
   policy and the client's grant: an application is allowed when both allow it; a window is allowed
   when it satisfies both pins and both class lists; values are returned when both say so;
   actuation is enabled when both enable it. The operator's stop is not part of a grant and always
   wins.
2. **A grant carries only limits:** the client it is bound to; a subset of the applications, with
   optional window classes for each; an optional window pin; the kinds of action the client may
   deliver; whether it may read values; a budget of delivered actions, where zero means it may only
   observe; a term on the engine's clock, a duration that runs from the client's admission; and an
   optional region of the frame outside which it has no target.
3. **A grant is bound to a connection, not quoted by the agent.** A client attaches with a one-use
   ticket that the operator's side creates and the host passes to the `attach` mode of ADR-0039.
   The policy store lists the SHA-256 of each ticket next to its grant, so the engine only reads and
   compares, and writes nothing to the store. The ticket selects the grant and is spent at once.
   Nothing the model writes can name or change a grant. The ticket reaches `attach` as the path of
   a file (`--ticket-file`), never as an argument value or an environment variable: an argument is
   visible in the process list and the environment is inherited by children, while a file can be
   restricted and removed by the operator after use. `attach` reads it and sends it as one
   handshake line on the pipe before any MCP message. The broker in `adapter-mcp`, which owns the
   wire, hashes it: `sha2` 0.11 is already a workspace dependency (`Cargo.toml` line 54), so
   `adapter-mcp` gains it with no new crate in `Cargo.lock`; the application crate may not depend
   on it (`src/acceptance/tests/repository.rs`, lines 288–295). The ticket never enters the
   model's context or a log. The handshake is part of
   [ADR-0043](0043-local-named-pipe-transport-for-several-clients.md), decision 9.
4. **Tickets, budgets and terms are per engine process.** Because the engine writes nothing to the
   store, that a ticket was spent, what a budget has left and when a term started are kept in the
   engine process, by the broker: the spent tickets in its spent set, and for each connected
   client a ledger of its deliveries, of the instant it was admitted and of the ceilings of its
   budget and term (decision 11). After the engine process restarts, a ticket that the store still lists can
   attach again, with a full budget and a new term. A restart is an operator's act; an operator who
   does not want a ticket reused removes it from the store. A spent-list written by the engine
   would be the first thing the engine writes to the store, and is not adopted.
5. **Grants live in the policy store, which needs a new section and a new read path.** The
   operator, or an orchestrator's harness running with the operator's authority, writes grants as
   it writes the rest of the policy. "A change or a withdrawal takes effect at the next action, as
   the stop does" requires what the stop and the pin already have: the grants are read from a file
   the configuration names, on every call, and a file that is missing, unreadable or malformed
   grants nothing. That is a new key of the configuration (the `[safety]` section refuses unknown
   keys today), a new reader in the app, and a way for the engine to ask the store for a client's
   policy, since `current()` takes no client. The agent-facing contract keeps its three tools and
   gains no way to write a grant.
6. **Where a grant is applied.** The policy-shaped part (applications, window classes, pin,
   actuation, values) is applied by a policy store that decorates the operator's store: the broker
   sets the calling client's grant under the engine's lock for the length of the call, and every
   read of the policy, the gate's, the last look's and the view's, then returns the meet. The
   budget and the term are checked by the broker, under its one lock, at the entry of each call:
   a live action new to the client's journal is refused when the term has ended, then when the
   budget is spent, then when the client does not hold the lease, and only otherwise reaches the
   gate; the lock also covers the engine call and the count, so two deliveries cannot both pass a
   budget of one. The kinds of action are to be checked in the broker too, and the region in the
   engine, because the region needs the target's box, which only the engine's registry knows. The
   policy-shaped part is built this way: `Grant::narrow` and `Grant::meet`, and `GrantedPolicy`,
   the decorating store, into which the broker installs the calling client's grant under its one
   lock for the length of the call; the budget and the term are built in the broker, with
   `GrantLedger` (see "What exists"). The kinds of action and the region are not built
   (decision 12).
7. **Fail closed, for every client.** With the broker on, a client without a valid grant (none, an
   ended term, a withdrawn grant) may observe without values and may deliver nothing. This holds for
   a single client too: the transparency of ADR-0039 covers the lease only. With the broker off
   nothing changes. As built, such a client is served under the default grant, which allows no
   application, no live input and no values: installing no grant installs that one, never the
   operator's policy unchanged. The term is evaluated once, at the entry of each call, with that
   call's instant, and it has ended when the instant reaches the admission plus the term: an action
   that passed the check completes, both phases of a `type` included, even if the term ends while
   it runs, and the next call is judged at its own entry. A client whose term has ended may still
   dry-run, have its own requests replayed, read the metrics, and ask for, hold or leave the lease;
   its new live actions are refused. A client whose budget is spent keeps everything its grant
   gives except live input, values included.
8. **What is observed is not filtered by application.** A grant filters the values (both the policy
   and the grant must allow them) and, when it has a region, leaves out of the client's page every
   element that is not inside the region. It does not filter elements by application: an element
   carries no application (see Context), so a client may see elements of an application it cannot
   act on. Nothing is lost against the present state, whose policy does not scope observation by
   application either; adding window evidence to elements would be a change of the dossier and
   needs its own decision. The page and its cursor are built per client.
9. **A refusal names the limit, not the screen.** A grant limit refuses with `POLICY_DENIED`, as the
   policy's own refusals do today (`act.rs`, lines 118–134), and the message names which limit
   applied without quoting the screen or the agent. As built, the broker's refusals for the term
   and the budget each name their limit in a static English sentence. A refusal by a grant's
   applications, windows or actuation still carries the engine's policy-shaped text, which does
   not say that a grant rather than the operator's policy refused, and so does a withdrawn grant
   whose term has not ended.
10. **What spends a budget.** A grant's delivery budget is spent by every action whose result the
    journal keeps for the session as possible input: a result that reports delivered input
    (`input_delivered`, `effect_observed`) or an `indeterminate` one, with or without delivered
    input (`ActionResult::reported_input`, `src/application/src/contract.rs`, the one predicate
    the journal and the broker both call). An attempt whose effect is unknown may have reached the
    desktop, so it counts: that fails closed. A two-phase `type`
    ([ADR-0018](0018-two-phase-type-with-element-level-focus-confirmation.md)) is one action and
    counts once, whether its keys were sent or only its click. A dry run, a replay of a request the
    client's journal holds, a `REQUEST_CONFLICT` and a refusal (the engine's or the broker's) spend
    nothing. Putting the count on the research record, with the lease number of ADR-0039, is not
    built: the broker keeps it per client and shows it only to tests. This is the counting
    [ADR-0034](0034-propose-fail-closed-retained-state-budgets.md) asks any budget to define.
11. **What restores a budget: only a new ticket.** A grant's spent budget lasts as long as the
    engine process (decision 4) and belongs to the grant, identified by its ticket's hash. An
    operator's purge ([ADR-0028](0028-operator-purge-file.md)) deletes perception state and the
    journal and does not restore a budget; reading the grants file again, changed or not, does not
    restore it either. A grant issued again under a new ticket starts a new budget. A count of
    deliveries is not screen data, so a purge has no reason to delete it, and restoring authority
    as a side effect of deleting data would fail open; issuing a new ticket is the operator's
    explicit act. The same holds for the term. While a client is connected, its budget and its
    term are ceilings: each is the meet of every value read for its ticket so far, so an edit of
    the grants file lowers them at the client's next call and never raises them, and a withdrawal
    followed by a new listing leaves both the count and the ceilings as they were. The broker keeps
    them per client; a ticket admits one client in a broker's life, so per client and per ticket
    are the same.
12. **A grant's remaining limits come later, in units of their own.** The kinds of action and the
    region are added on their own, because the broker and the engine must each check one of them
    and they are better reviewed alone. Until then a grant cannot express them, and the grants
    file refuses their keys, so nothing claims to enforce them. The delivery budget of decisions 10
    and 11 and the term are built.

Deferred: delegation from inside the contract (an orchestrator model that issues grants through a
tool). It would need a control tool and a change of chapter 07; the harness on the operator's side
covers the need without it.

### Open points

None is left in this ADR. The transport that carries the ticket has its own open points, in
ADR-0043.

## Alternatives considered

- **One policy for every client:** it is the present state and the reason for this ADR.
- **A bearer token in each request:** it travels through the model's context and can be extracted by
  the content the model reads.
- **Roles named in the client's own request:** a client would choose its own authority.
- **Grants that can add to the policy:** the operator's policy would stop being an upper bound, and
  every argument of chapter 08 that starts from the policy would have to be redone.
- **A separate engine per agent with its own policy file:** it is the several-process design that
  ADR-0039 rejects.
- **Filtering observations by application:** not implementable on the evidence an element has;
  it needs window evidence on every element, which is a dossier change.
- **A spent-list of tickets in the store:** it survives a restart, and it makes the engine write to
  the operator's store for the first time.
- **Applying the whole grant in a check before the engine:** the region needs the target's box,
  which, like the target's role, comes from the engine's registry, and only the engine has it. The
  action kind comes from the request and could be checked anywhere; the region could not.
- **Restoring a budget on a purge or a reload:** it would give authority back as a side effect of
  deleting data or of reading a file again.
- **Counting only the results that report delivered input:** an `indeterminate` attempt may have
  reached the desktop, so leaving it out would fail open.

## Consequences and cost

- The operator's policy remains the upper bound of everything any agent can act on, and that is a
  property of the construction: a meet is never above either operand.
- A sub-agent that is misled can do at most what its grant allows, for as long as its term and
  budget last within one engine process.
- A client may see elements of applications it cannot act on. Values and the region are the only
  limits on what it reads.
- More configuration: tickets, grants, their terms, and a grants file read on every call. A grant
  that is too narrow shows up as refused actions; the refusal says which limit applied without
  quoting the screen ([ADR-0021](0021-pin-every-loaded-artefact-and-keep-text-out-of-errors.md)).
- Filtering observations per client means the shared page of ADR-0039 is filtered at delivery, per
  client, and never cached already filtered for another.
- A process running as the same user can read a ticket file. It can also send input without the
  engine, so it is outside what the grant defends against; chapter 08 should say so explicitly.

## Required validation

Planned tests, on the simulated desktop with a fake clock and fixed seeds. Those of the
policy-shaped part now exist on local branches; "What exists" below says which, and under which
identifiers, which are named there because this branch's matrix does not have them yet. The rest
are still only planned.

- On generated policies, grants and actions: whatever the meet permits, the policy permits; the meet
  is the same in either order; narrowing twice equals narrowing by both.
- A client without a grant delivers no input event, a single client included. A client whose grant
  excludes the target's application, window, action kind or region delivers none either, each
  refused with `POLICY_DENIED` and a message naming its limit.
- The budget is never exceeded on generated sequences; a delivered or `indeterminate` result spends
  one, a two-phase `type` spends one whether or not its keys were sent, and a dry run or a refusal
  spends nothing.
- An operator's purge and a reload of the grants file leave a spent budget spent; the same grant
  under a new ticket starts a new budget.
- A grant that ends or is withdrawn refuses the next action; the stop refuses it whatever the grant.
  A grants file that is missing or malformed grants nothing.
- A ticket attaches once in one engine process; a second use is refused. After a restart of the
  process the ticket attaches again, as decision 4 states.
- An observation never contains an element outside the client's region, and never a value the
  client's grant or the policy does not allow. An element of an application the grant excludes
  may appear, and is never a target the client can act on.
- No refusal carries text from the screen or from the agent.
- The meet and its three laws are added to the formal model
  ([ADR-0042](0042-formal-model-as-a-third-oracle.md)). A first model exists outside the repository
  (`MultiAgente.lean`, delivered to the owner with the first draft): the meet permits exactly what
  both sides permit (`permits_meet_iff`), so no grant and no chain of grants permits what the policy
  refuses, and the meet is commutative, associative and idempotent.

## What exists (2026-11-27)

On branches that are not merged (branches are named rather than commits, because their heads
move):

- **`Grant`, its narrowing and its meet** (`feat/grant-meet`, `src/application/src/grant.rs`): a
  grant carries the applications, the window classes per application, an optional window pin,
  whether the client may deliver live input and whether it may read values. The default grant
  allows nothing. The broker binds a grant to a client by keeping one per connection, given when
  the client is admitted (`Broker::connect`).
- **The policy store that applies a grant** (`feat/engine-clients`, `GrantedPolicy` in
  `grant.rs`): it wraps the operator's store; while a client's grant is installed every read of
  the policy is narrowed by it, the operator's stop is read afresh each time, and dropping the
  installation puts back what was there, so an early return or a panic cannot leave one client's
  grant in force for the next.
- **The broker** (`feat/broker`, `src/adapters/mcp/src/broker.rs`) installs the calling client's
  grant under its one lock for the length of each tool call; a client admitted without a grant is
  served under the default grant (decision 7).
- **Tickets and the grants file** (`feat/transport-core` and `feat/broker-attach`): the ticket
  handshake, the spent set and `Broker::admit` in `broker.rs`; `GrantsFile`, the reader of
  `[safety] grants_file` read at every call and invalid as a whole on any problem
  (`src/app/src/grants.rs`); the executable serves them over the pipe of ADR-0043.
- **The delivery budget and the term** (`feat/grant-limits`): `Grant::delivery_budget` and
  `Grant::term` with their meet, `GrantLedger` (`grant.rs`), the grants file's optional keys
  `delivery_budget` and `term_ms`, and the broker's checks and refusals of decisions 6, 7, 9, 10
  and 11.

Of Required validation: the three laws of the meet, and narrowing by generated grants, are unit
tests of `grant.rs`; with the budget and the term they are UT-81, and the ledger is UT-93. IT-14
(`src/adapters/mcp/tests/broker.rs`) covers a client without a grant, alone and with others
(it observes without values and delivers nothing), a grant that leaves out the target's
application, and a grant whose window class or pin the window does not match (each refused at
the gate with `POLICY_DENIED`), and two clients with different permission to read values, each of
which sees its own view of one snapshot. IT-23 covers a withdrawn or changed grant, IT-24 and
IT-25 the ticket, UT-76 the grants file (its two new keys and the refusal of the keys of the
limits not built included). IT-37, IT-38 and IT-39 cover the budget and the term (generated
sequences and real threads included), with the scenarios BDD-72, BDD-74 and BDD-77. The rest is
still only planned: the action-kind and region limits as refusals, the region filter on
observations, and the meet in the formal model.

What does not exist: the kinds of action and the region (decision 12); the count on the research
record (decision 10).

## Reversibility

Grants exist only with the broker of ADR-0039, behind the same switch. Without grants configured the
fail-closed rule applies; removing the feature returns every client to the single policy. A case in
which the meet permitted something the policy refused would be a defect to fix before anything else.

## Provenance

The owner's request of 2026-11-22 for safer agent actions and orchestrators over several agents.
Revised on 2026-11-24 after reading the draft against the code
([review](../reviews/2026-11-24-adr-0039-0040-against-the-code.md)); the decisions were taken that
day under the owner's delegation. Refines ADR-0007 and ADR-0020; relies on ADR-0018, ADR-0028 and ADR-0034. The reasoning
is the principle of least privilege, fail-safe defaults and complete mediation (Saltzer and
Schroeder, 1975), capabilities as authority that is designated and can be attenuated (Dennis and Van
Horn, 1966; Miller, 2006) and the confused deputy (Hardy, 1988). None of these is in chapter 11 yet;
adding them is part of accepting this ADR.

## Resumen (ES)

Cada agente recibe una concesión ligada a su conexión que sólo puede estrechar la política local:
aplicaciones, ventana, tipos de acción, lectura de valores, un presupuesto de entregas, un plazo y,
si se quiere, una región. Con el intermediario encendido rige para todo cliente, también uno solo:
sin concesión válida se observa sin valores y no se entrega nada. Tiques, presupuestos y plazos
valen por proceso del motor. Lo observado no se filtra por aplicación, porque los elementos no la
llevan; se filtran los valores y la región. Las concesiones necesitan una sección y una lectura
nueva del almacén de política; la parte con forma de política se aplica decorando ese almacén, el
presupuesto y el plazo los comprueba el intermediario bajo su cerrojo a la entrada de cada llamada,
y la región se comprobará en el motor. El presupuesto lo gasta toda acción entregada o
indeterminada (un `type` en dos fases cuenta una vez); un ensayo, una repetición y un rechazo no
gastan nada; ni una purga ni una recarga lo restauran, sólo un tique nuevo. Mientras el cliente está
conectado, su presupuesto y su plazo son techos que un cambio del archivo puede bajar y nunca subir.
El tique llega a `attach` como ruta de un archivo, viaja en una primera línea del tubo y el
intermediario de `adapter-mcp` calcula su SHA-256 (ADR-0043). Existen en ramas sin fusionar `Grant`
con su estrechamiento y su encuentro, el almacén que aplica la concesión bajo el cerrojo del motor,
el intermediario, que sirve sin concesión válida bajo la concesión por defecto, los tiques, el
archivo de concesiones y el presupuesto y el plazo; los tipos de acción y la región no existen y
llegarán en unidades propias. No queda ningún punto abierto en esta ADR.
Revisada el 2026-11-24 contra el código; la aceptación es del responsable.
