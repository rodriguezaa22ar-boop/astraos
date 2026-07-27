#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="${ASTRAOS_ROOT:-$HOME/Developer/projects/astraos}"
ACTION="${1:-help}"
MILESTONE="${2:-}"

log()  { printf "\n\033[1;36m==>\033[0m %s\n" "$*"; }
ok()   { printf "\033[1;32m✓\033[0m %s\n" "$*"; }
warn() { printf "\033[1;33m!\033[0m %s\n" "$*" >&2; }
die()  { printf "\033[1;31mError:\033[0m %s\n" "$*" >&2; exit 1; }

usage() {
  cat <<'EOF'
AstraOS Milestone Runner

Usage:
  ./astraos-milestones.sh start <5|6|7|8>
  ./astraos-milestones.sh validate
  ./astraos-milestones.sh install
  ./astraos-milestones.sh finish
  ./astraos-milestones.sh status
  ./astraos-milestones.sh list

Milestones:
  5  Workspace commands
  6  Live interactive dashboard
  7  WezTerm workspace orchestration
  8  Project Context Engine

Typical workflow:
  ./astraos-milestones.sh start 5
  # Implement the generated docs/milestones/... specification
  ./astraos-milestones.sh validate
  ./astraos-milestones.sh install
  ./astraos-milestones.sh finish
EOF
}

require_repo() {
  [[ -d "$ROOT/.git" ]] || die "AstraOS Git repository not found: $ROOT"
  cd "$ROOT"
}

require_clean_tree() {
  if [[ -n "$(git status --porcelain)" ]]; then
    git status --short
    die "Working tree must be clean before starting or finishing a milestone."
  fi
}

current_branch() {
  git branch --show-current
}

ensure_main_current() {
  git checkout main
  git pull --ff-only origin main
}

milestone_name() {
  case "$1" in
    5) echo "workspace-commands" ;;
    6) echo "live-dashboard" ;;
    7) echo "wezterm-orchestration" ;;
    8) echo "project-context-engine" ;;
    *) die "Unknown milestone: $1" ;;
  esac
}

milestone_title() {
  case "$1" in
    5) echo "Workspace Commands" ;;
    6) echo "Live Interactive Dashboard" ;;
    7) echo "WezTerm Workspace Orchestration" ;;
    8) echo "Project Context Engine" ;;
    *) die "Unknown milestone: $1" ;;
  esac
}

write_milestone_5() {
  cat > docs/milestones/05-workspace-commands.md <<'EOF'
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
EOF
}

write_milestone_6() {
  cat > docs/milestones/06-live-dashboard.md <<'EOF'
# Milestone 6: Live Dashboard

## User capability

Users can view a live command center with local system, developer-service, and
workspace status.

## Required dashboard information

- Operating system and hostname
- CPU utilization
- Memory used and total
- Battery percentage and charging state
- Selected-workspace filesystem usage
- System uptime
- Docker daemon status
- Ollama local-service status
- Configured workspace names and paths

## Required controls

```text
Up/Down or J/K  Select workspace
R               Refresh immediately
Q or Esc        Quit
```

## Acceptance criteria

- Refreshes without flicker at a controlled interval.
- Restores the terminal after normal exit and errors.
- Does not panic when optional metrics or services are unavailable.
- Uses `astra-system` for metrics collection.
- Uses `astra-workspaces` for registry data.
- Displays unavailable metrics clearly.
- Does not run probes from rendering functions.
- Uses separate render/input and metrics-refresh timing.
- Uses bounded Docker and Ollama probes.
- Includes unit tests for state transitions, formatting, and probe mapping.

## Architecture

- `astra-system`: metric collection and typed snapshots.
- `astra-dashboard`: application state, input handling, rendering.
- `astra-cli`: command routing only.
- Avoid running expensive probes during every render call.

## Validation

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
astra dashboard --interactive
```
EOF
}

write_milestone_7() {
  cat > docs/milestones/07-wezterm-orchestration.md <<'EOF'
# Milestone 7: WezTerm Workspace Orchestration

## User capability

A single AstraOS command restores a complete development cockpit.

## Required commands

```text
astra workspace launch <name>
astra workspace layout <name>
```

## Configuration schema

```toml
[terminal]
command = "wezterm"

[workspace_layouts.astraos]
editor = true
ollama = false
panes = [
  "cargo watch -x check",
  "git status",
]
```

## Acceptance criteria

- Adds a typed terminal configuration section.
- Detects WezTerm before attempting orchestration.
- Opens a dedicated WezTerm workspace.
- Starts panes in the configured workspace directory.
- Opens the configured editor when enabled.
- Starts only explicitly configured services.
- Handles missing WezTerm with a useful error.
- Avoids shell injection by passing command arguments safely.
- Supports a dry-run mode that prints the launch plan.

## Suggested first layout

```text
┌───────────────────────────┬─────────────────────┐
│ Main shell                │ cargo watch         │
├───────────────────────────┼─────────────────────┤
│ git status / logs         │ optional service    │
└───────────────────────────┴─────────────────────┘
```

## Validation

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
astra workspace launch astraos --dry-run
```
EOF
}

