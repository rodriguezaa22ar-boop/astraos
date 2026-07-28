## Status

Accepted

## Context

Milestones 8–11 established deterministic project context, typed actions,
policy-bound plans, and state-bound check verification. The results of a
verified action were previously ephemeral. AstraOS needs a durable,
explainable representation of what it knows without coupling that
representation to project scanning, execution, a database, or an AI provider.

## Decision

Add an independent `astra-knowledge` crate. Knowledge is an evidence-backed
claim about a subject, predicate, and value. Claims carry a category, stable
deterministic identifier, confidence, validity, evidence references, validity
conditions, and creation time. The initial categories are facts, decisions,
verifications, and goals.

Knowledge IDs are SHA-256 values derived from the category, subject, predicate,
and canonical JSON value. Evidence stores identifiers, locators, and
fingerprints only; it never stores source files, diffs, credentials,
environment variables, or command output. The model rejects a small set of
obvious sensitive field names (`password`, `token`, `secret`, `authorization`,
`private_key`, and `api_key` variants) without attempting to be a secret
scanner.

Validity is historical state, not deletion. A claim can be current, stale,
invalidated, or unknown. State-bound verification claims preserve their
historical result and can be projected as stale when the current source-state
fingerprint differs.

Storage is a versioned, deterministic, inspectable file hierarchy under
`~/.astra/knowledge` (or `ASTRA_KNOWLEDGE_DIR` for isolated environments).
Writes use a temporary file, sync, and rename. Every envelope includes
`KNOWLEDGE_SCHEMA_VERSION`; unsupported versions and corruption are reported as
typed errors so migrations can be added without silently changing meaning.

Knowledge remains a foundational model. Context, action, and execution crates
do not depend on it. CLI adapters translate their structured outputs into
knowledge claims. The first integration stores the Milestone 11 check result
with project identity, action identity, state/action/plan fingerprints, exit
status, verdict, and typed execution evidence.

The CLI exposes read-only queries: `astra knowledge list`, `show`, `facts`,
`verifications`, and `decisions`. There are no mutation, editing, deletion,
planning, or AI commands in this milestone.

## Alternatives considered

- **Execution-owned history:** rejected because knowledge would be coupled to
  one producer and could not represent context facts, ADR decisions, or goals.
- **SQLite or a graph database:** deferred until the file model and schema
  stabilize; the initial data set is small and should remain inspectable.
- **Vector or semantic memory:** rejected; Milestone 12 stores evidence-backed
  facts, not embeddings, chat history, or model-generated summaries.
- **Knowledge depending on context/actions/execution:** rejected to preserve a
  foundational dependency direction. Producer adapters belong at the CLI or a
  later integration boundary.

## Consequences

Knowledge survives process completion and can explain its provenance without
persisting sensitive content. Verification freshness can be evaluated against
state fingerprints while historical claims remain available. The file-backed
format is intentionally limited: queries are structured and deterministic,
but there is no full-text search, automatic migration beyond version
detection, synchronization, encryption subsystem, or automatic ingestion of
all context and ADR facts yet.
