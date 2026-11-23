import {describe, expect, it} from "vitest";
import {emptySoftwareDraft, softwareDisplayExecutable, softwareExecutableDraft, softwareRegistrationDraft, validateSoftwareDraft} from "./softwareForm";

describe("software registration preparation", () => {
  it("starts with AI permissions off", () => {
    expect(emptySoftwareDraft()).toMatchObject({allow_observe: false, allow_actions: false, allow_content: false, allow_window_ocr: false, adapter: "generic"});
  });
  it.each(["Weixin.exe", "https://example.test/Weixin.exe", "C:\\Apps\\Weixin.exe --login", 'C:\\Apps\\Weixin.exe"', "C:\\Apps\\Weixin.exe\n"])("does not accept a command in place of an EXE path: %s", executable => {
    const result = validateSoftwareDraft({...emptySoftwareDraft(), executable});
    // Whitespace outside a path is normalized; embedded commands are never accepted.
    if (executable.endsWith("\n")) expect(result.value?.executable).toBe("C:\\Apps\\Weixin.exe");
    else expect(result.error).toBeTruthy();
  });
  it("retains project identity independently from the executable", () => {
    const a = validateSoftwareDraft({...emptySoftwareDraft(), executable: "C:\\软件\\Weixin.exe", workspace_id: "project-a"});
    const b = validateSoftwareDraft({...emptySoftwareDraft(), executable: "C:\\软件\\Weixin.exe", workspace_id: "project-b"});
    expect(a.value?.workspace_id).toBe("project-a");
    expect(b.value?.workspace_id).toBe("project-b");
  });
  it("does not permit action permission without explicit observation permission", () => {
    expect(validateSoftwareDraft({...emptySoftwareDraft(), executable: "C:\\Apps\\Weixin.exe", allow_actions: true}).error).toContain("先明确允许观察");
  });
  it("opens a canonical local registration for editing without changing its authority snapshot or permissions", () => {
    const registered = {...emptySoftwareDraft(), id: "registered", revision: 7, executable: "\\\\?\\F:\\软件\\Weixin.exe", allow_observe: true, allow_actions: true};
    const draft = softwareRegistrationDraft(registered);
    expect(draft.executable).toBe("F:\\软件\\Weixin.exe");
    expect(draft).toMatchObject({allow_observe: true, allow_actions: true});
    const reduced = validateSoftwareDraft({...draft, allow_observe: false, allow_actions: false});
    expect(reduced.error).toBeUndefined();
    expect(reduced.value).toMatchObject({executable: "F:\\软件\\Weixin.exe", allow_observe: false, allow_actions: false});
    expect(registered.executable).toBe("\\\\?\\F:\\软件\\Weixin.exe");
    expect(registered.revision).toBe(7);
  });
  it("does not revoke permissions for an equivalent local-path display spelling", () => {
    const draft = {...emptySoftwareDraft(), executable: "\\\\?\\F:\\Apps\\Weixin.exe", allow_observe: true, allow_actions: true, allow_window_ocr: true};
    expect(softwareExecutableDraft(draft, "f:/apps/Weixin.exe")).toMatchObject({allow_observe: true, allow_actions: true});
    expect(softwareExecutableDraft(draft, "F:/Other/Weixin.exe")).toMatchObject({allow_observe: false, allow_actions: false, allow_window_ocr: false});
  });
  it.each(["\\\\server\\share\\Weixin.exe", "\\\\?\\UNC\\server\\share\\Weixin.exe", "\\\\?\\GLOBALROOT\\Device\\Weixin.exe", "\\\\.\\C:\\Apps\\Weixin.exe", "\\??\\C:\\Apps\\Weixin.exe"])("does not normalize an unapproved namespace into a local path: %s", executable => {
    expect(softwareDisplayExecutable(executable)).toBe(executable);
    expect(validateSoftwareDraft({...emptySoftwareDraft(), executable}).error).toBeTruthy();
  });
});
