# ADR 0002: Store Configuration as TOML

## Status

Accepted

## Context

Hardcoded paths make AstraOS difficult to move between machines.

## Decision

Store user configuration at `~/.config/astra/config.toml` using Serde and TOML.

## Alternatives considered

- Environment variables only
- JSON
- YAML
- Hardcoded defaults

## Consequences

- Human-readable configuration
- Typed deserialization
- Config migrations will eventually be required
