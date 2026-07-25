# AstraOS

AstraOS is a native Rust command center for a macOS development workstation.

## Milestone 1

- `astra dashboard`
- `astra doctor`
- `astra workspace <name>`
- `astra project <node|python|static> <name>`

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

## Planned milestones

1. Native command foundation
2. TOML configuration
3. Interactive terminal dashboard
4. Plugin architecture
5. Update and backup engine
6. AI workspace orchestration
7. Security-lab orchestration
8. Cross-platform support
