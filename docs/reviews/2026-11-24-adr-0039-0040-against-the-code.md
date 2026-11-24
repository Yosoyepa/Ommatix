# Review: the drafts of ADR-0039 and ADR-0040 read against the code

Found by an AI agent (Claude, as architect) on 24 November 2026, while planning the work the two
drafts propose, by reading [ADR-0039](../adr/0039-several-agents-through-one-engine-with-an-input-lease.md)
and [ADR-0040](../adr/0040-task-scoped-grants-narrower-than-the-local-policy.md) as written on
22 November against the code at `5c3e363` (PR #114, which proposes them). The author of this
revision, another AI agent session (Claude), checked every finding a second time against the same
commit before writing it down; where the first reading had a line or a wording wrong, the table
says so. Nothing here used the real screen, input or a model, and no production code changes.

The decisions that resolve the findings were taken the same day under the owner's delegation. Both
ADRs stay **Proposed**: acceptance is still the owner's. The revision was made in two commits: the
first (`1ffb6f4`) resolved the findings with decisions D2 to D9; an independent QA pass by another
vendor's agent then accepted it and listed three wording points, and decisions D11 to D14, taken
after it on points the first commit had left open, were folded in by the second (see
[Decisions taken after the first revision](#decisions-taken-after-the-first-revision)).

## Findings

Priorities: P1, the draft could not be implemented as written or two rules contradicted each
other; P2, the draft was wrong about the code or left a behaviour undefined; P3, an omission. The
planned tests have no identifiers yet; the ADRs assign them on acceptance.

### ADR-0039

| # | P | Finding | Check | Resolution | Covered by |
|---|---|---|---|---|---|
| A1 | P1 | `McpServer::serve` purges the engine when any connection ends (`src/adapters/mcp/src/server.rs`, lines 79–81; pinned by `src/adapters/mcp/tests/sec_07_deletion.rs`). A second client that disconnects would wipe the shared engine, while the draft said a disconnect only ends the lease. | Confirmed. | Decision 10: a connection that ends deletes only its own state (its lease, its place in the queue, its journal entries); the engine is purged when the last connection ends. D12: what a departed client observed stays until the last client leaves or the operator purges; chapter 08 §8.4 will say so when the broker is wired. | Planned: a connection that ends while others remain leaves the snapshot and the others' entries; the last one purges. The existing `sec_07_` tests keep the one-client case. |
| A2 | P2 | The Context said one engine serves one connection. `McpServer` is `Clone` over `Arc<Mutex<Engine>>` (`server.rs`, lines 18–28), `serve` is generic over streams (lines 68–82), each stream is a `Connection` over the shared engine (`protocol.rs`, lines 70–90), and `tools::call` holds the lock for the engine's work on a call (`tools.rs`, lines 108–179). | Confirmed; the plan cited no lines for `server.rs` and `tools.rs`, added here. | The Context states it; the broker is smaller than the draft suggested. | Documentation only. |
| A3 | P1 | One client holds the lease implicitly and, with no holder, nobody delivers; the draft did not say what happens to the implicit lease when a second client connects. | Confirmed by reading the draft. | Decision 9: the implicit lease becomes an ordinary lease whose term starts when the second client connects. | Planned: one client holds the lease implicitly; when a second connects, the first keeps it for one term and then loses it. |
| A4 | P1 | ADR-0039 decision 6 (one client: today's behaviour) contradicted ADR-0040 decision 5 (no grant: deliver nothing) for one client without a grant with the broker on. | Confirmed by reading both drafts. | Decision 9 of 0039 and decision 7 of 0040: with the broker on, grants govern every client, a single one included; the transparency covers the lease only. With the broker off nothing changes. | Planned: a single client without a grant delivers nothing; with a grant equal to the policy, the broker answers as the stdio server. |
| A5 | P2 | "A dry run needs no lease" and "only the holder's `execute_action` reaches the gate" disagreed for a non-holder's dry run. A dry run returns before any policy read (`src/application/src/engine/act.rs`, lines 329–331). | Confirmed, and made more precise: the dry run returns after two checks that come first, a spent or missing snapshot refuses every action, dry runs included (lines 294–298), and the age bound applies (line 327). | Decision 4: a client without the lease may dry-run on the shared page while that snapshot is current and unspent; after the holder's delivery it gets `STALE_SNAPSHOT`, as today. | Planned: a non-holder's dry run passes on the current shared page and is refused after the holder's delivery. |
| A6 | P2 | The shared page was "marked as shared and with its age"; an age depends on the run, so by engineering rule 1 it belongs in `_meta`. No public method serves the published page again: `first_page` and `next_page` are `pub(super)` (`src/application/src/engine/view.rs`, lines 166–197), `observe_screen` without a cursor always observes (`src/application/src/engine.rs`, lines 231–251), and a cursor binds the effective `include_values` (`view.rs`, line 185). | Confirmed. | Decision 4: the age, and whether the page is shared, go in `_meta`; the page is rebuilt per client, which needs a new engine method. The lease state of decision 5 goes in `_meta` for the same reason. | Planned: a non-holder's page carries its age in `_meta` and not in `structuredContent`, and the snapshot counter does not move. |
| A7 | P2 | The draft spoke of "the total bound of ADR-0034". `JOURNAL_CAPACITY = 1024` bounds only results that delivered no input; delivered and indeterminate results are kept without bound, keyed by `request_id` alone (`src/application/src/engine/journal.rs`, lines 7–20 and 92–99), and ADR-0034 was accepted as documentation only, with no budget. | Confirmed. | Decision 11: the per-client quota can only split the evictable part; it bounds nothing that is not bounded today. | Planned: a full quota evicts only its owner's results that delivered no input. |
| A8 | P2 | The draft did not say when the lease term is checked. A two-phase `type` polls the clock between its click and its keys (`act.rs`, lines 557–600; ADR-0018). | Confirmed; the plan cited no line, added here. | Decision 8: the lease is evaluated once, when a call enters the critical section; a delivery in progress completes. A holder renews only when nobody waits, with the same number, spending nothing. | Planned: a `type` whose term ends between click and keys completes and the next call is refused; renewal with and without waiters. Not yet in the formal model, which has no time. |
| A9 | P2 | Transport: `unsafe` is allowed only in the capture and input adapters (ADR-0016), SEC-04 bans sockets, and the lockfile check bans `mio` and `socket2`. The plan's inference was that the client side (`attach`) could open the pipe with `std::fs` alone. | Corrected. The constraints hold (`src/acceptance/tests/repository.rs`, lines 142–173 and 228–262), and there are more: the SEC-04 source check also forbids `tokio::net`, where `tokio`'s named pipes live, and `OpenOptions`, which a read-write open of a pipe with `std::fs` needs. So neither side of the pipe is free of an exception today. | Decision 12: the broker is built and tested on in-memory streams first; the pipe is a separate spike and a separate decision, and the ADR states the three constraints. D13 names a candidate, adopted only under its own ADR. | Documentation only until the spike. |
| A10 | P3 | The draft did not say who starts the engine process that the `attach` relays connect to. | Confirmed. | Listed as open, with the fact that an `attach` that starts the engine needs `Command::new`, which the SEC-04 source check forbids (`repository.rs`, line 159). | Documentation only. |

### ADR-0040

| # | P | Finding | Check | Resolution | Covered by |
|---|---|---|---|---|---|
| B1 | P1 | Elements carry no application, so "a client does not receive elements of an application its grant excludes" cannot be implemented on the evidence the engine has. | Confirmed: `StoredElement` (`src/application/src/engine/observe.rs`, lines 198–213) and `ElementView` (`src/application/src/contract.rs`, lines 216–230) have none. Made more precise: with the freshness switch of ADR-0032 on, the snapshot keeps the handle of the window under each target, for that check only (`src/application/src/engine/freshness.rs`, lines 122–127), never its application. | Decision 8: no filtering of observations by application, said plainly; values (both sides must allow them) and an optional region are filtered; the page and its cursor are per client. | Planned: an observation never contains an element outside the client's region or a value it may not read; an element of an excluded application may appear and is never a target. |
| B2 | P1 | Spent tickets, delivery budgets and terms would live in memory while the engine "writes nothing", so they reset when the engine restarts. | Confirmed by reading the draft against `PolicyStore` (`src/application/src/ports.rs`, lines 378–395), which has no write. | Decision 4: tickets, budgets and terms are per engine process, and the ADR says what a restart does. | Planned: a ticket attaches once per process and again after a restart. |
| B3 | P2 | "A change or a withdrawal takes effect at the next action, as the stop does." `ConfigPolicy` fixes everything at start except the stop, the window pin and the purge file (`src/app/src/policy.rs`, lines 106–122 and 214–257); `PolicyStore::current()` takes no client (`ports.rs`, lines 380–381); `SafetySection` refuses unknown keys (`src/app/src/config.rs`, lines 178–179). | Confirmed. | Decision 5: a grants file named by the configuration, read on every call, a missing or malformed file granting nothing; a new configuration key, a new reader in the app and a per-client read path. | Planned: a withdrawn grant refuses the next action; a missing or malformed grants file grants nothing. |
| B4 | P2 | There are three places a grant could be applied (a check in the broker, a decorating `PolicyStore` set under the engine lock, the engine); the region needs the target's box, which only the engine's registry knows. | Confirmed: the gate reads the target's box from the registry (`act.rs`, lines 315–318). | Decision 6, the architect's recommendation: the policy-shaped part through a decorating store, kind, budget, term and region in the engine. The policy-shaped meet exists unwired (`Grant` in `src/application/src/grant.rs`, commit `dd0579c`, local branch `feat/grant-meet`). | Its properties on generated cases, on that branch. The engine checks are planned. |
| B5 | P3 | `sha2` is not a dependency of `adapter-mcp`; adding it changes `Cargo.lock`, and REL-01 must still pass. | Corrected: `sha2` 0.11 is already a workspace dependency (`Cargo.toml`, line 54), used by the app, the vision adapter and the acceptance crate, so adding it to `adapter-mcp` changes that adapter's entry in `Cargo.lock` and adds no crate. The application crate may not depend on it (`repository.rs`, lines 288–295). | Open point of 0040: where the hash is computed depends on the transport. | Documentation only. |

## Decisions that differ from the first draft

- **The purge.** The draft left `serve`'s purge on every disconnect in place by not mentioning it.
  The revision purges on the last disconnect only (decision 10 of 0039).
- **One client is not "today's behaviour" when the broker is on.** The lease is implicit; the grant
  still governs (0039 decision 9, 0040 decision 7).
- **Open points O1, O2 and O3 of 0039 are closed:** the `lease` argument of `observe_screen`,
  `POLICY_DENIED` (chapter 07, line 59, classes it as a decision code and `PERMISSION_DENIED` as
  an adapter code), and the validity window of ADR-0025 as the term with at most eight clients.
- **The journal quota** no longer claims a total bound that ADR-0034 never set.
- **The transport** is no longer a decision of this ADR: in-memory streams first, the pipe later.
- **Observations are not filtered by application** (0040 decision 8); the draft promised it.
- **Tickets can be used again after a restart** (0040 decision 4); the draft implied a ticket was
  spent for good.
- **The age of the shared page and the lease state go in `_meta`**, not in the page.

## Decisions taken after the first revision

The first commit left five points open. Four decisions of the same day, under the same delegation,
settle two of them (D11 and D14 the budget of 0040, D12 the retained observations of 0039), which
are no longer listed as open, and name a transport candidate (D13), on which the three points still
open depend.

| # | Decision | Where | Reason | Covered by |
|---|---|---|---|---|
| D11 | A grant's delivery budget is spent by every action whose result the journal keeps as possible input: delivered input or `indeterminate` (`src/application/src/engine/journal.rs`, lines 22–25). A two-phase `type` counts once. A dry run and a refusal spend nothing. | 0040 decision 10 | Fail closed: an attempt whose effect is unknown may have reached the desktop. ADR-0034 asks any budget to define this. | Planned: the budget on generated sequences, with delivered, indeterminate and two-phase results. |
| D12 | What a client observed stays in the engine until the last client leaves or the operator purges. Chapter 08 §8.4 will say so when the broker is wired. | 0039 decision 10 | Perception state belongs to the desktop, not to a client; the operator's purge file still deletes it at any request boundary. | Planned: a departed client's observations stay until the last disconnect or the operator's purge. |
| D13 | Transport candidate: a Windows named pipe through the `interprocess` crate (`=2.4.9`, `default-features = false`, synchronous API), one blocking thread per connection with a synchronous line reader driving `Connection::handle_message`. Adopted only if an experiment supports it, and then under its own ADR (an exception to the SEC-04 source check if the `attach` side needs `OpenOptions`, the licences in `S4_REPRODUCIBILIDAD.md`, a CHANGELOG entry). The fallback is our own `unsafe` under a new exception in the manner of ADR-0016. | 0039 decision 12, as a candidate, not a decision of the ADR | A read-only spike kept outside the repository (U3, `U3-named-pipe.md`) found, from the crate's source, that it refuses remote clients and takes the first-instance flag by default and pulls in no `tokio`, `mio` or `socket2`; the other crates it looked at each fail a requirement or a repository check. Not yet run: the restriction to the current user, the build on this toolchain, flushing under pipelined traffic. | The experiment; then the transport's own ADR. |
| D14 | A spent budget lasts as long as the engine process and belongs to the grant, identified by its ticket's hash. An operator's purge and a reload of the grants file do not restore it; a grant issued again under a new ticket starts a new budget. | 0040 decision 11 | ADR-0034 asks a budget to say what a purge and a reload do. A count of deliveries is not screen data, and restoring authority as a side effect of deleting data would fail open. | Planned: a purge and a reload leave a spent budget spent; a new ticket starts a new one. |
| D16 | The experiment supports D13, so the transport is the named pipe through `interprocess`'s synchronous API, proposed as [ADR-0043](../adr/0043-local-named-pipe-transport-for-several-clients.md) and built after the broker. Carried from the experiment: the descriptor `D:P(A;;GA;;;OW)`, because the user's own SID cannot be obtained in safe Rust; the bound on clients enforced in the accept loop, because the pipe's instance limit makes a ninth client wait instead of refusing it; `widestring` as a direct dependency; three crates and two licence expressions in the lockfile and in `S4_REPRODUCIBILIDAD.md`. | ADR-0043; 0039 decision 12 points to it | An experiment run on 2026-11-24 in a scratch crate outside the repository: no banned crate; a second listener refused (OS error 5); the read-back descriptor grants the owner only; 10 000 pipelined requests matched and an oversized line refused; eight clients at once; no `unsafe` and no SEC-04 substring in its code. Not tested: a second local user and an elevated process (they need administrator rights), the MSVC host, the real `McpServer` over the pipe. | ADR-0043's required validation, the two administrator checks supervised by the owner before acceptance. |
| D17 | The operator, or an agent host's launcher acting with the operator's authority, starts the one engine process with the broker on. `attach` only connects: with no engine listening it fails with a clear error and starts nothing. A second engine on the same desktop is refused by the pipe: its name is fixed per user session and created first-instance, so a second `serve` cannot create it and exits. | 0039 decision 1; ADR-0043 decisions 4 and 8 | SEC-04 forbids `Command::new` in production code, and an engine started from the agent's side would put authority on the wrong side; the experiment showed the second listener refused. | Planned in ADR-0043: a second listener refused; `attach` without an engine starts nothing. |
| D18 | A ticket reaches `attach` as the path of a file (`--ticket-file`), never as an argument value or an environment variable; `attach` sends it as one handshake line before any MCP traffic, and the broker in `adapter-mcp` hashes it with SHA-256 (`sha2`, already in the workspace) and compares it with the grants file. The ticket never enters the model's context or a log. | 0040 decision 3; ADR-0043 decision 9 | An argument is visible in the process list and the environment is inherited by children; a file can be restricted and removed after use. Reading a file is not a call SEC-04 forbids; `adapter-mcp` owns the wire. | Planned in ADR-0043: the handshake on in-memory streams, the ticket never in a reply or a log. |

The same commit takes in the three wording points of the QA pass: the page a client without the
lease receives has the same `structuredContent` shape as the holder's, each being built with the
client's own `include_values` and grant (0039 decision 4); in the alternatives of 0040, only the
region's box and the target's role come from the registry, while the action kind comes from the
request, as decision 6 says; and the Spanish summary of 0040 lists every point still open. It also
records that the lease state machine exists unwired (`LeaseTable` in `src/application/src/lease.rs`,
commit `fd53a21`, local branch `feat/lease-machine`).

Three later decisions, D16 to D18, settle the three points the second commit still left open:
the transport is proposed as its own ADR (D16), who starts the engine and how a second one is
refused (D17), and where the ticket is hashed (D18). They were folded in by a third commit, which
also proposes [ADR-0043](../adr/0043-local-named-pipe-transport-for-several-clients.md); neither
ADR-0039 nor ADR-0040 has an open point left.

## What stays open

- In [ADR-0043](../adr/0043-local-named-pipe-transport-for-several-clients.md): a pipe name taken
  by another process before the engine starts, and how the engine refuses to start the broker
  when elevated; its acceptance also waits for the two checks that need administrator rights.
- The lease's time, waiting order and renewal in the formal model, which has no time
  ([ADR-0042](../adr/0042-formal-model-as-a-third-oracle.md)).

## Evidence

- Every file and line cited above exists at `5c3e363`, except `src/application/src/grant.rs`,
  which is on the local branch `feat/grant-meet` (`dd0579c`), and `src/application/src/lease.rs`,
  which is on `feat/lease-machine` (`fd53a21`). The files and lines that ADR-0043 cites were
  checked at the commit that proposes it.
- The ADRs, this record and the index are documentation; the repository invariants
  (`cargo test -p ommatix-acceptance --test repository`) and the thesis checks
  (`referencia_ejecutable/run_tests.py`, `validar_paquete.py`) were run on the change.
