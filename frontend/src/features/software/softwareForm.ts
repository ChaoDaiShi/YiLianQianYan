/** Frontend preparation only. The native/backend validators remain authoritative. */
import type {SoftwareRegistration} from "../../api/software";
export interface SoftwareDraft {
  name: string;
  adapter: "generic" | "wechat" | "generic_fixture";
  executable: string;
  workspace_id: string;
  allow_observe: boolean;
  allow_content: boolean;
  allow_window_ocr: boolean;
  allow_actions: boolean;
}

export const emptySoftwareDraft = (): SoftwareDraft => ({
  name: "", adapter: "generic", executable: "", workspace_id: "",
  allow_observe: false, allow_content: false, allow_window_ocr: false, allow_actions: false,
});

/** Only Win32 verbatim LOCAL drive paths have a display equivalent here.
 * UNC, GLOBALROOT and device namespaces remain invalid form inputs. */
export function softwareDisplayExecutable(executable: string): string {
  return /^\\\\\?\\([a-zA-Z]:[\\/].*)$/.exec(executable)?.[1] ?? executable;
}

export function softwareRegistrationDraft(registration: SoftwareRegistration): SoftwareDraft {
  return {
    name: registration.name, adapter: registration.adapter as SoftwareDraft["adapter"],
    executable: softwareDisplayExecutable(registration.executable), workspace_id: registration.workspace_id ?? "",
    allow_window_ocr: registration.allow_window_ocr ?? false, allow_content: registration.allow_content ?? false, allow_observe: registration.allow_observe, allow_actions: registration.allow_actions,
  };
}

export function softwareExecutableDraft(draft: SoftwareDraft, executable: string): SoftwareDraft {
  const comparable = (value: string) => softwareDisplayExecutable(value.trim()).replace(/\\/g, "/").toLowerCase();
  const sameExecutable = comparable(draft.executable) === comparable(executable);
  return {...draft, executable,
    allow_window_ocr: sameExecutable && draft.allow_window_ocr,
    allow_content: sameExecutable && draft.allow_content,
    allow_observe: sameExecutable && draft.allow_observe,
    allow_actions: sameExecutable && draft.allow_actions,
  };
}

export function validateSoftwareDraft(draft: SoftwareDraft): {value?: SoftwareDraft; error?: string} {
  const value = {...draft, name: draft.name.trim() || softwareDisplayExecutable(draft.executable.trim()).split(/[\\/]/).pop()?.replace(/\.exe$/i, "") || "", executable: softwareDisplayExecutable(draft.executable.trim())};
  if (!value.name || [...value.name].length > 80) return {error: "软件名称须为 1–80 个字符"};
  if (!/^[a-zA-Z]:[\\/]/.test(value.executable) || !/\.exe$/i.test(value.executable)
    || /[\u0000-\u001f"<>|?*]/.test(value.executable)) {
    return {error: "请填写本机现有 EXE 的完整绝对路径；不接受命令或启动参数"};
  }
  if ((value.allow_actions || value.allow_content || value.allow_window_ocr) && !value.allow_observe) return {error: "允许 AI 操作前须先明确允许观察控件"};
  return {value};
}

export function softwareError(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

export function formatSoftwareTime(value: number): string {
  const date = new Date(value);
  return Number.isFinite(date.getTime()) ? date.toLocaleString() : "时间不可用";
}
