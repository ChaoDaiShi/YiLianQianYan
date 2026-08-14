# Changelog

## Unreleased

### v0.3 Runtime Intelligence

- Added local Memory Runtime with validated extraction, sensitive-data filtering, lexical and hybrid retrieval, embedding fallback, reindexing, and chat-context injection.
- Added stdio MCP Runtime with tool discovery, namespaced adapters, production registry integration, and approval-aware execution.
- Added single-layer Subagent Runtime with strict `AGENT.md` discovery, private instructions, allowlisted child tools, isolated execution state, and bounded results.
- Kept Workflow at the Template / Prompt Template stage; executable DAG workflows remain outside v0.3.

### v0.2 Trusted Execution

- Added the unified `SecurityExecutionGateway`, runtime `PolicyEngine`, RBAC subject resolution, sandbox path enforcement, approval execution unification, deterministic verification, and redacted security auditing.
