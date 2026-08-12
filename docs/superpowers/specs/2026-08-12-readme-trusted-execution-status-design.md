# README Trusted Execution Status Update Design

## Objective

Update README.md so its security architecture and version-status claims match the current develop branch. This is a documentation-only change.

## File scope

Modify:

- README.md

Do not modify:

- Rust or TypeScript source files;
- build or runtime configuration;
- Agent prompts;
- UI assets;
- package versions.

No CHANGELOG.md or docs/version.md exists, so no version file will be added.

## Editing approach

Preserve the current README structure and replace only stale security/status passages. Add one v0.2 status section near the security overview so readers can distinguish completed runtime integration from later roadmap work.

## Content structure

### Unified execution architecture

State that all Tool Calls currently connected to the Agent and Approval runtime execute through SecurityExecutionGateway.

Document this chain:

Tool Call
→ ToolSecurityDescriptor
→ ResourceScope Resolution
→ Sandbox Policy
→ SafetyPolicy Risk Evaluation
→ PolicyEngine Decision
→ Allow / RequireApproval / Deny
→ Tool Execution
→ Deterministic Verification
→ Redacted Audit Recording

### Security components

SecurityExecutionGateway:

- single Tool execution entry;
- connects PolicyEngine, Sandbox, Verifier, and AuditRecorder;
- used by normal Agent Tool Calls and approved Tool resume;
- future modules must use the same gateway rather than bypass it.

PolicyEngine:

- evaluates RBAC role, Capability, Permission, ResourceScope, and final risk;
- returns Allow, RequireApproval, or Deny;
- unknown or invalid security metadata remains fail-closed.

Sandbox:

- application-level path enforcement only;
- profiles: read-only, workspace-write, custom, open;
- supports writable_paths and denied_write_paths;
- explicit denied paths take precedence;
- does not claim Windows AppContainer, Linux namespace, an isolated worker, or OS-level isolation.

### Approval lifecycle

Document:

Tool Request
→ Policy Decision
→ RequireApproval
→ User approves/rejects
→ SecurityExecutionGateway resume
→ Execute once
→ Verify
→ Audit

State that approval executes the original Tool Call, does not ask the LLM to regenerate it, and atomic consumption prevents replay. Reject and cancel do not execute the Tool.

### Audit lifecycle

List implemented runtime events:

- policy_decided
- approval_requested
- approval_resolved
- execution_started
- execution_finished
- verification_finished

State that AuditRecorder applies redaction before persistence and does not persist API keys, tokens, passwords, cookies, data-URI bodies, or unrestricted sensitive raw content.

### Version status

Add:

- v0.2 Trusted Execution: Completed;
- Unified SecurityExecutionGateway;
- PolicyEngine Runtime Integration;
- Sandbox Path Enforcement;
- Approval Execution Unification;
- Verification Pipeline;
- Security Audit Chain.

Roadmap:

- v0.3 Memory & Extensions;
- Memory Extraction;
- Embedding Retrieval;
- MCP Runtime;
- Subagent Runtime.

Roadmap wording must remain future tense.

## Accuracy boundaries

README must not claim:

- OS-level Sandbox isolation;
- completed MCP Tool Runtime;
- a complete Workflow Engine beyond current Workflow Templates;
- a complete Multi-Agent Runtime beyond Subagent infrastructure;
- durable PendingApproval persistence across backend restarts.

## Validation

- Review the README diff and confirm only documentation changed.
- Search for stale claims that PolicyEngine, SecurityExecutionGateway, runtime audit, or Sandbox are still unintegrated.
- Search for forbidden overclaims.
- Compare the final execution chain and event list against current develop source.
- Confirm UTF-8 Markdown renders without replacement characters.

