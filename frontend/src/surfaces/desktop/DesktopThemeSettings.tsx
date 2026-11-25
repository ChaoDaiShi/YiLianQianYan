import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Window } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../../components/ui";
import { useTheme } from "../../theme";

type Provider = "wallpaper_engine" | "lively";
type WallpaperPreset = "daylight" | "night";
type TargetAction = { type: "native"; preset?: WallpaperPreset } | { type: "playback"; provider: Provider; play: boolean };
interface MonitorWallpaper { id: string; label: string; path: string }
interface ProviderStatus { id: Provider; running: boolean; commandAvailable: boolean; version: string | null }
interface WallpaperState { monitors: MonitorWallpaper[]; providers: ProviderStatus[] }

const names = { wallpaper_engine: "Wallpaper Engine", lively: "Lively Wallpaper" } as const;
const presets = { daylight: "白昼", night: "夜色" } as const;
const LINK_THEME_KEY = "yilian:desktop-link-theme";

export default function DesktopThemeSettings() {
  const theme = useTheme();
  const [state, setState] = useState<WallpaperState | null>(null);
  const [target, setTarget] = useState("");
  const [pending, setPending] = useState<TargetAction | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [linkTheme, setLinkTheme] = useState(() => localStorage.getItem(LINK_THEME_KEY) === "true");
  const [widget, setWidget] = useState<boolean | null>(null);
  const [widgetBusy, setWidgetBusy] = useState(false);

  async function refresh() {
    setError("");
    setBusy(true);
    try {
      const result = await invoke<WallpaperState>("desktop_wallpaper_status");
      setState(result);
      setTarget(id => result.monitors.some(monitor => monitor.id === id) ? id : result.monitors[0]?.id ?? "");
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => { void refresh(); }, []);
  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    const sync = () => {
      void Window.getByLabel("desktop-widget")
        .then(value => { if (active) setWidget(Boolean(value)); })
        .catch(() => undefined);
    };
    sync();
    void listen<string>("floating:closed", event => {
      if (active && event.payload === "desktop-widget") setWidget(false);
    }).then(dispose => { if (active) stop = dispose; else dispose(); }).catch(() => undefined);
    window.addEventListener("focus", sync);
    return () => { active = false; stop?.(); window.removeEventListener("focus", sync); };
  }, []);

  async function toggleWidget() {
    setWidgetBusy(true);
    try {
      await invoke("desktop_widget_set", { open: !widget });
      setWidget(!widget);
    } catch (error) { setError(String(error)); }
    finally { setWidgetBusy(false); }
  }

  async function apply() {
    if (!pending) return;
    setBusy(true);
    setStatus("");
    setError("");
    const action = pending;
    setPending(null);
    try {
      if (action.type === "native") {
        const result = await invoke<MonitorWallpaper | null>("desktop_wallpaper_apply", { monitor: target, preset: action.preset });
        if (!result) { setStatus("已取消壁纸选择。"); return; }
        setState(current => current ? { ...current, monitors: current.monitors.map(monitor => monitor.id === result.id ? result : monitor) } : null);
        if (linkTheme && action.preset) theme.setMode(action.preset === "daylight" ? "light" : "dark");
        setStatus(`${result.label}的 Windows 壁纸已更新。`);
      } else {
        await invoke("desktop_wallpaper_playback", { provider: action.provider, play: action.play });
        setStatus(`已向 ${names[action.provider]} 发送${action.play ? "播放" : "暂停"}命令。`);
      }
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function preview(open: boolean) {
    setBusy(true);
    setStatus("");
    setError("");
    try {
      const visible = await invoke<boolean | null>("desktop_wallpaper_engine_preview", { open });
      if (visible === null) setStatus("已取消视频选择。");
      else setStatus(visible ? "Wallpaper Engine 预览窗口已打开。" : "Wallpaper Engine 预览窗口已关闭。");
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }

  const monitor = state?.monitors.find(item => item.id === target);
  const externalRunning = state?.providers.some(provider => provider.running);
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-semibold">Windows 壁纸</h3>
        <Button variant="ghost" size="sm" disabled={busy} onClick={() => void refresh()}>刷新状态</Button>
      </div>
      {monitor && <div className="space-y-2">
        <label className="block text-sm">目标显示器
          <select className="ui-input block w-full" value={target} disabled={busy || Boolean(pending)} onChange={event => setTarget(event.target.value)}>
            {state?.monitors.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}
          </select>
        </label>
        <p className="break-all text-sm text-[var(--text-muted)]">{monitor.path || "未设置原生图片壁纸"}</p>
        <label className="block text-sm"><input type="checkbox" checked={linkTheme}
          onChange={event => { setLinkTheme(event.target.checked); localStorage.setItem(LINK_THEME_KEY, String(event.target.checked)); }} /> 内置壁纸同步界面明暗</label>
        <div className="flex flex-wrap gap-2">
          {Object.entries(presets).map(([key, name]) => <Button key={key} variant="secondary" size="sm"
            disabled={busy || externalRunning || Boolean(pending)} onClick={() => setPending({ type: "native", preset: key as WallpaperPreset })}>{name}壁纸</Button>)}
          <Button variant="secondary" size="sm" disabled={busy || externalRunning || Boolean(pending)}
            onClick={() => setPending({ type: "native" })}>选取图片</Button>
        </div>
        {externalRunning && <p className="text-sm text-[var(--text-muted)]">外部壁纸软件正在运行。结束外部会话后可设置 Windows 壁纸。</p>}
      </div>}
      <fieldset className="space-y-2">
        <legend className="text-sm font-semibold">外部壁纸提供者</legend>
        {state?.providers.map(provider => <div key={provider.id} className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <p className="text-sm">{names[provider.id]}{provider.version ? ` · ${provider.version}` : ""}</p>
            <p className="text-sm text-[var(--text-muted)]">{provider.running ? (provider.commandAvailable ? "运行中 · 命令行可用" : "运行中 · 命令行不可访问") : "未运行"}</p>
          </div>
          {provider.commandAvailable && <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="secondary" disabled={busy || Boolean(pending)}
              onClick={() => setPending({ type: "playback", provider: provider.id, play: true })}>播放</Button>
            <Button size="sm" variant="secondary" disabled={busy || Boolean(pending)}
              onClick={() => setPending({ type: "playback", provider: provider.id, play: false })}>暂停</Button>
            {provider.id === "wallpaper_engine" && <>
              <Button size="sm" variant="secondary" disabled={busy || Boolean(pending)}
                onClick={() => void preview(true)}>在窗口预览视频</Button>
              <Button size="sm" variant="secondary" disabled={busy || Boolean(pending)}
                onClick={() => void preview(false)}>关闭预览</Button>
            </>}
          </div>}
        </div>)}
      </fieldset>
      {pending && <div className="space-y-2 rounded-lg border border-[var(--border-soft)] p-3" aria-live="polite">
        <p className="text-sm">{pending.type === "native"
          ? `应用到${monitor?.label ?? "所选显示器"}？`
          : `在 ${names[pending.provider]} 中${pending.play ? "播放" : "暂停"}所有壁纸？`}</p>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="secondary" onClick={() => setPending(null)}>取消</Button>
          <Button size="sm" onClick={() => void apply()}>确认</Button>
        </div>
      </div>}
      <label className="block text-sm"><input type="checkbox" checked={widget === true} disabled={widget === null || widgetBusy}
        onChange={() => void toggleWidget()} /> 显示桌面组件（实验）</label>
      {busy && <p role="status" className="text-sm">正在与壁纸提供者通信…</p>}
      {status && <p role="status" className="text-sm">{status}</p>}
      {error && <p role="alert" className="break-words text-sm text-[var(--danger)]">{error}</p>}
    </div>
  );
}
