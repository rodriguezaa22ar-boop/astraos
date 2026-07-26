# ADR 0004: Use Declarative WezTerm Launch Plans

## Status

Accepted

## Context

AstraOS needs to restore repeatable local development layouts while preserving
the existing workspace registry and editor-opening behavior. Layout commands
come from user configuration, so implicit shell evaluation would create
ambiguous argument handling and an unnecessary injection boundary. WezTerm's
GUI startup command also differs from its mux CLI commands: `start` does not
return an initial pane ID, while `cli spawn` and `cli split-pane` do.

## Decision

Store terminal layouts as typed configuration in `astra-config`. Commands are
vectors whose first item is the executable and whose remaining items are
literal arguments. AstraOS does not invoke a shell, expand variables, or
interpret shell operators.

Normalize and validate layouts in `astra-terminal` before process execution.
Use a deterministic mux workspace name, `astra:<workspace-name>`, and reject a
launch if that WezTerm workspace already exists.

Prefer the pane IDs printed directly by `wezterm cli spawn` and
`wezterm cli split-pane`. First attempt to create the initial pane through the
mux CLI. If no GUI or mux is reachable, start the GUI with `wezterm start` and
poll `wezterm cli list --format json` with a bounded policy to discover exactly
one initial pane in the intended mux workspace. Subsequent operations use the
returned pane IDs and do not rediscover global state.

Keep the process runner private to `astra-terminal`. Dry-run renders the
validated plan without probing executables, querying WezTerm, starting
services, opening the editor, or mutating the filesystem.

## Alternatives considered

- Store commands as shell strings
- Use raw WezTerm pane IDs in persistent configuration
- Rediscover pane IDs from the global pane list after every command
- Add an asynchronous runtime
- Roll back panes and windows after partial failure

## Consequences

- Argument boundaries, including spaces, remain unambiguous.
- Pane targets are stable, tab-local configuration indices.
- Live startup remains synchronous and bounded without an async runtime.
- The initial GUI fallback needs short, bounded polling because `wezterm start`
  does not return a pane ID.
- Failed operations stop the plan, but already created WezTerm resources may
  remain; automatic rollback is intentionally out of scope.