write_milestone_8() {
  cat > docs/milestones/08-project-context-engine.md <<'EOF'
# Milestone 8: Project Context Engine

## User capability

AstraOS can inspect any local project and produce deterministic, structured
knowledge without calling an AI provider.

## Required commands

```text
astra context [PATH]
astra context [PATH] --json
astra context tree [PATH]
```

## Architecture

```text
Selected Project Root
        ↓
Inventory Phase
        ↓
ManifestCatalog
        ↓
FactGraphBuilder
        ↓
Immutable FactGraph
        ↓
Projection Scanners
        ↓
ProjectContext
        ↓
InsightsEngine
        ↓
ScanReport
        ↓
Text / JSON / Tree Renderers
```

The `FactGraph` is private, immutable after construction, and never serialized
or exposed as a public API. Projection scanners consume only that graph and do
not read files, execute Git, parse manifests, or mutate facts.

## Acceptance criteria

- Adds the provider-neutral `astra-context` crate and `ProjectAnalyzer`.
- Returns a schema-versioned `ScanReport` containing `ProjectContext`, scanner
  results, diagnostics, and factual insights.
- Carries confidence and evidence with semantic discoveries.
- Uses independent fact-producing and projection scanners.
- Is read-only, bounded, deterministic, ignore-aware, and recoverable.
- Retains bounded fixture files as facts while excluding fixture and example
  facts from production project summaries.
- Prefers authoritative workspace validation commands over equivalent
  repetitive package commands while preserving distinct package commands.
- Never calls an LLM, executes discovered validation commands, or modifies a
  project.
- Adds no persistent AstraOS configuration, cache, graph database, or plugin
  interface.
- Excludes internal graph IDs and runtime duration from serialization.
- Includes isolated tests that require no network, GitHub, LLM, editor,
  WezTerm, user configuration, or developer-machine state.

## Validation

```bash
cargo test -p astra-context
cargo test -p astra
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
./scripts/astraos-milestones.sh validate
./target/release/astra context .
./target/release/astra context . --json
./target/release/astra context tree .
```
EOF
}

start_milestone() {
  local number="$1"
  local slug title branch spec

  slug="$(milestone_name "$number")"
  title="$(milestone_title "$number")"
  branch="feat/$slug"

  require_clean_tree
  ensure_main_current

  if git show-ref --verify --quiet "refs/heads/$branch"; then
    die "Local branch already exists: $branch"
  fi

  git checkout -b "$branch"
  mkdir -p docs/milestones

  case "$number" in
    5) write_milestone_5; spec="docs/milestones/05-workspace-commands.md" ;;
    6) write_milestone_6; spec="docs/milestones/06-live-dashboard.md" ;;
    7) write_milestone_7; spec="docs/milestones/07-wezterm-orchestration.md" ;;
    8) write_milestone_8; spec="docs/milestones/08-project-context-engine.md" ;;
  esac

  git add "$spec"
  git commit -m "docs: define milestone $number $slug"

  ok "Started Milestone $number: $title"
  echo
  echo "Branch: $branch"
  echo "Specification: $spec"
  echo
  echo "Open the repository:"
  echo "  code --new-window \"$ROOT\""
  echo
  echo "Then implement the acceptance criteria in the specification."
}

validate() {
  log "Formatting"
  cargo fmt --all --check

  log "Tests"
  cargo test --workspace

  log "Clippy"
  cargo clippy --workspace --all-targets -- -D warnings

  log "Release build"
  cargo build --release

  ok "Full validation pipeline passed"
}

install_binary() {
  validate
  mkdir -p "$HOME/.local/bin"
  cp target/release/astra "$HOME/.local/bin/astra"
  chmod +x "$HOME/.local/bin/astra"
  hash -r 2>/dev/null || true
  rehash 2>/dev/null || true

  ok "Installed release binary"
  "$HOME/.local/bin/astra" --version
}

finish_milestone() {
  require_clean_tree

  local branch
  branch="$(current_branch)"
  [[ "$branch" != "main" ]] || die "Cannot finish a milestone from main."

  validate

  git push -u origin "$branch"

  local title body
  case "$branch" in
    feat/workspace-commands)
      title="Add workspace management commands"
      body="Implements Milestone 5: workspace list, add, remove, and open commands."
      ;;
    feat/live-dashboard)
      title="Add live interactive dashboard"
      body="Implements Milestone 6: live system metrics and keyboard-driven workspace navigation."
      ;;
    feat/wezterm-orchestration)
      title="Add WezTerm workspace orchestration"
      body="Implements Milestone 7: reproducible WezTerm development layouts and launch plans."
      ;;
    feat/project-context-engine)
      title="Add Project Context Engine"
      body="Implements Milestone 8: deterministic, provider-neutral project context generation."
      ;;
    *)
      title="Complete ${branch#feat/}"
      body="Implements the feature work on branch $branch."
      ;;
  esac

  gh pr create \
    --base main \
    --head "$branch" \
    --title "$title" \
    --body "## Summary

$body

## Validation

- cargo fmt --all --check
- cargo test --workspace
- cargo clippy --workspace --all-targets -- -D warnings
- cargo build --release"

  ok "Pull request created"
}

status() {
  git status
  echo
  git log --oneline -5
  echo
  echo "Installed binary:"
  command -v astra || true
  astra --version 2>/dev/null || true
}

list_milestones() {
  cat <<'EOF'
5  Workspace Commands
   Registry list/add/remove/open operations.

6  Live Interactive Dashboard
   CPU, memory, battery, storage, services, and keyboard navigation.

7  WezTerm Workspace Orchestration
   Reproducible panes, services, editor launch, and dry-run plans.

8  Project Context Engine
   Deterministic text, JSON, and semantic-tree project knowledge.
EOF
}

require_repo

case "$ACTION" in
  start)
    [[ -n "$MILESTONE" ]] || die "Provide a milestone number: 5, 6, 7, or 8."
    start_milestone "$MILESTONE"
    ;;
  validate) validate ;;
  install) install_binary ;;
  finish) finish_milestone ;;
  status) status ;;
  list) list_milestones ;;
  help|-h|--help) usage ;;
  *) usage; exit 1 ;;
esac
