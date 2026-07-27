# AstraOS Architecture

## Applications

- `apps/astra-cli` — command-line entry point

## Crates

- `astra-core` — shared application constants and common types
- `astra-config` — configuration model, defaults, paths, loading, saving, errors
- `astra-system` — typed operating-system snapshots and bounded local-service
  inspection
- `astra-workspaces` — workspace discovery and paths
- `astra-projects` — project-name validation and future scaffolding
- `astra-dashboard` — terminal lifecycle, dashboard state, refresh scheduling,
  keyboard input, and rendering
- `astra-context` — provider-neutral project analysis, immutable project
  facts, structured context, derived insights, and deterministic renderers

## Live dashboard

The interactive dashboard uses a single-threaded event loop with separate
timing for terminal rendering and system-metric refreshes. Rendering consumes
structured snapshots and never performs system or network probes.

`astra-system` collects CPU, memory, disk, battery, host, and uptime data.
Docker and Ollama probes use bounded synchronous I/O and return explicit
service states. Optional or unsupported metrics are represented as unavailable
instead of failing the dashboard.

`astra-dashboard` owns workspace selection and preserves the last valid metric
when a later optional refresh cannot provide a replacement. Terminal setup is
protected by a cleanup guard so raw mode, the alternate screen, and cursor
visibility are restored on normal and error exits.

## Project context engine

`astra-context` analyzes an explicitly selected project root without loading
AstraOS configuration, modifying the project, or calling an AI provider.
`ProjectAnalyzer` coordinates a fixed, read-only pipeline:

```text
Inventory → ManifestCatalog → FactGraphBuilder → immutable FactGraph
          → projection scanners → ProjectContext → InsightsEngine → ScanReport
```

Inventory, manifest, and Git scanners produce normalized facts. All other
scanners consume only the private immutable `FactGraph`; they do not traverse
the filesystem, invoke Git, parse manifests, mutate facts, or depend on one
another. The graph eliminates repeated discovery but is not serialized,
persisted, configured, cached, or exposed as a plugin API.

Private fact provenance carries a deterministic semantic scope. Files below
known fixture, test-data, example, and sample boundaries remain in the bounded
inventory, but only primary facts feed project-level projections. A selected
project root is always the semantic root, so its own directory name does not
cause the project to be treated as fixture data. Ordinary source tests remain
primary testing evidence.

`ScanReport` is the public result and contains a schema-versioned
`ProjectContext`, scanner results, diagnostics, and factual insights. Semantic
discoveries retain confidence and evidence. Runtime-only information such as
scan duration is excluded from deterministic serialization.

## Dependency direction

Applications may depend on crates. Lower-level crates should not depend on the
CLI application.
