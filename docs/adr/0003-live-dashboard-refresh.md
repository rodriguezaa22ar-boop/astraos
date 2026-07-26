# ADR 0003: Use Typed Snapshots and a Two-Rate Dashboard Loop

## Status

Accepted

## Context

The dashboard needs responsive keyboard input while collecting system metrics
and checking local developer services. Running commands from render functions
would couple presentation to system access and create excessive subprocesses.
Adding an asynchronous runtime would be disproportionate for a local terminal
dashboard.

## Decision

Use `sysinfo` for CPU, memory, disk, operating-system, hostname, and uptime
inspection. Use `starship-battery` for optional battery inspection on macOS and
other supported platforms.

Keep a single-threaded dashboard event loop with:

- a 100 ms render and input tick;
- a one-second metrics refresh interval;
- internally cached developer-service checks, refreshed at most every five
  seconds unless the user requests an immediate refresh.

Docker is checked with a bounded `docker info` child process. Ollama is checked
with a bounded HTTP request to its loopback version endpoint. Both checks
produce a stable `Running`, `Stopped`, `Unavailable`, or `Unknown` state.

Rendering receives typed snapshots and performs no probing. A terminal-session
guard restores raw mode, the alternate screen, and cursor visibility.

## Alternatives considered

- Run shell commands from each render pass
- Add an asynchronous runtime and background tasks
- Parse macOS-only command output for every metric
- Treat missing optional metrics as dashboard errors

## Consequences

- Input and rendering run more frequently than expensive probes.
- System and service behavior can be tested independently from terminal output.
- The dashboard remains usable when optional hardware or services are absent.
- A service check can briefly occupy the single-threaded loop, but every
  operation has a strict 500 ms timeout and service checks are cached.
- Docker status follows the daemon selected by the installed Docker CLI.
