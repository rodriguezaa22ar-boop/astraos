# Milestone 8: Project Context Engine

## Objective

Add a provider-neutral, read-only project analyzer that produces deterministic
structured knowledge about a selected project. The resulting context can be
consumed by future AI adapters and other AstraOS features, but the analyzer
does not call an LLM or depend on any AI provider.

## User capability

Users can inspect any local project with:

```text
astra context [PATH]
astra context [PATH] --json
astra context tree [PATH]
```

`PATH` defaults to the current directory. All views render the same scan
report. JSON is the complete stable serialized contract; text and tree are
selective human-readable views. Scanning does not load or modify the AstraOS
configuration or workspace registry.

## Architecture

The `astra-context` crate owns the analyzer, public context model, private
scanning pipeline, insights, and renderers. The CLI only parses arguments,
invokes the crate, and prints the selected rendering.

The pipeline is fixed:

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

No stage bypasses an earlier stage. The selected path is the scan boundary. A
containing Git repository may be reported, but the analyzer does not widen the
filesystem scan outside the selected root.

## FactGraph

`FactGraph` is a private, immutable, in-memory source of normalized repository
facts. It is built once per analysis and discarded after the public report is
created.

The graph:

- eliminates repeated traversal and manifest parsing;
- deduplicates observations and evidence deterministically;
- provides typed relationships between files, manifests, packages,
  workspaces, dependencies, commands, tools, documentation, and repository
  facts;
- exposes no public IDs or query API;
- is not serialized, persisted, cached, configured, or shared globally;
- is not a graph database or plugin interface.

`FactGraphBuilder::finish` normalizes and sorts facts, merges duplicate
evidence, resolves relationships, assigns deterministic internal IDs, and
freezes the graph. Projection scanners receive only an immutable
`ScannerInput`.

Every fact provenance record has a private semantic scope: primary, fixture,
or example. Inventory still retains bounded files from every scope. Known
nested fixture boundaries such as `tests/fixtures`, `test/fixtures`,
`testdata`, and fixtures below test-support directories are non-primary.
Examples and samples are likewise non-primary. Projection scanners query only
primary facts, preventing embedded repositories from becoming top-level
packages, languages, tools, commands, entry points, or documentation.

Scope is derived from project-relative paths. The selected root itself is
always primary, including when its directory is named `fixtures`. A directory
named `tests` alone is not excluded: ordinary project tests continue to
contribute language, size, and testing evidence.

## Scanner phases

Fact-producing scanners may perform bounded discovery:

- inventory scanner;
- manifest scanner;
- Git scanner.

Projection scanners consume only the immutable `FactGraph`:

- language;
- workspace;
- dependency;
- documentation;
- CI;
- configuration;
- validation;
- build;
- testing;
- entry points;
- license.

Projection scanners do not read the filesystem, execute commands, parse
manifests, mutate facts, or consume another projection scanner's output.

Every scanner has stable metadata consisting of an ID, version, and
description. Scanner failures are represented as structured results and
diagnostics so optional information can be unavailable without discarding the
rest of the report.

## Public model

`ProjectAnalyzer` coordinates the phases and returns `ScanReport`.

`ScanReport` contains:

- `schema_version`, set from `PROJECT_CONTEXT_SCHEMA_VERSION`;
- `ProjectContext`;
- scanner results;
- diagnostics;
- derived `Detected<Insight>` values.

Runtime duration is available in memory but excluded from serialization so it
cannot make JSON output nondeterministic; deserialization restores it to zero.
The schema version is a stable public contract: serialized fields must not be
silently renamed or removed without a schema-version change.

Semantic discoveries use `Detected<T>`:

```text
Detected<T>
├── value
├── confidence
└── evidence
```

Raw observations remain private facts. Public evidence explains why a
semantic value was detected without exposing internal fact or relationship
IDs. Evidence vectors are deterministic, bounded provenance samples rather
than exhaustive provenance listings.

## Context coverage

The context model describes, when evidence is available:

- selected root and containing Git repository;
- branch, HEAD, recent commits, changes, and repository cleanliness;
- languages and project size;
- workspace and package structure;
- package managers, build systems, and testing frameworks;
- direct declared dependencies;
- documentation, ADRs, milestones, and important configuration;
- CI configuration;
- entry points;
- license information;
- common development and recommended validation command argument vectors.

Commands are reported as argument vectors and never executed. Source-code
bodies are not collected; bounded documentation headings, recognized manifest
metadata, and local Git metadata are collected. A built-in path policy excludes
common sensitive files and directories plus URL-like dependency requirements,
but the analyzer is not a content-level secret scanner.

Validation projection prefers authoritative Cargo workspace commands. An
equivalent command inferred from a nested package is folded into the nearest
workspace command, with its evidence retained. Commands with distinct
arguments remain package-specific.

