# ADR 0006: Discover Project Actions Without Executing Them

## Status

Accepted

## Context

Milestone 8 already produces deterministic validation commands as structured
executable, argument-vector, working-directory, purpose, confidence, and
evidence values. A project-facing command needs to expose useful actions
without duplicating context scanning or turning discovered text into arbitrary
shell execution.

## Decision

Create a dedicated `astra-actions` crate downstream of `astra-context`.
`astra-actions` contains a minimal typed action model and a pure resolver.
The initial resolver recognizes only:

- `cargo build` → `build`;
- `cargo check` → `check`;
- `cargo test` → `test`.

The first argument selects the action. Remaining arguments and the working
directory are preserved exactly. `Detected<T>` confidence is reused from
`astra-context`; the source is recorded as `context_engine`.

Duplicate candidates are reduced by highest confidence. Equal-confidence
candidates use a lexical `(working directory, arguments, executable)` tie
breaker. Final actions use the stable semantic order build, check, test.

`astra project commands <NAME>` resolves the registered project through
`astra-workspaces`, analyzes it with `ProjectAnalyzer`, resolves actions, and
renders either a human table or a versioned JSON report. It performs no action
execution and does not mutate configuration or the project.

The existing project scaffolding behavior is preserved as the explicit
`astra project create <KIND> <NAME>` subcommand. Ambiguous positional
`astra project <KIND> <NAME>` parsing is removed.

## Alternatives considered

- Store shell command strings instead of argv vectors
- Execute discovered commands from the discovery command
- Add arbitrary command or shell interpretation
- Put action mapping inside the context scanners
- Make `astra-cli` own the action model
- Recognize every ecosystem before defining execution policy

## Consequences

- Context scanning and action discovery have separate responsibilities.
- Action output is safe to inspect and deterministic to serialize.
- Executable paths and argument boundaries remain explicit for future policy
  and execution layers.
- The current vocabulary intentionally omits non-Cargo and package-specific
  actions, even when context detects them.
- Execution, policy enforcement, history, parallelism, and remote operation
  remain deferred and require a later architectural decision.

## Dependency direction

`astra-actions` depends on `astra-context` for detected command and confidence
types. `astra-cli` depends on both `astra-actions` and `astra-context` for
orchestration and rendering. Context and lower-level crates do not depend on
the action crate or CLI.
