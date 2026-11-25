import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/ui";
import { VTubeStudioProvider, type StudioModel } from "./vtubeStudio";
import { getDesktopIntegrationStatus, type DesktopApplicationStatus } from "./desktopIntegrations";

export default function VTubeStudioSettings() {
  const client = useRef<VTubeStudioProvider | null>(null);
  const refreshSequence = useRef(0);
  const [port, setPort] = useState(8001);
  const [model, setModel] = useState<StudioModel | null>(null);
  const [hotkey, setHotkey] = useState("");
  const [status, setStatus] = useState<"off" | "connecting" | "on">("off");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [sent, setSent] = useState("");
  const [application, setApplication] = useState<DesktopApplicationStatus | null>(null);
  const [version, setVersion] = useState<string | null>(null);

  function release() {
    const current = client.current;
    client.current = null;
    current?.disconnect();
  }
  useEffect(() => release, []);
  useEffect(() => {
    let mounted = true;
    void getDesktopIntegrationStatus().then(result => { if (mounted) setApplication(result.vtubeStudio); })
      .catch(() => undefined);
    return () => { mounted = false; };
  }, []);

  async function refresh(current: VTubeStudioProvider) {
    const sequence = ++refreshSequence.current;
    const result = await current.readModel();
    if (client.current !== current || refreshSequence.current !== sequence) return;
    setModel(result);
    setHotkey("");
  }

  async function connect() {
    setStatus("connecting");
    setError("");
    setSent("");
    let connection: VTubeStudioProvider | undefined;
    try {
      const current = new VTubeStudioProvider(port,
        () => { if (connection && client.current === connection) void refresh(connection).catch(error => setError(String(error))); },
        () => {
          if (client.current !== connection) return;
          client.current = null;
          setStatus("off");
          setModel(null);
          setError("VTube Studio 连接已断开。");
        });
      client.current = connection = current;
      const apiVersion = await current.connect();
      await refresh(current);
      if (client.current === current) { setStatus("on"); setVersion(apiVersion); }
    } catch (error) {
      if (connection && client.current !== connection) return;
      release();
      setStatus("off");
      setError(error instanceof Error ? error.message : String(error));
    }
  }

  async function trigger() {
    const current = client.current;
    if (!current || !model || !hotkey) return;
    setBusy(true);
    setError("");
    setSent("");
    try {
      await current.trigger(model, hotkey);
      setSent("VTube Studio 已确认热键。");
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }

  return <div className="space-y-3">
    {application && <p className="text-sm">{application.running
      ? `检测到 VTube Studio 进程${application.version ? ` · ${application.version}` : ""}`
      : "未检测到 VTube Studio 进程"}</p>}
    <p className="text-sm text-[var(--text-muted)]">在 VTube Studio 开启插件 API，连接后在其窗口授权。本连接在关闭此页时断开。</p>
    <label className="block text-sm">本机 API 端口
      <input type="number" min="1" max="65535" className="ui-input block w-full" value={port}
        disabled={status !== "off"} onChange={event => setPort(Number(event.target.value))} />
    </label>
    {status === "off" ? <Button onClick={() => void connect()}>连接 VTube Studio</Button> :
      <Button variant="secondary" onClick={() => { release(); setStatus("off"); setModel(null); setError(""); }}>断开</Button>}
    {status === "connecting" && <p role="status" className="text-sm">等待 VTube Studio 授权…</p>}
    {status === "on" && <p role="status" className="text-sm">{model?.modelLoaded ? `当前角色：${model.modelName}` : "已连接，VTube Studio 尚未载入模型。"}</p>}
    {status === "on" && <p className="text-sm text-[var(--text-muted)]">API 1.0{version ? ` · VTube Studio ${version}` : ""} · 角色切换订阅与热键可用</p>}
    {model && model.availableHotkeys.length > 0 && <>
      <label className="block text-sm">动画或表情热键
        <select className="ui-input block w-full" value={hotkey} onChange={event => setHotkey(event.target.value)}>
          <option value="">选择热键</option>
          {model.availableHotkeys.map(key => <option key={key.hotkeyID} value={key.hotkeyID}>{key.name}</option>)}
        </select>
      </label>
      <Button variant="secondary" disabled={!hotkey || busy} onClick={() => void trigger()}>触发所选热键</Button>
    </>}
    {sent && <p role="status" className="text-sm">{sent}</p>}
    {error && <p role="alert" className="break-words text-sm text-[var(--danger)]">{error}</p>}
  </div>;
}
