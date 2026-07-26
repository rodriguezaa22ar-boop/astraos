# Milestone 7: WezTerm Orchestration

## Objective

Launch a registered AstraOS workspace as a repeatable, named WezTerm
development layout without changing the workspace registry or the existing
`astra workspace open` workflow.

## Implemented scope

```text
astra workspace layout <layout-name>
astra workspace launch <workspace-name> [--layout <layout-name>] [--dry-run]
```

`workspace-name` selects a registered filesystem path. `layout-name` selects a
declarative entry in `workspace_layouts`. Without `--layout`, launch requires a
layout with the same name as the workspace and explains how to select another
layout if that default is absent.

The generated WezTerm mux workspace is `astra:<workspace-name>`. A live launch
refuses to reuse an existing mux workspace with that name.

## Configuration

Existing configuration files remain valid. The default terminal command is
`wezterm`, and the default layout map is empty.

```toml
[terminal]
command = "wezterm"

[workspace_layouts.rust-development]
editor = true
ollama = false

[[workspace_layouts.rust-development.tabs]]
name = "development"
command = []

[[workspace_layouts.rust-development.tabs.panes]]
target = 0
direction = "right"
percent = 45
command = ["cargo", "watch", "-x", "check"]
```

Every tab has an initial pane at index `0`. Additional panes receive indices
`1`, `2`, and so on in declaration order. A target must refer to an
already-created pane in the same tab; forward and cross-tab references are
invalid.

Supported split directions are `left`, `right`, `top`, and `bottom`. Split
percentages must be in the safe inclusive range 10–90. Empty or omitted
commands start the user's default shell for both initial and additional panes.

Commands are literal argument vectors. AstraOS never invokes a shell, joins
arguments into a shell string, expands environment variables or globs, or
interprets pipes, redirects, substitutions, or `&&`.

## Architecture boundaries

- `astra-config` owns the serialized terminal, layout, tab, pane, and split
  direction types.
- `astra-terminal` owns validation, normalized launch plans, dry-run
  presentation, WezTerm detection, process execution, and pane discovery.
- `astra-cli` owns only command parsing, configuration loading, and result
  presentation.
- `astra-workspaces`, `astra-system`, and `astra-dashboard` retain their
  Milestone 5 and 6 runtime behavior.

Process invocations retain the executable, argument vector, optional current
directory, exit status, stdout, and stderr. Live commands have a five-second
execution timeout, and displayed stderr is capped at 4 KiB.

## WezTerm behavior

The locally verified release is:

```text
wezterm 20240203-110809-5046fc22
```

This is the minimum version tested for this milestone. Its installed help
confirms the options used by AstraOS:

- `wezterm cli list --format json`
- `wezterm cli spawn --new-window --workspace --cwd`
- `wezterm cli split-pane --pane-id --left|--right|--top|--bottom --percent --cwd`
- `wezterm cli set-tab-title --pane-id`
- `wezterm start --workspace --cwd`

AstraOS first reads the structured pane list and checks the generated mux
workspace name. It prefers the pane ID printed directly by `cli spawn`. When
the CLI cannot reach a GUI or mux, it falls back to `wezterm start`, then polls
only for the intended workspace up to 20 times at 100 ms intervals. Discovery
must produce exactly one new initial pane. After that, every spawn and split
uses its directly returned, validated pane ID.

`editor = true` starts the configured `[editor].command` with the registered
workspace path as one argument. It does not hardcode an editor.
`ollama = true` explicitly runs the existing opt-in
`brew services start ollama` behavior. Both default to false, and Ollama is
never started implicitly.

## Dry-run

Dry-run fully resolves and validates the workspace and layout, then prints
every intended invocation in deterministic order. Executables and arguments
are JSON-quoted so boundaries are visible.

It performs no process execution, WezTerm query, directory creation, editor
startup, Ollama startup, or configuration write. WezTerm does not need to be
installed.

## Errors and partial launches

Errors distinguish missing executables, an unreachable mux, failed commands,
malformed output, startup timeout or ambiguity, existing mux workspaces,
invalid layouts, and failures after creation begins. Error stderr context is
bounded.

After an initial pane is created, AstraOS stops at the first failing operation
and reports that a partial layout may remain. It does not hide the original
error or kill panes, tabs, windows, or processes. Automatic rollback is out of
scope.

## Test coverage

- Existing TOML compatibility and empty-layout defaults
- Terminal and layout round trips
- Different workspace and layout names
- Same-name default selection and missing-default errors
- Tab-local target validation and forward-reference rejection
- Zero and 100 percent rejection
- Empty-command default shells
- Existing mux workspace detection
- Direct spawn and split pane-ID parsing
- Malformed and ambiguous pane discovery
- Arguments and executable paths containing spaces
- Deterministic dry-run with zero process invocations
- CLI layout and dry-run behavior using isolated configuration
- Preservation of all existing workspace and dashboard tests

Tests use fake process outcomes and temporary configuration; they do not query
the installed WezTerm instance or the user's configuration.

## Known limitations

- Only local WezTerm workspaces are supported.
- Automatic rollback and reuse/replacement of existing mux workspaces are not
  implemented.
- Layouts do not manage containers, models, remote machines, applications
  other than the configured editor, or services other than opt-in Ollama.
- Persistent pane IDs, mouse orchestration, and shell-expression commands are
  intentionally unsupported.

## Validation

```bash
cargo test -p astra-config
cargo test -p astra-terminal
cargo test -p astra
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
./scripts/astraos-milestones.sh validate
```
