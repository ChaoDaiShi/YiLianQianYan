# ADR 001 Optional Identity and Local First

状态：Accepted
日期：2026-10-06

## Decision

YiLianQianYan creates a stable LocalIdentity before any account flow. Guest mode retains the complete local Agent, Task, Canvas, MCP, Voice, Memory and Resource runtime. Authentication adds account, device and sync capabilities; it is never an application start requirement.

The identity state is one of `guest`, `authenticated`, `offline_authenticated`, or `closing`. Cloud outages transition an authenticated client to `offline_authenticated`; they do not disable local work. Login after local use requires an explicit choice between synchronizing existing data, keeping it local, or deciding later.

## Consequences

- Local data always has a stable owner ID before cloud registration.
- UI may advertise account benefits without blocking or degrading Guest mode.
- Cloud features use capability detection and feature flags.
- Session credentials are stored as secrets, not ordinary settings or SQLite plaintext.
