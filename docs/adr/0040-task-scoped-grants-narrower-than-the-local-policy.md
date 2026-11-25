# 0040. Give each agent a task-scoped grant that can only narrow the local policy

- **Status:** Proposed
- **Date:** 2026-11-22
- **Approval:** Pending. Draft written by an AI agent at the owner's request; nothing in it is
  implemented.

## Context

The local policy is one per process: whether actuation is enabled, whether field values may be
returned, the allowed applications, their window classes, the operator's pin on one window and the
stop (`LocalPolicy`, [ADR-0007](0007-fail-closed-actuation-and-honest-simulation.md),
[ADR-0020](0020-operator-scope-on-windows.md)). The agent has no port to write it, and the engine
reads it on every action and again before each delivery.

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
   when it satisfies both pins; values are returned when both say so; actuation is enabled when both
   enable it. The operator's stop is not part of a grant and always wins.
2. **A grant carries only limits:** the client it is bound to; a subset of the applications; an
   optional window pin; the kinds of action the client may deliver; whether it may read values; a
   budget of delivered actions, where zero means it may only observe; a term on the engine's clock;
   and an optional region of the frame outside which it has no target.
3. **A grant is bound to a connection, not quoted by the agent.** A client attaches with a one-use
   ticket that the operator's side creates and the host passes to the `attach` mode. The policy
   store lists the SHA-256 of each ticket next to its grant, so the engine only reads and compares,
   and writes nothing. The ticket selects the grant and is spent at once. Nothing the model writes
   can name or change a grant.
4. **Grants live in the policy store.** The operator, or an orchestrator's harness running with the
   operator's authority, writes them as it writes the rest of the policy. The engine keeps reading
   the store on every action, so a change or a withdrawal takes effect at the next action, as the
   stop does. The agent-facing contract keeps its three tools and gains no way to write a grant.
5. **Fail closed.** With the broker on, a client without a valid grant may observe without values
   and may deliver nothing. With the broker off nothing changes.
6. **The check sits where the policy is read** in the gate and before delivery, and what is observed
   is filtered by the same grant: a client does not receive elements of an application its grant
   excludes.
7. **A budget is spent by a delivery**, not by a request, a dry run or a refusal, and the count is
   on the research record with the lease number of ADR-0039.

Deferred: delegation from inside the contract (an orchestrator model that issues grants through a
tool). It would need a control tool and a change of chapter 07; the harness on the operator's side
covers the need without it.

## Alternatives considered

- **One policy for every client:** it is the present state and the reason for this ADR.
- **A bearer token in each request:** it travels through the model's context and can be extracted by
  the content the model reads.
- **Roles named in the client's own request:** a client would choose its own authority.
- **Grants that can add to the policy:** the operator's policy would stop being an upper bound, and
  every argument of chapter 08 that starts from the policy would have to be redone.
- **A separate engine per agent with its own policy file:** it is the several-process design that
  ADR-0039 rejects.

## Consequences and cost

- The operator's policy remains the upper bound of everything any agent can do, and that is a
  property of the construction: a meet is never above either operand.
- A sub-agent that is misled can do at most what its grant allows, for as long as its term and
  budget last.
- More configuration: tickets, grants, their terms. A grant that is too narrow shows up as refused
  actions; the refusal must say which limit applied without quoting the screen
  ([ADR-0021](0021-pin-every-loaded-artefact-and-keep-text-out-of-errors.md)).
- Filtering observations per client means the shared page of ADR-0039 is filtered at delivery, per
  client, and never cached already filtered for another.
- A process running as the same user can read a ticket file. It can also send input without the
  engine, so it is outside what the grant defends against; chapter 08 should say so explicitly.

## Required validation

New planned tests, with identifiers to be assigned in the matrix when the ADR is accepted, on the
simulated desktop with a fake clock and fixed seeds.

- On generated policies, grants and actions: whatever the meet permits, the policy permits; the meet
  is the same in either order; narrowing twice equals narrowing by both.
- A client without a grant delivers no input event. A client whose grant excludes the target's
  application, window, action kind or region delivers none either, each with its own refusal.
- The budget is never exceeded on generated sequences, and a dry run or a refusal does not spend it.
- A grant that ends or is withdrawn refuses the next action; the stop refuses it whatever the grant.
- A ticket attaches once; a second use is refused.
- An observation never contains an element the client's grant excludes, values included.
- No refusal carries text from the screen or from the agent.
- The meet and its three laws are added to the formal model
  ([ADR-0042](0042-formal-model-as-a-third-oracle.md)). A first model exists outside the repository
  (`MultiAgente.lean`, delivered to the owner with this draft): the meet permits exactly what both sides permit
  (`permits_meet_iff`), so no grant and no chain of grants permits what the policy refuses, and the
  meet is commutative, associative and idempotent.

## Reversibility

Grants exist only with the broker of ADR-0039, behind the same switch. Without grants configured the
fail-closed rule applies; removing the feature returns every client to the single policy. A case in
which the meet permitted something the policy refused would be a defect to fix before anything else.

## Provenance

The owner's request of 2026-11-22 for safer agent actions and orchestrators over several agents.
Refines ADR-0007 and ADR-0020. The reasoning is the principle of least privilege, fail-safe defaults
and complete mediation (Saltzer and Schroeder, 1975), capabilities as authority that is designated
and can be attenuated (Dennis and Van Horn, 1966; Miller, 2006) and the confused deputy (Hardy,
1988). None of these is in chapter 11 yet; adding them is part of accepting this ADR.

## Resumen (ES)

Cada agente recibe una concesión ligada a su conexión que sólo puede estrechar la política local:
aplicaciones, ventana, tipos de acción, lectura de valores, un presupuesto de entregas, un plazo y,
si se quiere, una región. El motor actúa bajo la intersección de política y concesión, de modo que
la política del operador sigue siendo el límite superior por construcción. El agente no cita ningún
secreto, y las concesiones se escriben desde el lado del operador, sin tocar las tres herramientas.
