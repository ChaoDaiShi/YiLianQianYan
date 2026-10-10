import {
  switchCloudProfile,
  logoutCloud,
  type ActivationPlan,
  type LogoutReport,
  type ProfileSwitchPayload,
  type RemoteRevocationStatus,
} from "../../api/cloud";

export type SwitchState =
  | "idle"
  | "quiescing"
  | "prepared"
  | "awaiting_restart"
  | "active"
  | "failed";

export interface ProfileSwitchOptions {
  onBeforeSwitch?: () => void;
  onQuiesce?: () => void;
  onRestartRequired?: (plan: ActivationPlan) => void;
  onSuccess?: (plan: ActivationPlan) => void;
  onError?: (error: string) => void;
}

export class ProfileSwitchController {
  private state: SwitchState = "idle";
  private currentGeneration: string | null = null;
  private activeProfileId: string | null = null;
  private abortController: AbortController = new AbortController();
  private pendingRevocation: RemoteRevocationStatus | null = null;

  constructor(initialGeneration?: string, initialProfileId?: string) {
    if (initialGeneration) this.currentGeneration = initialGeneration;
    if (initialProfileId) this.activeProfileId = initialProfileId;
  }

  public getState(): SwitchState {
    return this.state;
  }

  public getCurrentGeneration(): string | null {
    return this.currentGeneration;
  }

  public getActiveProfileId(): string | null {
    return this.activeProfileId;
  }

  public setProfileInfo(profileId: string, generation: string): void {
    this.activeProfileId = profileId;
    this.currentGeneration = generation;
  }

  public getSignal(): AbortSignal {
    return this.abortController.signal;
  }

  public abortInFlight(): void {
    this.abortController.abort();
    this.abortController = new AbortController();
  }

  public validateGeneration(generation: string | null | undefined): boolean {
    if (!this.currentGeneration || !generation) {
      return true;
    }
    return this.currentGeneration === generation;
  }

  public async switchProfile(
    payload: ProfileSwitchPayload,
    options?: ProfileSwitchOptions
  ): Promise<{ ok: boolean; plan?: ActivationPlan; error?: string }> {
    options?.onBeforeSwitch?.();
    this.state = "quiescing";
    options?.onQuiesce?.();

    // Abort all in-flight fetch and SSE operations
    this.abortInFlight();

    try {
      const result = await switchCloudProfile(payload);
      if (!result.ok) {
        this.state = "failed";
        options?.onError?.(result.error);
        return { ok: false, error: result.error };
      }

      const plan = result.data.plan;
      this.currentGeneration = plan.lease.generation;
      this.activeProfileId = plan.profile_id;

      if (plan.requires_restart) {
        this.state = "awaiting_restart";
        options?.onRestartRequired?.(plan);
      } else {
        this.state = "active";
        options?.onSuccess?.(plan);
      }

      return { ok: true, plan };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      this.state = "failed";
      options?.onError?.(message);
      return { ok: false, error: message };
    }
  }

  public async logoutWithReport(): Promise<LogoutReport> {
    this.abortInFlight();
    const result = await logoutCloud();
    if (result.ok && result.data.report) {
      this.pendingRevocation = result.data.report.remote_revocation;
      return result.data.report;
    }
    // Default fail-closed fallback
    const fallback: LogoutReport = {
      local_signed_out: true,
      remote_revocation: "unknown",
    };
    this.pendingRevocation = "unknown";
    return fallback;
  }

  public getPendingRevocation(): RemoteRevocationStatus | null {
    return this.pendingRevocation;
  }
}

export const defaultProfileSwitchController = new ProfileSwitchController();

export function revocationPresentation(status: RemoteRevocationStatus | null | undefined): {
  label: string;
  tone: "success" | "warning" | "default";
  detail: string;
} {
  switch (status) {
    case "confirmed":
      return {
        label: "远端已注销",
        tone: "success",
        detail: "本地与云端会话均已成功撤销。",
      };
    case "pending":
      return {
        label: "远端注销待重试",
        tone: "warning",
        detail: "本地凭据已清除，网络恢复后将在后台安全重试撤销远端会话。",
      };
    case "unknown":
      return {
        label: "远端状态未知",
        tone: "warning",
        detail: "本地已退出登录；建议从其他已登录设备或控制台确认撤销。",
      };
    default:
      return {
        label: "未登录或无撤销待办",
        tone: "default",
        detail: "当前没有在途撤销任务。",
      };
  }
}
