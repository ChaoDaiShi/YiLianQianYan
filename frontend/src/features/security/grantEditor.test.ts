import { describe, expect, it } from "vitest";
import { grantPayload, grantWarning, initialGrantEditorState, isolationRows, mcpGrantError } from "./grantEditorModel";

describe("grant editor", () => {
  it("validates bounded literal identities and explicitly warns about server scope", () => {
    expect(mcpGrantError("A", "")).toBeNull();
    expect(mcpGrantError("A", "Publish")).toBeNull();
    for (const invalid of [" ", "*", "a*", "a\nb", "\u007f", "a".repeat(257), "界".repeat(86)]) {
      expect(mcpGrantError(invalid, "publish")).not.toBeNull();
      expect(mcpGrantError("A", invalid)).not.toBeNull();
    }
    expect(mcpGrantError("", "publish")).not.toBeNull();
    expect(grantWarning({ ...initialGrantEditorState, kind: "mcp" })).toContain("逐次审批");
  });
  it("builds exact MCP and server-wide grants without changing identity", () => {
    for (const toolName of ["Publish", ""]) {
      const payload = grantPayload({ ...initialGrantEditorState, kind: "mcp", serverId: "Server-A", toolName, effect: "deny" });
      expect(payload).toEqual({ permission_id: "mcp.invoke", effect: "deny", resource: { type: "mcp", server_id: "Server-A", tool_name: toolName || null } });
    }
  });
  it("builds a filesystem grant create payload", () => {
    const payload = grantPayload({ ...initialGrantEditorState, root: "C:/workspace" });
    expect(payload.resource).toEqual({ type: "filesystem", root: "C:/workspace", recursive: true });
  });

  it("builds a process grant payload", () => {
    const payload = grantPayload({ ...initialGrantEditorState, kind: "process" });
    expect(payload.resource).toEqual({ type: "process", scope: { kind: "managed_children" } });
  });

  it("warns for private and loopback network grants", () => {
    expect(grantWarning({ ...initialGrantEditorState, kind: "network", zone: "private" })).toContain("私有网络");
    expect(grantWarning({ ...initialGrantEditorState, kind: "network", zone: "loopback" })).toContain("回环");
  });

  it("warns for host shell grants", () => {
    expect(grantWarning({ ...initialGrantEditorState, kind: "shell", hostEscapeAcknowledged: true })).toContain("逃逸");
  });

  it("labels isolation capabilities without claiming a full sandbox", () => {
    const rows = isolationRows({ privilege_reduction: true, restricting_sids: false, job_object: true, process_containment: true, filesystem_os_enforced: false, network_os_enforced: false });
    expect(rows[0]).toEqual(["Privilege-reduced token", true]);
    expect(rows[1]).toEqual(["Restricting-SID ACL sandbox", false]);
  });
});
