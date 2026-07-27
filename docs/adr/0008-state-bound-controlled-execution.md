# ADR 0008: State-Bound Controlled Check Execution

## Status

Accepted

## Context

Milestone 10 established a strict Cargo action policy and deterministic
dry-run plans, but it intentionally did not start processes. A later execution
boundary must not turn a stale or edited plan into an arbitrary command. It
also must not widen the action vocabulary merely because a command was found
by project analysis.

## Decision

Add `astra-execution` as the only crate that can turn an authorized plan into
a process invocation. The crate owns the execution schema, SHA-256
fingerprints, bounded Git-state capture, direct process launching, and typed
execution results. `astra-actions` remains a pure action vocabulary and policy
crate; it does not gain process-runner or fingerprint abstractions.

The execution flow is:

```text
registered project
  → context validation commands
  → astra-actions resolver and policy
  → astra-execution authorized plan
  → source-state revalidation
  → direct argv launch
  → post-run source-state capture and verdict
```

The first executable action is only `cargo check --workspace`. `build` and
`test` continue to support dry-run planning but are rejected for real
execution. The executable, arguments, and working directory remain separate
process fields. No shell, interpolation, environment mutation, package
installation, parallel execution, or remote execution is used.

The plan binds the canonical selected project root, repository-wide `HEAD`,
and three selected-subtree state components: staged diff, unstaged tracked
diff, and relevant non-ignored untracked files. A sibling project inside the
same repository therefore does not invalidate the selected project, while a
repository commit does. Untracked files are sorted and content-fingerprinted
under fixed count and byte limits. Git command output is bounded and Git is
run with prompting, lazy fetching, global/system configuration, and pagers
disabled. Each Git state command also has a five-second deadline; timeout is a
typed capture failure rather than a partial state binding.

Before starting Cargo, the engine rechecks the plan schema and policy version,
canonical root, exact action fingerprint, source-state fingerprint, and plan
fingerprint. A mismatch returns a typed error and the launcher is not called.
After the process exits, state is captured again. The result distinguishes a
verified successful check, command failure, source-state change, combined
failure/change, and interruption.

## Alternatives considered

- Keep execution in `astra-actions`: rejected because action discovery/policy
  should remain pure and reusable by read-only callers.
- Execute arbitrary discovered commands: rejected; discovery is not
  authorization.
- Re-scan context immediately before launch: rejected; execution must validate
  the exact authorized plan and state, not silently rebuild it.
- Use a shell string: rejected because it loses argv boundaries and enables
  shell interpretation.
- Bind all repository changes for nested projects: rejected because unrelated
  sibling work should not stale a selected project plan.

## Consequences

- Real execution is deliberately narrow and currently requires a Git
  repository.
- A source mutation between planning and launch refuses execution; a mutation
  during the command produces an unverified result rather than hiding it.
- A partial process or post-run capture failure is reported without rollback.
- Fingerprints and state bindings are versioned serialized contracts, but the
  internal Git representation and runner traits remain private to
  `astra-execution`.
- Execution is synchronous and single-process. Time, retries, history,
  cancellation, parallelism, and remote actions remain future work.

## Dependency direction

`astra-context` produces structured validation commands. `astra-actions`
resolves and policy-checks them without side effects. `astra-execution`
consumes the resulting action and owns execution-only types and process/Git
implementation. `astra-cli` is the thin adapter that resolves registry
projects, selects the check action, and renders results.
