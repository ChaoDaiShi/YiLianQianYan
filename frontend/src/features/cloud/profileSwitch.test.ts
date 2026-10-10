import { describe, expect, it, vi } from "vitest";
import {
  ProfileSwitchController,
  revocationPresentation,
} from "./profileSwitch";
import * as cloudApi from "../../api/cloud";

describe("ProfileSwitchController", () => {
  it("rejects late response when generation does not match active profile", () => {
    const controller = new ProfileSwitchController("gen-1", "profile-guest");

    // Matching generation is valid
    expect(controller.validateGeneration("gen-1")).toBe(true);

    // Mismatched generation (e.g. from prior account before switch) is rejected
    expect(controller.validateGeneration("gen-old-0")).toBe(false);
    expect(controller.validateGeneration("gen-2")).toBe(false);
  });

  it("aborts active requests on in-flight abort", () => {
    const controller = new ProfileSwitchController("gen-1");
    const signal = controller.getSignal();
    expect(signal.aborted).toBe(false);

    controller.abortInFlight();
    expect(signal.aborted).toBe(true);

    // New signal is created and not yet aborted
    const newSignal = controller.getSignal();
    expect(newSignal.aborted).toBe(false);
  });

  it("handles successful profile switch requiring restart", async () => {
    const controller = new ProfileSwitchController("gen-1", "profile-guest");

    vi.spyOn(cloudApi, "switchCloudProfile").mockResolvedValue({
      ok: true,
      data: {
        plan: {
          profile_id: "profile-account-abc",
          lease: {
            scope_id: "scope-hash-123",
            generation: "gen-2",
          },
          requires_restart: true,
          import_count: 5,
        },
        status: "awaiting_restart",
      },
    });

    const onRestartRequired = vi.fn();
    const onQuiesce = vi.fn();

    const res = await controller.switchProfile(
      {
        service_origin: "https://cloud.example.com",
        account_id: "user-1",
      },
      {
        onQuiesce,
        onRestartRequired,
      }
    );

    expect(onQuiesce).toHaveBeenCalledTimes(1);
    expect(onRestartRequired).toHaveBeenCalledTimes(1);
    expect(res.ok).toBe(true);
    expect(controller.getState()).toBe("awaiting_restart");
    expect(controller.getCurrentGeneration()).toBe("gen-2");
    expect(controller.getActiveProfileId()).toBe("profile-account-abc");
  });

  it("handles switch failure gracefully", async () => {
    const controller = new ProfileSwitchController("gen-1", "profile-guest");

    vi.spyOn(cloudApi, "switchCloudProfile").mockResolvedValue({
      ok: false,
      error: "Quiesce timeout: active transactions in progress",
      status: 500,
    });

    const onError = vi.fn();
    const res = await controller.switchProfile(
      {
        service_origin: "https://cloud.example.com",
        account_id: "user-1",
      },
      { onError }
    );

    expect(res.ok).toBe(false);
    expect(onError).toHaveBeenCalledWith(
      "Quiesce timeout: active transactions in progress"
    );
    expect(controller.getState()).toBe("failed");
    // Original generation is preserved
    expect(controller.getCurrentGeneration()).toBe("gen-1");
  });

  it("tracks remote revocation reporting on logout", async () => {
    const controller = new ProfileSwitchController("gen-1");

    vi.spyOn(cloudApi, "logoutCloud").mockResolvedValue({
      ok: true,
      data: {
        report: {
          local_signed_out: true,
          remote_revocation: "pending",
        },
        activation: {
          profile_id: "guest-1",
          lease: { scope_id: "guest-scope", generation: "guest-generation" },
          requires_restart: true,
          import_count: 0,
        },
        status: "awaiting_restart",
      },
    });

    const report = await controller.logoutWithReport();
    expect(report.local_signed_out).toBe(true);
    expect(report.remote_revocation).toBe("pending");
    expect(controller.getPendingRevocation()).toBe("pending");

    const meta = revocationPresentation(report.remote_revocation);
    expect(meta.tone).toBe("warning");
    expect(meta.label).toBe("远端注销待重试");
    expect(meta.detail).toContain("后台安全重试");
  });
});
