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