## Insights

`InsightsEngine` is a pure derived stage over `ProjectContext` and the
immutable `FactGraph`. It performs no filesystem access, command execution,
Git access, AI calls, or mutation.

Milestone 8 insights are factual observations, such as:

- no README evidence was detected;
- no testing evidence was detected;
- conflicting lockfiles were detected;
- a workspace member references a missing path;
- a scan was truncated by a safety limit.

Insights do not recommend project changes or generate prompts.

## Determinism and safety

- Traversal is single-threaded, sorted, bounded, ignore-aware, and does not
  follow directory symlinks.
- Total entries, retained files, traversal depth, per-file reads, Git output,
  and Git execution time are independently bounded.
- Project-local ignore rules are honored without reading machine-specific
  global Git ignore configuration.
- Git uses bounded, noninteractive, machine-readable invocations without a
  shell or network operation.
- Paths, facts, evidence, diagnostics, insights, and projected values use
  stable deterministic ordering.
- Truncation and partial scans are explicit.
- Missing tools, malformed manifests, permission failures, and unsupported
  project types are recoverable where a useful partial report remains.

Byte-for-byte determinism applies to repeated scans with the same options,
canonical project location, filesystem contents, and Git state.
`identity.root` and `repository_root` are absolute local paths, so relocating a
project changes serialized output. Other context and evidence paths are
relative to the selected project root.

## Configuration and cache

Milestone 8 adds no persistent AstraOS configuration. `ScanOptions` contains
only validated in-memory safety limits.

There is no persistent cache. Correct cache invalidation and versioning are
deferred until scan cost and incremental-analysis requirements are known.

## Dependencies

- New: `ignore = "0.4"` for serial, sorted, project-ignore-aware traversal and
  directory pruning.
- Reused workspace dependencies: `serde`, `serde_json`, `thiserror`, `toml`,
  and test-only `tempfile`.
- No async runtime, Git library, AI SDK, or model dependency is added.

## Test coverage

Tests use isolated fixtures and temporary directories. They cover:

- deterministic FactGraph construction independent of insertion order;
- duplicate fact and evidence merging;
- relationship resolution and dangling references;
- manifest parsing and reuse without downstream reparsing;
- each projection scanner with synthetic facts;
- scanner error and partial-result behavior;
- deterministic insight derivation;
- exclusion of internal graph state and runtime duration from JSON;
- repeatable text, JSON, and tree output;
- Rust workspace, Node monorepo, and polyglot fixture projects;
- retention of fixture manifests in inventory without promoting embedded
  fixture repositories into project-level context;
- ordinary source-test evidence and selected roots named `fixtures`;
- consolidation of equivalent package Cargo commands under workspace
  validation commands;
- ignored paths, malformed inputs, scan limits, symlinks, and permissions;
- clean, dirty, detached, unavailable, timed-out, and malformed Git outcomes
  through a fake process boundary;
- CLI behavior without user configuration or installed external tools.

Tests do not require the internet, GitHub, an LLM, an editor, WezTerm, or the
developer's machine state.

## Known limitations

- Language detection is extension- and convention-based.
- Maven, Gradle, Go workspace, pnpm workspace, and Swift parsing is
  intentionally conservative rather than full grammar evaluation.
- Detection may report unknown or partial results for unsupported manifest
  formats.
- Dependency summaries cover direct declarations rather than full resolved
  transitive graphs.
- Inferred commands are neither verified nor executed.
- Insights are limited to factual structural observations.
- Included documentation headings, manifest metadata, branch names, and commit
  subjects are not content-level secret-scanned.
- Text and tree views omit fields available in JSON.
- The semantic tree is a view of known workspace and package relationships,
  not an unrestricted filesystem tree.
- A timed-out Git child is killed and the scan returns promptly, but a
  separately spawned descendant that retained an output pipe may outlive the
  scan on platforms without process-group cleanup.
- Persistent caching, public scanner extensions, prompt generation, semantic
  search, remote projects, and dashboard integration are out of scope.

## Validation

```bash
cargo test -p astra-context
cargo test -p astra
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
./scripts/astraos-milestones.sh validate
```

## Manual verification

```bash
cargo build --release
ASTRA_CONFIG_DIR="$(mktemp -d)" ./target/release/astra context .
ASTRA_CONFIG_DIR="$(mktemp -d)" ./target/release/astra context . --json
ASTRA_CONFIG_DIR="$(mktemp -d)" ./target/release/astra context tree .
ASTRA_CONFIG_DIR="$(mktemp -d)" env PATH="" ./target/release/astra context .
```

The final command verifies graceful Git-unavailable behavior. None of these
commands reads or modifies the user's AstraOS configuration.
