import { describe, expect, it } from "vitest";
import { grantPayload, grantWarning, initialGrantEditorState, isolationRows } from "./grantEditorModel";

describe("grant editor", () => {
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
