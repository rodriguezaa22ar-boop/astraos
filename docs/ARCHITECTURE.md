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

## Dependency direction

Applications may depend on crates. Lower-level crates should not depend on the
CLI application.
