# AstraOS

AstraOS is a native Rust command center for a macOS development workstation.

## CLI commands

- `astra dashboard`
- `astra doctor`
- `astra workspace list|add|remove|open`
- `astra project list`
- `astra project inspect <name>`
- `astra project commands <name> [--json]`
- `astra project run <name> <action> --dry-run [--json]`
- `astra project understand <name> [--json]`
- `astra project create <node|python|static> <name>`
- `astra context [path] [--json]`
- `astra knowledge list`
- `astra knowledge show <project> [--json]`
- `astra knowledge facts <project> [--json]`
- `astra knowledge verifications <project> [--json]`
- `astra knowledge decisions <project> [--json]`

## Install

```bash
chmod +x install.sh
./install.sh
```

Then open a new Terminal and run:

```bash
astra dashboard
astra doctor
```

## Development

```bash
cargo run -- dashboard
cargo run -- doctor
cargo test
cargo build --release
```

## Current roadmap

8. Project Context Engine
9. Read-only Project Action Engine
10. Action Policy and Dry-Run Planning
11. State-Bound Controlled Check Execution
12. Knowledge Foundation
13. Project Intelligence Foundation

The original roadmap also included the following pre-context milestones:

1. Native command foundation
2. TOML configuration
3. Interactive terminal dashboard
4. Plugin architecture
5. Update and backup engine
6. AI workspace orchestration
7. Security-lab orchestration
8. Cross-platform support

Milestone 10 evaluates and previews only the currently discovered Cargo
validation actions. Milestone 11 adds tightly controlled execution of the
discovered `check` action: it requires a Git-backed, state-bound plan and
refuses stale plans. Build and test remain dry-run only.
