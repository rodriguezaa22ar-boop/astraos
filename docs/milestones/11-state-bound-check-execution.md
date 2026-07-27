# Milestone 11: State-Bound Controlled Check Execution

## Objective

Turn the Milestone 10 dry-run boundary into a narrowly controlled execution
path for the existing Cargo workspace check. A command may run only when its
typed action, policy decision, project root, and source state still match the
authorized plan.

## CLI

```text
astra project run <NAME> check
astra project run <NAME> check --json
astra project run <NAME> build --dry-run [--json]
astra project run <NAME> test --dry-run [--json]
```

`check` without `--dry-run` is the only real execution path. `build` and
`test` remain dry-run only. Existing `project list`, `inspect`, `commands`,
and explicit `create` behavior is preserved.

## Architecture

The CLI uses the existing context engine and action resolver, then delegates
execution to the new `astra-execution` crate:

```text
ProjectAnalyzer
    ↓ structured validation command
astra-actions::resolve_actions + ActionPolicy
    ↓ exact cargo check action
astra-execution::ExecutionEngine::plan
    ↓ AuthorizedExecutionPlan
revalidate exact plan and source state
    ↓ direct argv process launch
post-run Git capture → ExecutionResult / VerificationVerdict
```

`astra-actions` stays read-only and process-free. `astra-execution` owns
fingerprints, state capture, plan/result schemas, and the private process
launcher. The CLI does not reconstruct or reinterpret commands.

## State binding and bounds

Each plan records:

- canonical selected project root;
- canonical containing repository root;
- repository-wide `HEAD` commit identity;
- staged diff fingerprint restricted to the selected subtree;
- unstaged tracked diff fingerprint restricted to the selected subtree;
- sorted, non-ignored untracked paths and content fingerprints restricted to
  the selected subtree;
- a combined state fingerprint.

The default capture limits are:

- 64 MiB per Git command output stream;
- five seconds per Git state command;
- 1 MiB per untracked regular file;
- 16 MiB total untracked regular-file content;
- 1,000 relevant untracked paths;
- 64 KiB retained Git error context.

Git is invoked with structured arguments, no shell, no pager, no terminal
prompt, no lazy fetch, and no user/system Git configuration. Ignored files
are not part of the binding. For a nested project, sibling changes do not
invalidate the selected project, while a new repository `HEAD` does.

## Fingerprints and revalidation

Fingerprints are SHA-256 values encoded as lowercase 64-character hexadecimal
strings. Length-prefixed fields avoid delimiter ambiguity. Action fingerprints
include policy version, action identity, executable, argument count and argv,
working directory, source, and confidence. Plan fingerprints include the
project identity, schema/policy versions, action fingerprint, and combined
state fingerprint.

Before launch, the engine checks plan and policy versions, canonical root,
policy-normalized action, action fingerprint, current source binding, and plan
fingerprint. Any mismatch returns a typed error and no process is started.

After launch, the state is captured again. The result reports one of:

- `verified_check` — exit code 0 and unchanged bound state;
- `command_failed` — nonzero exit and unchanged bound state;
- `source_state_changed` — exit code 0 but state changed during execution;
- `command_failed_and_source_state_changed`;
- `interrupted`.

There is no rollback. If a command fails after starting, subsequent work stops
and the original process outcome is preserved. A partial source change or
post-execution capture failure is reported explicitly.

## Output

Human mode prints the approved plan, separated executable/arguments, state,
action, and plan fingerprints, followed by a concise check result. JSON mode
prints a versioned `ExecutionResult` on stdout; child stdout and stderr are
forwarded to stderr so stdout remains parseable. The result contains no source
contents, timestamps, generated IDs, or nondeterministic ordering. Duration is
runtime metadata in the result and is not a planning input.

## Tests

`astra-execution` tests cover fingerprint format and determinism, plan
material binding, serialized state without source contents, bounded and
malformed Git output, ignored/untracked/staged/unstaged/rename transitions,
repository HEAD changes, nested sibling isolation, state limits, exact argv
and working-directory process invocation, stale-plan refusal before a fake
launcher, and verdict mapping.

CLI integration tests use temporary projects, isolated configuration, local
Git identity, and a generated lockfile. They cover clean and dirty real checks,
valid JSON output, child failure and exit status, non-Git rejection, dry-run
non-execution, existing project commands, and concise failures. No test uses
the real AstraOS configuration or repository.

## Known limitations and deferred work

- Only `cargo check --workspace` can execute.
- `build` and `test` remain dry-run only.
- Execution is synchronous and has no cancellation, retry, history, rollback,
  parallelism, remote execution, or dashboard integration.
- Git state capture is intentionally bounded; repositories with oversized
  untracked state or Git output are refused rather than partially fingerprinted.
- A file can change during the state-capture race itself; post-run comparison
  reports what the bounded captures observe.

## Validation

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
./scripts/astraos-milestones.sh validate
git diff --check
```

## Manual verification

Use a temporary HOME and `ASTRA_CONFIG_DIR` containing a registered temporary
Git Cargo workspace. Then run:

```bash
target/release/astra project --help
target/release/astra project list
target/release/astra project commands demo
target/release/astra project run demo check --dry-run --json
target/release/astra project run demo check
target/release/astra project run demo check --json
target/release/astra project run demo build
target/release/astra project run demo test
target/release/astra project run demo check   # after a deliberate source edit
```

Verify that check uses direct Cargo argv, JSON stdout remains valid, build and
test refuse real execution, a stale plan is refused before launch, and a
mutation during execution is reported as unverified. Remove the temporary
configuration and project afterward.
