import { afterEach, expect, it, vi } from "vitest";
vi.mock("./controlSession", () => ({ controlSessionHeaders: () => ({ "x-yilian-control-session": "fixture" }) }));
import { ensurePluginService, getPluginService, startPluginService, stopPluginService } from "./pluginServices";
afterEach(() => vi.unstubAllGlobals());
it("uses protected service routes and sends the exact approved launch configuration", async () => {
  const fetch = vi.fn().mockImplementation(async () => new Response(JSON.stringify({ state: "stopped" }), { status: 200 }));
  vi.stubGlobal("fetch", fetch);
  const launch = { executable: "F:/NapCat/NapCatWinBootMain.exe", working_directory: "F:/NapCat", args: ["10086"] };
  await getPluginService("qq.plugin", "napcat");
  await startPluginService("qq.plugin", "napcat", launch);
  await stopPluginService("qq.plugin", "napcat");
  expect(fetch.mock.calls.map(call => new URL(call[0]).pathname)).toEqual([
    "/api/plugins/qq.plugin/services/napcat", "/api/plugins/qq.plugin/services/napcat/start", "/api/plugins/qq.plugin/services/napcat/stop",
  ]);
  expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual({ launch });
  expect(fetch.mock.calls[1][1].headers["x-yilian-control-session"]).toBe("fixture");
});

it("keeps the server launch confirmation and explicitly scopes model consent", async () => {
  const launch = { executable: "F:/service.exe", working_directory: "F:/", args: [] };
  const fetch = vi.fn().mockImplementationOnce(async () => Response.json({
    error: "请确认服务启动配置", code: "launch_confirmation_required", launch, configuration_revision: "revision-1",
  }, { status: 409 })).mockImplementationOnce(async () => Response.json({ state: "running", browser_tab_id: "tab-1" }));
  vi.stubGlobal("fetch", fetch);
  await expect(ensurePluginService("demo", "service", { acquire_page: true })).rejects.toMatchObject({
    status: 409, details: { code: "launch_confirmation_required", launch, configuration_revision: "revision-1" },
  });
  expect(await ensurePluginService("demo", "service", { launch, configuration_revision: "revision-1", allow_model: true, acquire_page: true })).toMatchObject({ browser_tab_id: "tab-1" });
  expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual({ launch, configuration_revision: "revision-1", allow_model: true, acquire_page: true });
});
