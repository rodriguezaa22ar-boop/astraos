# Milestone 6: Live Dashboard

## Objective

Turn the existing Ratatui scaffold into a continuously updating terminal
dashboard for local system, developer-service, and AstraOS workspace status.
The dashboard remains responsive, handles optional services and metrics
gracefully, and restores the terminal after normal and error exits.

## Implemented scope

The live dashboard remains available through:

```text
astra dashboard --interactive
```

The static dashboard remains the default for `astra dashboard` and for an
invocation without a subcommand.

## Architecture boundaries

- `astra-system` owns typed system snapshots, battery inspection, and bounded
  Docker and Ollama probes.
- `astra-dashboard` owns terminal setup and cleanup, application state,
  refresh scheduling, workspace selection, keyboard handling, formatting,
  layout, and rendering.
- `astra-cli` loads the initial configuration and invokes the dashboard.
- `astra-workspaces` remains the source of deterministic workspace registry
  ordering.
- `astra-config` retains its existing persistent schema.

Rendering functions consume structured state and do not perform system,
network, filesystem, or subprocess probes.

## Refresh model

- Render/input tick: 100 ms.
- System metrics refresh: one second.
- Developer-service probes: cached for five seconds to avoid excessive Docker
  subprocesses; `r` bypasses the cache.
- Workspace configuration: re-read without writing during refresh. Invalid or
  temporarily unavailable configuration preserves the last valid list.

The event loop is synchronous and single-threaded. Docker and Ollama operations
have strict 500 ms bounds.

## Visible panels

### Header

- AstraOS name
- application version
- Overview view name
- current local time

### System

- operating system
- hostname
- CPU usage
- used and total memory
- used and total disk space for the selected workspace filesystem
- battery charge and state when supported
- system uptime

### Developer services

- Docker
- Ollama

### Workspaces

- configured workspace count
- deterministic workspace names and paths
- selected workspace
- explicit empty state

Long paths are truncated on character boundaries.

## Keyboard controls

```text
q or Esc       Quit
r              Refresh metrics and services immediately
Up/Down        Select workspace
j/k            Select workspace
```

The dashboard does not open, create, edit, or remove workspaces.

## Service-status semantics

- `Running`: the daemon or local endpoint responded successfully.
- `Stopped`: the service is installed or was probed but is not reachable,
  including a bounded timeout.
- `Unavailable`: the corresponding executable is not installed and no service
  was reachable.
- `Unknown`: a probe failed in a way that does not reliably indicate running
  or stopped.

Docker uses `docker info`, which checks daemon reachability rather than only
checking for an executable. Ollama uses `GET /api/version` on loopback and
never starts the service or downloads models.

## Unsupported or unavailable metrics

Optional values are rendered as `unavailable`. Missing battery hardware,
unsupported system data, stopped services, and individual probe failures do
not terminate the dashboard. The last valid optional metric or service state
is retained when a later refresh only yields an unknown result.

## Test coverage

`astra-system` tests cover:

- service-state mapping
- Docker child exit and timeout mapping
- Ollama HTTP response parsing
- malformed response handling
- unavailable and invalid battery data
- command lookup without shell execution

`astra-dashboard` tests cover:

- initial state and refresh timing
- immediate manual refresh
- quit and refresh keys
- empty, single, and multiple workspace navigation
- deterministic ordering and selection clamping
- last-valid snapshot preservation
- human-readable units and uptime
- battery presentation
- Unicode-safe truncation

Existing CLI workspace integration tests remain unchanged.

## Known limitations

- The live dashboard remains opt-in through `--interactive`.
- Ollama probing uses the default local loopback port `11434`.
- Docker status follows the daemon selected by the installed Docker CLI.
- The single-threaded loop can pause briefly during a service check, but each
  probe is bounded to 500 ms and checks are cached.
- Metrics are current values only; no history or charts are stored.

## Validation

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
./scripts/astraos-milestones.sh validate
```
