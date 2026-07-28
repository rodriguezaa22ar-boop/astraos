# Decision Log

Use this file for small engineering decisions that do not require a full ADR.

- Rust workspace adopted for modular growth.
- TOML selected for user configuration.
- Clap selected for CLI parsing.
- Tracing selected for structured diagnostics.
- Thiserror selected for typed library errors.
- Ratatui selected for the interactive terminal dashboard.
- Project actions remain structured argv values and read-only until a later
  milestone defines execution policy.
- The first action policy is an explicit Cargo workspace allowlist and is
  evaluated through deterministic dry-run plans before execution is designed.
- Controlled execution is isolated in `astra-execution`: only a state-bound
  Cargo workspace check may run, with direct argv invocation and post-run Git
  state verification. Build and test remain dry-run only until a later policy
  decision.
- Knowledge is an independent, evidence-backed foundation in
  `astra-knowledge`, not an execution log or AI memory store. Versioned JSON
  files preserve historical claims and state-aware validity without coupling
  the model to context, actions, or execution.
- Project intelligence is a pure, read-only model layer in
  `astra-intelligence`. It receives explicit normalized inputs at the CLI
  boundary and produces runtime graph edges and deterministic insights without
  scanning, executing, reading storage, planning, or mutating knowledge.
