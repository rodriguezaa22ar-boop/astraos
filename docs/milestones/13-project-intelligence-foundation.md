# Milestone 13: Project Intelligence Foundation

## Mission

Build a deterministic, explainable understanding layer over live project
context, discovered actions, execution capability, and projected knowledge.
The first user-facing capability is:

```text
astra project understand <PROJECT> [--json]
```

It answers what is known, how it is related, what is verified, and what is
unavailable. It does not plan, recommend, or execute actions.

## Architecture

```text
context + actions + execution capability + projected knowledge
                           ↓
              ProjectIntelligenceInput (CLI adapter)
                           ↓
                 astra-intelligence analyzer
                           ↓
   runtime relationship graph → model → rules → report projection
```

`astra-intelligence` is pure and receives explicit typed input. It performs no
filesystem scanning, Git invocation, process execution, configuration loading,
storage access, or mutation. The CLI owns integration and is the only layer
that resolves a registered project, gathers live context, reads knowledge, and
captures current state for verification-validity projection.

Persisted `astra-knowledge` relationships remain distinct from runtime
intelligence graph edges. A knowledge relationship is durable claim data;
an intelligence edge is a current connection in the synthesized project model.

## Model and provenance

Entities, relationships, insights, risks, and limitations have deterministic,
semantic IDs independent of absolute checkout paths and input order. Every
relationship and derived insight has structured evidence. Information is
classified as `observed`, `derived`, or `operator_decided`; all Milestone 13
insights are `derived` and non-authoritative.

The versioned report contains identity, architecture, capabilities,
verification, knowledge counts, repository dimensions, entities, edges,
insights, risks, and limitations. It has no timestamp, health score, source
content, command output, absolute path, raw knowledge value, or secret data.

## Initial rules

- `PI-001`: multi-package workspace
- `PI-002`: controlled, evidence-producing verification
- `PI-003`: discoverable actions restricted from direct execution
- `PI-004`: latest verification is stale, with a separate risk
- `PI-005`: no operator-decision input becomes a limitation, not a fact
- `PI-006`: multiple workspace packages when explicit workspace structure exists

## CLI and read-only behavior

Text and JSON output are rendered from the same `ProjectIntelligence` value.
The command never invokes a project action, writes knowledge, changes the
registry, edits a project, or refreshes verification. A bounded read-only Git
state capture may be used by the CLI only to project a state-bound verification
as current, stale, or unknown; it is not command execution of the project.

## Security and determinism

Only approved summaries and references are admitted. The report excludes
absolute roots, source contents, command output, diffs, environment data,
credentials, and arbitrary knowledge values. Equivalent inputs must produce an
equal model and byte-identical JSON, regardless of input order or checkout
location.

Collections use canonical semantic order: entities by kind/name/ID,
relationships by kind/source/target/ID, insights by rule ID/ID, and risks and
limitations by statement/ID.

## Future boundary: Milestone 13.1

Operator Annotations, Corrections, and Overrides may add explicit editing and
historical override workflows. Milestone 13 intentionally contains no mutation
commands, manual relationships, insight acceptance, or overrides.

## Non-goals

- Planning, recommendations, orchestration, or autonomous execution
- LLMs, embeddings, semantic search, or a graph database
- Cloud synchronization and automatic knowledge ingestion
- Source-level semantic analysis or broad health/quality scores

## Validation

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --release
./scripts/astraos-milestones.sh validate
git diff --check
```
