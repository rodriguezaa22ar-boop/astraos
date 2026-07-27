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
- `astra-actions` — typed action discovery, strict policy evaluation, and
  deterministic dry-run planning; it does not execute processes

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

## Project action engine

`astra-actions` is a downstream adapter from `astra-context` to a small,
typed action vocabulary. The CLI resolves a registered project through
`astra-workspaces`, asks `ProjectAnalyzer` for its structured validation
commands, and passes those commands to the pure action resolver:

```text
registered project
        ↓
ProjectAnalyzer → validation CommandSpec values
        ↓
astra-actions resolver → ProjectAction values
        ↓
human or versioned JSON rendering
```

The initial vocabulary is deliberately limited to Cargo `build`, `check`, and
`test`. Executable paths, argument vectors, working directories, source, and
confidence remain structured. No shell strings are stored or interpreted, and
the resolver and policy never spawn a process. The policy permits only the
current context-engine shape `cargo <build|check|test> --workspace` and
requires the canonical working directory to remain inside the project root.

`astra-execution` is the separate state-bound execution boundary. It owns
fingerprints, bounded Git state capture, authorized plans, direct process
launching, and execution-result models. A plan binds the repository-wide HEAD
plus staged, unstaged, and relevant non-ignored untracked state in the selected
canonical project subtree. Before launch, the exact plan, action, policy
version, project root, and source binding are revalidated; stale plans are
refused. After launch, the same state is captured again and the result
distinguishes a verified check from command failure or source-state change.

`astra project run <NAME> check` is the only real execution path. It invokes
the approved Cargo argv directly, never through a shell. `build` and `test`
remain dry-run only. JSON execution results are versioned and keep child
diagnostics on stderr so stdout remains machine-readable. Execution history,
policy configuration, retries, parallelism, and remote actions remain future
work.

## Dependency direction

Applications may depend on crates. Lower-level crates should not depend on the
CLI application. The dependency flow for controlled checks is
`astra-context` → `astra-actions` → `astra-execution` → `astra-cli`; neither
execution crate depends on the CLI, and `astra-actions` remains process-free.
