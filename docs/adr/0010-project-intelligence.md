## Status

Accepted

## Context

Milestones 8–12 provide deterministic context, discovered actions, controlled
execution, and evidence-backed knowledge. They do not yet provide one coherent,
explainable view of a project from those inputs.

## Decision

Add `astra-intelligence`, a pure, read-only project-understanding crate.
The CLI gathers typed live observations, discovered actions, execution
capability information, and projected knowledge, then maps them into an
explicit `ProjectIntelligenceInput`. The deterministic analyzer constructs a
runtime relationship graph, a synthesized `ProjectIntelligence` model, and
evidence-backed derived insights.

> AstraOS will construct project intelligence from typed live observations,
> discovered capabilities, execution capability information, and projected
> knowledge. All derived insights must be deterministic, evidence-backed,
> explainable, and non-authoritative.

The crate never scans a filesystem, invokes Git, launches a process, reads a
knowledge store, writes storage, loads configuration, or depends on the CLI.
It has no wall-clock or random output. Its public model excludes absolute
paths, source content, command output, diffs, environment values, credentials,
and arbitrary knowledge payloads.

Intelligence distinguishes observed, derived, and operator-decided information.
Insights are derived only by stable named rules and do not become stored facts.
Milestone 13 remains read-only: it neither recommends commands nor plans or
executes them. Future Milestone 13.1 may add operator annotations, corrections,
and overrides without destroying this distinction.

The runtime intelligence graph is not the persisted `astra-knowledge`
relationship store. Persisted knowledge relationships describe durable claim
connections. Intelligence edges are current, evidence-backed model connections
derived from explicitly supplied inputs; an edge can cite a knowledge claim but
does not mutate it.

## Consequences

`astra project understand <PROJECT> [--json]` produces a deterministic report
from one typed model. It can describe unavailable information as a limitation
rather than inventing a negative conclusion. The CLI remains the integration
boundary and maps execution support into a neutral input, so
`astra-intelligence` has no dependency on `astra-execution`.

The initial rule set is deliberately narrow: multi-package workspace,
controlled evidence-producing verification, restricted discovered actions,
stale verification, unavailable operator decisions, and multiple workspace
packages. No AI, semantic search, graph database, operator editing, planning,
or orchestration is introduced.

## Alternatives considered

- **Put understanding in the CLI:** rejected because the deterministic model,
  graph, rules, and invariants are reusable platform behavior.
- **Let intelligence inspect projects or storage:** rejected because it would
  duplicate scanning and persistence responsibilities and hide I/O.
- **Depend directly on execution:** rejected because the CLI can map the small
  execution-capability contract into a neutral input.
- **Persist the graph:** deferred. Runtime graph edges model a current
  interpretation; durable relationships remain knowledge-owned.
