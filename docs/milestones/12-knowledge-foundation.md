# Milestone 12: Knowledge Foundation

## Objective

Persist a small, evidence-backed model of what AstraOS knows after the
context/action/execution pipeline produces a result. This milestone is a
knowledge foundation, not AI memory: it does not call an LLM, generate
embeddings, or modify projects.

## Architecture

`astra-knowledge` is independent of the context, action, and execution crates.
Producer adapters remain at the CLI boundary. The flow is:

```text
Context / decision / verification evidence
                    ↓
            KnowledgeClaim
                    ↓
          versioned file storage
                    ↓
          read-only knowledge queries
```

Claims are deterministic by category, subject, predicate, and canonical JSON
value. Their creation timestamp is historical metadata and is not part of the
ID. The storage layer is atomic and versioned, while `KNOWLEDGE_SCHEMA_VERSION`
is included in every claim and relationship envelope.

## Knowledge model

Initial categories are `fact`, `decision`, `verification`, and `goal`.
Confidence is one of `certain`, `high`, `medium`, `low`, or `unknown`.
Validity is one of `current`, `stale`, `invalidated`, or `unknown`.
Relationships include `created_by`, `supports`, `depends_on`, `verified_by`,
`invalidated_by`, `related_to`, and `derived_from`.

Evidence points to reality using a typed source, identifier, optional locator,
and optional fingerprints. It does not copy source contents, terminal output,
raw diffs, environment variables, or credentials. The model rejects obvious
sensitive field names, but is not a general secret detector.

## Storage

The default root is `~/.astra/knowledge`. Tests and isolated environments may
set `ASTRA_KNOWLEDGE_DIR`. Project claims are stored under:

```text
projects/<project>/facts/*.json
projects/<project>/decisions/*.json
projects/<project>/verifications/*.json
projects/<project>/goals/*.json
projects/<project>/relationships.json
```

Each JSON file is a schema-versioned envelope. Writes are performed through a
temporary file followed by sync and rename. Missing directories are empty;
corrupt files and unsupported schema versions are typed errors. Claims are
never silently deleted. Invalidation writes the same historical claim with an
`invalidated` validity state.

## Verification integration

`astra project run <NAME> check` now stores a verification claim after a
successful execution result (including failed or changed-state verdicts as
structured historical observations). The claim records project/action
identity, verdict, exit status, state fingerprint, action fingerprint, plan
fingerprint, and a typed execution-result evidence reference. It never stores
child stdout/stderr or source contents. `astra knowledge verifications` can
compare state-bound claims with the current project state when the registered
project and Git state are available; otherwise the projection is unknown.

## CLI

```text
astra knowledge list
astra knowledge show <PROJECT> [--json]
astra knowledge facts <PROJECT> [--json]
astra knowledge verifications <PROJECT> [--json]
astra knowledge decisions <PROJECT> [--json]
```

Human output is concise and deterministic. JSON output is versioned and is
derived from the same typed claims. There are no edit, delete, memory,
planning, or AI commands.

## Tests

`astra-knowledge` tests cover deterministic category-aware IDs, canonical JSON
values, evidence-safe claims, stale projections, atomic writes, missing and
corrupt stores, schema errors, invalidation, and relationship endpoint
integrity. CLI tests cover help, deterministic and versioned queries, empty
stores, and persistence of a fingerprint-only verification claim after the
controlled check path.

## Known limitations

- Only the Milestone 11 controlled check currently produces knowledge.
- Context facts, ADR decisions, and roadmap goals require future explicit
  ingestion adapters.
- Storage has schema detection but no migrations beyond version 1 yet.
- Querying is category/project based; there is no semantic or full-text search.
- There is no synchronization, encryption service, automatic retention, or
  graph database.

## Validation

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
./scripts/astraos-milestones.sh validate
git diff --check
```

For isolated manual verification, set `HOME` and `ASTRA_KNOWLEDGE_DIR` to
temporary directories, run a controlled check on a registered Git project,
then inspect `astra knowledge verifications <name> --json` and mutate a source
file to observe the stale validity projection.
