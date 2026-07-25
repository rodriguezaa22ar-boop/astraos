# Contributing

Before opening a pull request, run:

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo build --release
```

Use focused branches and conventional commit messages.
