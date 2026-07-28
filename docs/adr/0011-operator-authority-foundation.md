# ADR 0011: Operator Authority Foundation

## Status

Accepted

## Context

Milestone 13 produces deterministic, evidence-backed Project Intelligence, but
it intentionally has no human authority layer. AstraOS needs to preserve the
difference between discovered reality, derived interpretation, and an
operator's governing response.

The governing principle is:

> AstraOS discovers reality, the operator governs interpretation, and history
> preserves the difference.

Operator responses are not knowledge claims. Knowledge answers what is known;
operator authority records how current understanding should be governed.
Observed information remains scanner-owned and cannot be corrected by this
layer. Derived insights may be accepted, rejected, corrected, disputed, or
annotated without mutating their evidence or historical base representation.

## Decision

Extend `astra-knowledge` with a separate, typed operator-response model and a
file-backed transaction log. It owns response identity, payload validation,
lifecycle, persistence, history, and project-scoped sequence allocation.
`astra-knowledge` does not depend on `astra-intelligence`.

`astra-intelligence` depends on the neutral operator-response vocabulary and
owns pure resolution:

```text
Base Project Intelligence + committed operator responses
                              ↓
                  Resolved Project Intelligence
```

The base model is never mutated. Base mode retains the Milestone 13 schema.
Resolved mode uses a new schema and reports resolution status, active
authority, conflicts, base intelligence, resolved interpretations,
annotations, and optional explanations.

### Response domain

Responses have a stable project-scoped ID, project, target binding, operator
identity, lifecycle, typed payload, optional supersession, and audit metadata.
Payload variants are annotation, acceptance, rejection, correction, and
dispute. Each variant contains only fields meaningful to that response type.
Human confidence and intent use separate bounded vocabularies.

Target bindings preserve the target ID, optional rule ID, target
classification, statement, deterministic evidence fingerprint, and related
entity IDs. Bindings contain references and hashes, never source contents,
diffs, command output, environment values, credentials, or absolute paths.

`LocalOperator` and `NamedOperator` are the initial identity forms. Identity
has a stable ID distinct from its display name, so display-name changes do not
rewrite history. Authentication, teams, and services are future concerns.

### Correction boundary

Observed targets may be annotated but cannot be accepted, rejected, corrected,
or disputed. A correction applies only to a derived insight and atomically
means that the old interpretation is rejected while a replacement
interpretation becomes operational. Neither operation alters the original
insight or promotes the replacement to observed fact.

### Lifecycle

Lifecycle states are draft, active, superseded, retired, withdrawn, expired,
review-required, and orphaned. Drafts are editable and deletable and have no
effect. Active records are immutable. Editing an active response is rejected;
replacement creates a new response and requires explicit supersession.

Acceptance and annotations activate immediately. Rejection, correction, and
dispute begin as drafts. Activation validates the exact current target
binding and locks the target, operator, payload, intent, and confidence.

Target drift is projected rather than written over history:

- acceptance and state-bound annotation become expired;
- rejection, correction, and dispute become review-required;
- a response whose target no longer exists becomes orphaned;
- persistent annotation remains active only while its exact target exists.

An orphan reconnects automatically only when target ID, rule, statement,
evidence fingerprint, and related entities all match.

### Transaction protocol

The store remains file-backed. Every mutating operation:

1. validates the operation and expected current state;
2. allocates project-scoped sequence IDs while holding a narrow store lock;
3. writes a versioned transaction manifest;
4. writes immutable response revisions;
5. writes a commit marker.

Readers replay only committed transactions in sequence order. Prepared but
uncommitted transactions remain inspectable and cannot affect understanding.
Draft edits keep the same response ID but add immutable revisions. Draft
deletion is a committed tombstone. Active transitions and replacements add new
responses; old records remain in history.

IDs contain no paths, timestamps, or random values. Timestamps are historical
audit metadata only and never participate in semantic identity or resolved
output.

### Determinism and trust

Collections are canonically ordered. Resolution is pure and has no filesystem,
Git, process, clock, configuration, or storage access. Resolved semantic JSON
contains no timestamps. Invalid option combinations are rejected. A
`--require-resolved` request fails when governing conflicts or disputes leave
the operational interpretation unresolved.

## Alternatives considered

- **Store responses as `KnowledgeClaim`:** rejected because operator governance
  is neither discovered fact nor evidentiary knowledge.
- **Mutate or replace base insights:** rejected because it destroys provenance
  and confuses operational resolution with objective truth.
- **Store only current state:** rejected because draft edits, supersession, and
  disagreement history must remain inspectable.
- **Use a database:** deferred. The initial model is small, local, and benefits
  from inspectable versioned files.
- **Put resolution in the CLI:** rejected because deterministic authority
  projection is reusable domain behavior.

## Consequences

AstraOS can now distinguish base and resolved understanding while preserving
evidence and history. Operators can govern derived interpretations without
rewriting observations. The first implementation is local and unauthenticated,
uses narrow transaction safety rather than a database, and does not implement
observation challenges, protected suppression, planning, orchestration, or
Milestone 13.2 operator-policy features.
