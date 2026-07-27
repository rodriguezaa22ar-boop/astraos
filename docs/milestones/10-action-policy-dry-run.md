# Milestone 10: Action Policy and Dry-Run Planning

This document records the Milestone 10 boundary. Milestone 11 preserves its
dry-run behavior and adds a separate, state-bound execution path for `check`.

## Objective

Introduce the first decision layer between discovered project actions and
future execution. AstraOS selects an existing typed action, evaluates a strict
allowlist, and renders a deterministic plan without starting a process.

## CLI contract

~~~
astra project run <NAME> <ACTION> --dry-run [--json]
~~~

`ACTION` is one of `build`, `check`, or `test`, and must already be exposed by
the selected project. Omitting `--dry-run` returns:

~~~
astra: real action execution is not available; use --dry-run
~~~

The existing `project list`, `inspect`, `commands`, and explicit `create`
commands remain unchanged.

## Architecture

~~~
registered project
        ↓
ProjectAnalyzer
        ↓
astra-actions::resolve_actions
        ↓
typed action selection
        ↓
ActionPolicy
        ↓
ExecutionPlan → DryRunReport
        ↓
human or JSON renderer
~~~

`astra-actions` owns policy, path validation, plan models, and stable report
serialization. `astra-cli` owns argument parsing, registry resolution,
context orchestration, and presentation. No executor or process abstraction is
introduced.

## Policy rules

The initial policy allows exactly:

~~~
cargo build --workspace
cargo check --workspace
cargo test --workspace
~~~

The executable must be exactly `cargo`. The action ID and first Cargo
subcommand must agree. The only accepted additional argument is
`--workspace`, in that exact position. Other Cargo flags—including
`--features`, `--package`, `--manifest-path`, `--target`, `--config`, `--`,
and shell-like tokens—are rejected.

Working directories are resolved against the canonical project root when
relative, canonicalized when present, and accepted only when equal to or
inside the canonical project root. Missing paths, outside paths, and symlink
escapes are rejected using path-aware comparisons.

## Dry-run report

`astra project run NAME ACTION --dry-run --json` emits schema version 1:

~~~json
{
  "schema_version": 1,
  "project": {
    "name": "astraos",
    "root": "/path/to/astraos"
  },
  "action": {
    "id": "check",
    "executable": "cargo",
    "arguments": ["check", "--workspace"],
    "working_directory": "/path/to/astraos",
    "source": "context_engine",
    "confidence": "high"
  },
  "policy": {
    "decision": "allowed"
  },
  "execution": {
    "mode": "dry_run",
    "process_started": false
  }
}
~~~

Rejected policy plans use `decision: "rejected"` and a stable reason
identifier, return a nonzero status, and still report
`process_started: false`. Unknown projects, unsupported action names, and
actions unavailable from the project return concise CLI errors before a plan
is produced.

## Human output

Successful output identifies the project, action, canonical working directory,
executable, separated arguments, policy decision, and:

~~~
Dry run complete. No process was started.
~~~

Rejected plans state that no process was started and return a policy error.
No debug formatting or tracing timestamps are included in normal output.

## Read-only guarantees

The run command does not invoke a shell, spawn a process, write configuration,
modify project files, modify Git state, create caches, or persist history. Its
context pass disables the context engine's repository-process boundary, so a
Git repository is reported with the existing unavailable Git diagnostic rather
than causing Git to start.
Missing configuration is read through `load_if_present()` and remains missing.
Filesystem and manifest context analysis remains active, but the discovered
action is never executed.

## Tests

`astra-actions` tests cover supported mappings, executable/subcommand and
argument rejection, deterministic policy serialization, root/child/relative
working directories, missing and outside paths, traversal, symlink escapes,
canonicalization, repeatability, and no file mutation.

CLI tests cover help, all three dry-run actions, JSON schema and
`process_started: false`, missing `--dry-run`, unknown and missing projects,
unsupported and unavailable actions, empty `PATH`, missing configuration,
unchanged project files, and preservation of Milestone 9 behavior.

Tests use temporary projects and isolated configuration paths. They do not
depend on installed Cargo, shells, network access, user configuration, or the
real AstraOS checkout.

## Known limitations

- Only Cargo workspace `build`, `check`, and `test` are supported.
- Only the `--workspace` argument is allowed.
- There is no process execution, output capture, policy configuration,
  history, retry, cancellation, parallelism, or remote execution.
- Policy rejection rendering is diagnostic and returns a nonzero CLI status.

## Validation

~~~bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
./scripts/astraos-milestones.sh validate
git diff --check
~~~

## Manual verification

Use an isolated temporary `ASTRA_CONFIG_DIR` and a temporary registered Cargo
workspace:

~~~bash
cargo run --bin astra -- project --help
cargo run --bin astra -- project run --help
cargo run --bin astra -- project run <name> build --dry-run
cargo run --bin astra -- project run <name> check --dry-run
cargo run --bin astra -- project run <name> test --dry-run
cargo run --bin astra -- project run <name> check --dry-run --json
cargo run --bin astra -- project run <name> check
cargo run --bin astra -- project run <name> deploy --dry-run
cargo run --bin astra -- project run nonexistent check --dry-run
~~~

Confirm that every successful report says `process_started: false` in JSON or
`No process was started` in human output, and that the temporary configuration
and fixture are removed afterward.
