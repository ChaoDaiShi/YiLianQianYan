import assert from "node:assert/strict";
import {join} from "node:path";

export async function capabilityUpgrade({ page, backendPort, frontendPort, controlToken, evidenceDir }) {
  const base = "http://127.0.0.1:" + backendPort;
  const web = "http://127.0.0.1:" + frontendPort;
  const headers = { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken };
  async function api(path, method = "GET", data) {
    const res = await fetch(base + path, {
      method,
      headers,
      body: data === undefined ? undefined : JSON.stringify(data),
    });
    assert.ok(res.ok, method + " " + path + " " + res.status);
    return res.json();
  }
  const go = (path) => page.goto(web + path, { waitUntil: "domcontentloaded" });
  const next = () => page.getByRole("button", { name: "下一步", exact: true }).click();
  await go("/setup");
  await page.getByRole("heading", { name: "欢迎使用忆涟千言" }).waitFor();
  await next();
  await page.getByLabel("希望小涟怎样称呼你").fill("验收用户");
  await next();
  await page.getByLabel("回复长度").fill("先给简短结论");
  await page.getByRole("button", { name: "保存进度，稍后配置" }).click();
  await page.waitForURL("**/chat");
  await go("/setup");
  await page.getByLabel("回复长度").waitFor();
  assert.equal(await page.getByLabel("回复长度").inputValue(), "先给简短结论");
  const draft = await api("/api/v1/onboarding/status");
  assert.equal(draft.draft.fields.display_name, "验收用户");
  assert.equal(draft.profile.fields.display_name, undefined);
  await next();
  await page.getByLabel("允许请求屏幕截图").check();
  await page.setViewportSize({width:800,height:600});
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth <= window.innerWidth));
  await page.screenshot({path:join(evidenceDir,"onboarding-permissions.png")});
  await page.setViewportSize({width:1440,height:900});
  await next();
  await page.getByRole("heading", { name: "设备与模型" }).waitFor();
  await next();
  await page.getByLabel("默认工作空间").waitFor();
  await next();
  await page.getByRole("button", { name: "保存并开始使用" }).click();
  await page.waitForURL("**/chat");
  const profile = await api("/api/v1/profile");
  assert.equal(profile.profile.permissions.screen, true);
  assert.equal(profile.profile.fields.display_name, "验收用户");
  assert.ok(profile.field_dates.display_name > 0);
  const stale = await fetch(base + "/api/v1/profile", {
    method: "PATCH",
    headers,
    body: JSON.stringify({ expected_revision: 0, fields: { display_name: "过期编辑" } }),
  });
  assert.equal(stale.status, 409);
  await go("/setup?permissions=1");
  await page.getByLabel("允许请求屏幕截图").uncheck();
  await page.getByRole("button", { name: "立即保存权限" }).click();
  await page.waitForURL("**/chat");
  assert.equal((await api("/api/v1/profile")).profile.permissions.screen, false);
  const voice = await fetch(base + "/api/voice/sessions/start", { method: "POST", headers, body: "{}" });
  assert.equal(voice.status, 403);
  const events = await api("/api/v1/events?stream_id=" + encodeURIComponent("profile:" + profile.scope));
  assert.ok(events.events.length >= 2);
  assert.equal(
    (
      await api(
        "/api/v1/events?after=" +
          events.next_cursor +
          "&stream_id=" +
          encodeURIComponent("profile:" + profile.scope),
      )
    ).events.length,
    0,
  );
  await go("/settings?section=resource-policy");
  await page.getByRole("dialog", { name: "运行资源" }).waitFor();
  await page.getByLabel("资源模式").selectOption("efficiency");
  const [savedPolicy] = await Promise.all([
    page.waitForResponse(response => response.url().endsWith("/api/v1/resources/policy") && response.request().method()==="PUT"),
    page.getByRole("button", { name: "保存设置", exact: true }).click(),
  ]);
  assert.equal(savedPolicy.status(),200);
  assert.equal((await savedPolicy.json()).policy.mode,"efficiency");
  await page.screenshot({path:join(evidenceDir,"resource-policy.png")});
  assert.equal((await api("/api/v1/resources/policy")).policy.mode, "efficiency");
  return {
    status: "passed",
    checks: [
      "wizard-draft-resume",
      "atomic-consent",
      "revision-conflict",
      "microphone-denial",
      "screen-revocation",
      "scoped-event-cursor",
      "runtime-policy",
    ],
    real_backend: true,
  };
}
