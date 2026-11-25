import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../../components/ui";
import { getDesktopIntegrationStatus, type DesktopApplicationStatus } from "./desktopIntegrations";

type RainmeterAction = { type: "open_manager" } | { type: "refresh_skin"; config: string };

export default function RainmeterSettings() {
  const [application, setApplication] = useState<DesktopApplicationStatus | null>(null);
  const [selected, setSelected] = useState("");
  const [pending, setPending] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  async function refresh() {
    setError("");
    try { setApplication((await getDesktopIntegrationStatus()).rainmeter); }
    catch (error) { setError(String(error)); }
  }
  useEffect(() => { void refresh(); }, []);

  async function run(action: RainmeterAction) {
    setBusy(true);
    setError("");
    setNotice("");
    setPending(null);
    try {
      await invoke("desktop_rainmeter_invoke", { action });
      setNotice("已向 Rainmeter 发送命令。");
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }

  return <div className="space-y-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <p role="status" className="text-sm">
        {!application ? "等待读取 Rainmeter 状态" : application.running
          ? `Rainmeter 运行中${application.version ? ` · ${application.version}` : ""} · ${application.commandAvailable ? "命令行可用" : "命令行不可访问"}`
          : "Rainmeter 未运行"}
      </p>
      <Button size="sm" variant="ghost" disabled={busy} onClick={() => void refresh()}>刷新状态</Button>
    </div>
    {application?.commandAvailable && <>
      <Button variant="secondary" disabled={busy} onClick={() => void run({ type: "open_manager" })}>打开皮肤管理</Button>
      <label className="block text-sm">皮肤配置名
        <input className="ui-input block w-full" value={selected} maxLength={160} placeholder="illustro\Clock"
          disabled={busy || pending !== null} onChange={event => setSelected(event.target.value)} />
      </label>
      {pending === null ? <Button variant="secondary" disabled={!selected.trim() || busy}
        onClick={() => setPending(selected.trim())}>刷新皮肤</Button> :
        <div className="space-y-2 rounded-lg border border-[var(--border-soft)] p-3">
          <p className="break-all text-sm">重新载入 {pending} 的皮肤文件？</p>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="secondary" onClick={() => setPending(null)}>取消</Button>
            <Button size="sm" onClick={() => void run({ type: "refresh_skin", config: pending })}>确认刷新</Button>
          </div>
        </div>}
    </>}
    {notice && <p role="status" className="text-sm">{notice}</p>}
    {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
  </div>;
}
