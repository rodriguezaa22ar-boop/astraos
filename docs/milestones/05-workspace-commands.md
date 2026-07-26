# Milestone 5: Workspace Commands

## User capability

Users can manage the persistent workspace registry from the CLI.

## Required commands

```text
astra workspace list
astra workspace add <name> <path> [--force]
astra workspace remove <name>
astra workspace open <name>
```

## Acceptance criteria

### `workspace list`

- Loads the current TOML configuration.
- Prints workspace names and paths in deterministic order.
- Marks missing directories without failing.
- Returns success for an empty registry.

### `workspace add`

- Validates workspace names.
- Expands `~` at the beginning of paths.
- Converts relative paths to absolute paths.
- Refuses duplicate names unless `--force` is present.
- Saves changes through `astra_config::save`.
- Does not require the target directory to exist.

### `workspace remove`

- Removes only the registry entry.
- Never deletes the workspace directory.
- Returns a useful error for unknown names.
- Saves the updated configuration.

### `workspace open`

- Resolves the path from configuration.
- Creates the directory only after explicit confirmation or an explicit flag.
- Opens the configured editor.
- Retains AI and cybersecurity workspace-specific launch behavior.

## Recommended code changes

- Add nested Clap subcommands for workspace operations.
- Move workspace mutation logic into `astra-workspaces`.
- Add typed workspace errors.
- Add unit tests for add, replace, remove, and lookup.
- Add CLI integration tests using temporary config directories.

## Validation

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```
