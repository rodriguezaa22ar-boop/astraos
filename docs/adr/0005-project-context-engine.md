# ADR 0005: Build Project Context Through an Immutable Fact Graph

## Status

Accepted

## Context

AstraOS needs deterministic project knowledge that can support future AI
adapters, prompt construction, planning, repository memory, and distributed
analysis without coupling project discovery to a provider. Project facts come
from several sources, including files, manifests, and Git. Allowing every
consumer to rediscover those facts would duplicate parsing, produce
inconsistent conclusions, and make provenance difficult to preserve.

The serialized context will become a long-lived platform contract. Runtime
implementation details must not leak into that contract, and read-only project
inspection must remain independent of AstraOS configuration, dashboards,
terminal orchestration, and AI services.

## Decision

Create a dedicated `astra-context` crate with `ProjectAnalyzer` as its public
coordinator. Analysis follows one phased flow:

```text
Selected Project Root
        ↓
Inventory Phase
        ↓
ManifestCatalog
        ↓
FactGraphBuilder
        ↓
Immutable FactGraph
        ↓
Projection Scanners
        ↓
ProjectContext
        ↓
InsightsEngine
        ↓
ScanReport
        ↓
Renderers
```

Inventory, manifest, and Git scanners are the only fact-producing scanners.
They normalize observations through `FactGraphBuilder`. Once `finish` is
called, the resulting `FactGraph` is immutable.

All other scanners are projections. They consume only immutable graph input
and may not access the filesystem, execute Git, parse manifests, mutate facts,
or depend on another projection scanner's output.

The graph and its IDs remain private. It is neither serialized nor exposed as
a public query API. It is not persistent storage, a cache, a graph database, a
configuration surface, or a plugin interface.

Private fact provenance records whether an observation belongs to primary
project content, embedded fixture/test data, or an example. Inventory retains
all bounded observations, while project-level projections consume only
primary facts. Classification uses paths relative to the selected root so a
legitimate selected root named `fixtures` remains primary and ordinary
directories named `tests` are not globally excluded.

`ProjectAnalyzer` returns `ScanReport`, containing a versioned
`ProjectContext`, scanner results, diagnostics, and factual derived insights.
Every semantic discovery uses `Detected<T>` to retain confidence and public
evidence. Runtime duration is excluded from serialization.

`InsightsEngine` is a pure derivation stage over the completed context and
immutable graph. It performs no I/O, mutation, recommendations, prompt
generation, or AI calls.

Milestone 8 adds no persistent configuration or cache. It uses deterministic
bounded scanning and a private, fakeable boundary for local, noninteractive
Git inspection.

## Alternatives considered

- Extend `astra-projects` with project inspection
- Put repository discovery in `astra-system`
- Implement analysis directly in `astra-cli`
- Let each scanner traverse files and parse manifests independently
- Serialize the internal graph as the public context
- Expose a public scanner or graph plugin interface
- Add a model-specific AI context command
- Add persistent caching before defining invalidation semantics
- Use a Git library or asynchronous runtime

## Consequences

- Project discovery is provider-neutral and reusable by future consumers.
- A single immutable fact source prevents downstream rediscovery and
  inconsistent manifest interpretation.
- Fact provenance can be projected into stable public evidence without
  exposing internal IDs.
- Embedded fixture repositories remain observable internally without
  distorting production project summaries.
- Scanner ordering cannot create hidden scanner-to-scanner dependencies.
- Deterministic IDs and ordering require normalization during graph
  construction.
- Projection scanners are easier to test because they use synthetic graphs
  rather than live repositories.
- The public schema must be evolved deliberately through
  `PROJECT_CONTEXT_SCHEMA_VERSION`.
- New ecosystem support requires fact production and projection work, but
  does not require changes to the pipeline.
- Persistent caching and external scanner extensibility remain future
  decisions.
