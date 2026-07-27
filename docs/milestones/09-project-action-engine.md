# Milestone 9: Project Action Engine Foundation

## Objective

Expose deterministic, read-only project actions derived from the existing
Project Context Engine. This milestone establishes the command contract and
typed discovery layer; it does not execute processes.

## CLI surface

```text
astra project list
astra project inspect <NAME> [--json]
astra project commands <NAME> [--json]
astra project create <KIND> <NAME>
```

`NAME` is resolved through the existing AstraOS workspace registry. The
previous ambiguous `astra project <KIND> <NAME>` form is replaced by the
explicit `create` subcommand, preserving the existing scaffolding behavior.

## Architecture

```text
registered project path
        ↓
ProjectAnalyzer
        ↓
structured validation commands
        ↓
astra-actions resolver
        ↓
ProjectActionReport
        ↓
human or JSON renderer
```

The CLI remains a thin integration boundary. `astra-actions` does not scan
files, load configuration, execute Git, spawn commands, or mutate projects.

## Action model

Each `ProjectAction` contains:

- stable `ActionId`: `build`, `check`, or `test`;
- structured executable and `Vec<String>` arguments;
- resolved working directory (relative context paths become absolute under the
  selected canonical project root at the CLI boundary);
- `ActionSource::ContextEngine`;
- reused context `Confidence`.

The resolver recognizes only a `cargo` executable whose first argument is
`build`, `check`, or `test`. It preserves every argument exactly and ignores
unsupported executables and Cargo subcommands. Duplicate candidates select
the highest confidence; equal-confidence candidates use lexical working
directory, arguments, and executable ordering. Final output is ordered build,
check, test.

## Human output

```text
Available actions for astraos

ACTION  COMMAND
build   cargo build --workspace
check   cargo check --workspace
test    cargo test --workspace
```

Arguments are shell-safe for display only. No displayed string is used as an
execution input. Projects without supported actions succeed with a clear empty
state message.

## JSON contract

`astra project commands NAME --json` emits schema version 1:

```json
{
  "schema_version": 1,
  "project": {
    "name": "astraos",
    "root": "/path/to/astraos"
  },
  "actions": [
    {
      "id": "build",
      "executable": "cargo",
      "arguments": ["build", "--workspace"],
      "working_directory": "/path/to/astraos",
      "source": "context_engine",
      "confidence": "high"
    }
  ]
}
```

The report contains no timestamps, generated IDs, shell strings, or hidden
execution state. Human and JSON views use the same typed action report.

## Error behavior

- Unknown names return `unknown project: NAME`.
- Missing or non-directory registry paths return a concise project-path error.
- Context scan errors are returned without executing any detected action.
- Configuration load failures remain configuration errors because the registry
  is required for project commands. Read-only project commands do not create a
  default configuration file when none exists; the explicit `project create`
  command retains its existing configuration behavior.

## Tests

Unit tests cover Cargo mapping, unsupported commands, argv preservation,
working directories, confidence and duplicate resolution, ordering, and stable
serialization. CLI tests cover help, list, inspect, text and JSON commands,
unknown/missing projects, empty action sets, isolated temporary registries,
malformed-config resilience of existing context behavior, and proof that
action discovery succeeds with an empty `PATH`.

Tests do not use the real AstraOS registry, execute Cargo, require an AI
provider, or depend on developer machine state.

## Known limitations

- Only Cargo `build`, `check`, and `test` are recognized initially.
- No action executor, policy engine, shell support, history, parallelism,
  remote execution, or dashboard integration exists yet.
- Registry paths are still governed by the existing AstraOS workspace
  configuration. Action discovery does not persist configuration, while the
  explicit `project create` command retains its existing persistence behavior.

## Validation

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```

## Manual verification

Use an isolated `ASTRA_CONFIG_DIR` containing a temporary registered Cargo
workspace, then run:

```bash
cargo run --bin astra -- project --help
cargo run --bin astra -- project list
cargo run --bin astra -- project inspect <name>
cargo run --bin astra -- project inspect <name> --json
cargo run --bin astra -- project commands <name>
cargo run --bin astra -- project commands <name> --json
cargo run --bin astra -- project commands nonexistent
```

Confirm that actions are listed but no Cargo process starts, and that unknown
and missing projects fail without duplicate timestamped error output.
