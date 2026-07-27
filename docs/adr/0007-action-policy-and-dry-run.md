# ADR 0007: Evaluate Actions Through Strict Dry-Run Policy

## Status

Accepted for Milestone 10; execution details superseded by ADR 0008

## Context

Milestone 9 discovers a small set of structured project actions. Discovering a
command is not sufficient authorization to execute it. AstraOS needs a
decision boundary that can reject unexpected commands and validate their
working directories before a later milestone introduces process execution.

## Decision

Extend `astra-actions` with a strict, read-only policy and deterministic
execution-plan models. The CLI resolves a registered project, scans context,
selects an already discovered action, evaluates it through `ActionPolicy`, and
renders a `DryRunReport`. No executor, process abstraction, shell, or command
spawning is introduced.

The initial allowlist accepts only:

- executable `cargo`;
- action/subcommand pairs `build`, `check`, and `test`;
- exactly one additional argument: `--workspace`;
- a working directory that exists and canonicalizes to the project root or a
  descendant.

The action ID and Cargo subcommand must agree. Arguments remain a structured
`Vec<String>`; no shell string is constructed for policy evaluation. Path
containment uses canonical path components, so symlink escapes are rejected.

`astra project run <NAME> <ACTION> --dry-run` is the only supported run
contract. Omitting `--dry-run` fails clearly. Successful and rejected policy
evaluations report `process_started: false`; real execution is deferred.
Read-only project commands use `load_if_present()` and do not create a
configuration file.

## Alternatives considered

- Execute discovered actions immediately.
- Accept any executable because the action came from the context engine.
- Use a blacklist of dangerous arguments.
- Store policy rules in configuration or environment variables.
- Add an executor or process-runner abstraction before execution exists.
- Validate paths with string-prefix comparisons.

## Consequences

- The policy is intentionally conservative and currently supports only Cargo
  workspace validation.
- Future legitimate command shapes require an explicit policy change and tests.
- Dry-run JSON is stable, versioned, and contains no timestamps or generated
  identifiers.
- Rejected policy plans can be rendered for diagnosis while the CLI returns a
  nonzero status.
- Process execution, output capture, retries, cancellation, and policy
  configuration remain future work.

## Dependency direction

`astra-context` discovers structured validation commands. `astra-actions`
resolves actions, evaluates policy, and builds dry-run plans. `astra-cli`
resolves registry projects and renders plans. No lower-level crate depends on
the CLI, and no process execution code is added to `astra-actions`.
