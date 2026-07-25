# ADR 0001: Use a Rust Workspace

## Status

Accepted

## Context

AstraOS began as a single Rust binary. The project now includes system,
workspace, project-generation, configuration, and dashboard responsibilities.

## Decision

Use a Cargo workspace with one CLI application and focused library crates.

## Alternatives considered

- Keep a single crate
- Split into unrelated repositories

## Consequences

- Clearer ownership boundaries
- Faster focused testing
- More initial structure and dependency wiring
