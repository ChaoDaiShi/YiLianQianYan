import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import accountCenterSource from "./AccountCenterPage.tsx?raw";
import accountPanelSource from "../features/cloud/AccountProfilePanel.tsx?raw";
import { AccountProfilePanel } from "../features/cloud/AccountProfilePanel";
import type { LogoutReport } from "../api/cloud";

describe("AccountCenterPage and AccountProfilePanel contracts (P2)", () => {
  it("removes manual service origin and account id inputs from profile switch", () => {
    // Manual inputs must be removed in favor of explicit login
    expect(accountPanelSource).not.toContain('placeholder="https://cloud.example.com"');
    expect(accountPanelSource).not.toContain('placeholder="user-uuid-or-id"');
    expect(accountPanelSource).not.toContain("importGuestCheck");
    expect(accountPanelSource).toContain("onRequestLogin");
  });

  it("links AccountCenterPage with onRequestLogin to reveal login section", () => {
    expect(accountCenterSource).toContain("showSwitchLogin");
    expect(accountCenterSource).toContain("onRequestLogin");
    expect(accountCenterSource).toContain("取消切换");
  });

  it("keeps the real logout report and renders profile status for guests", () => {
    expect(accountCenterSource).toContain("setLastLogoutReport(result.data.report)");
    expect(accountCenterSource).toContain("lastLogoutReport={lastLogoutReport}");
    expect(accountCenterSource).toContain("identity?.status === \"guest\"");
    expect(accountPanelSource).toContain("pending_revocations");
    expect(accountPanelSource).toContain("远端撤销待处理");
  });

  it("renders AccountProfilePanel with onRequestLogin button", () => {
    const onRequestLogin = vi.fn();
    const html = renderToStaticMarkup(
      <AccountProfilePanel
        onRequestLogin={onRequestLogin}
      />
    );
    expect(html).toContain("切换账号");
    expect(html).toContain("登录目标账号");
  });

  it("renders distinct local cleanup and remote revocation states in logout report", () => {
    const report: LogoutReport = {
      local_signed_out: true,
      local_cleanup: "cleared",
      remote_revocation: "pending",
      error_codes: [],
    };
    const html = renderToStaticMarkup(
      <AccountProfilePanel
        lastLogoutReport={report}
      />
    );
    expect(html).toContain("本地已退出");
    expect(html).toContain("远端注销待重试");
    expect(html).toContain("本地凭据已清除");
  });

  it("renders failure when local cleanup failed with error codes", () => {
    const report: LogoutReport = {
      local_signed_out: false,
      local_cleanup: "failed",
      remote_revocation: "unknown",
      error_codes: ["LOCAL_SECRET_DELETE_FAILED"],
    };
    const html = renderToStaticMarkup(
      <AccountProfilePanel
        lastLogoutReport={report}
      />
    );
    expect(html).toContain("本地退出未完成");
    expect(html).toContain("远端状态未知");
    expect(html).toContain("LOCAL_SECRET_DELETE_FAILED");
  });
});
