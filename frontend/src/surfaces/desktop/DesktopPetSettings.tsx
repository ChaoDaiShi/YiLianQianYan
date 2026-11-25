import { useEffect, useRef, useState, type ChangeEvent, type InputHTMLAttributes } from "react";
import { Button } from "../../components/ui";
import { installCubismCore, loadCubismCore, loadPetModel, parseModelPackage, savePetModel, type PetModelPackage } from "./petModel";
import { PET_ACTIVITY_LABELS, readPetPreferences, writePetPreferences, type DesktopPetPreferences } from "./petPreferences";

const modelFolderInput = { webkitdirectory: "" } as InputHTMLAttributes<HTMLInputElement>;

export default function DesktopPetSettings() {
  const [preferences, setPreferences] = useState(readPetPreferences);
  const [model, setModel] = useState<PetModelPackage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const coreInput = useRef<HTMLInputElement>(null);
  const motionGroups = Object.keys(model?.manifest.FileReferences.Motions ?? {});
  const expressions = model?.manifest.FileReferences.Expressions ?? [];

  useEffect(() => {
    let active = true;
    void loadPetModel().then(value => { if (active) setModel(value); })
      .catch(() => { if (active) setError("无法读取已保存的 Live2D 模型。"); });
    return () => { active = false; };
  }, []);

  function update(changes: Partial<DesktopPetPreferences>) {
    const next = { ...preferences, ...changes };
    writePetPreferences(next);
    setPreferences(next);
  }

  async function selectModel(event: ChangeEvent<HTMLInputElement>) {
    const files = Array.from(event.currentTarget.files ?? []);
    event.currentTarget.value = "";
    if (!files.length) return;
    setBusy(true);
    setError("");
    setStatus("");
    try {
      const model = await parseModelPackage(files);
      await loadCubismCore();
      await savePetModel(model);
      update({ motionGroups: {}, expressions: {} });
      setModel(model);
      setStatus(`已保存 ${model.name}，桌宠打开时加载模型。`);
    } catch (error) {
      setError(error instanceof Error ? error.message : "导入模型失败。");
    } finally {
      setBusy(false);
    }
  }

  async function resetModel() {
    setBusy(true);
    setError("");
    try {
      await savePetModel(null);
      setModel(null);
      setStatus("已切回内置桌宠。");
    } catch {
      setError("无法移除当前模型。");
    } finally {
      setBusy(false);
    }
  }

  async function installCore(file?: File) {
    setBusy(true);
    setError("");
    setStatus("");
    try {
      await installCubismCore(file);
      setStatus("Cubism Core 已安装并通过校验。");
    } catch (error) {
      setError(error instanceof Error ? error.message : "无法安装 Cubism Core。");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-4">
      <fieldset className="space-y-2" aria-label="桌宠位置和动画">
        <label className="block"><input type="checkbox" checked={preferences.locked}
          onChange={event => update({ locked: event.target.checked })} /> 锁定位置</label>
        <label className="block"><input type="checkbox" checked={preferences.reducedMotion}
          onChange={event => update({ reducedMotion: event.target.checked })} /> 减少动态效果</label>
        {navigator.userAgent.includes("Windows") && <label className="block"><input type="checkbox" checked={preferences.hideOnFullscreen}
          onChange={event => update({ hideOnFullscreen: event.target.checked })} /> 前台应用全屏时隐藏桌宠</label>}
        <label className="block">
          <span className="flex justify-between"><span>桌宠大小</span><span>{Math.round(preferences.scale * 100)}%</span></span>
          <input className="w-full" type="range" min={0.8} max={2.5} step={0.1} value={preferences.scale}
            onChange={event => update({ scale: Number(event.target.value) })} />
        </label>
      </fieldset>
      <fieldset className="space-y-2">
        <legend className="text-sm font-semibold">Live2D 模型</legend>
        <p className="break-words text-sm">{model?.name ?? "使用内置桌宠"}</p>
        <input ref={input} type="file" multiple hidden {...modelFolderInput} onChange={event => void selectModel(event)} />
        <div className="flex flex-wrap gap-2">
          <Button variant="secondary" disabled={busy} onClick={() => input.current?.click()}>导入模型文件夹</Button>
          {model && <Button variant="ghost" disabled={busy} onClick={() => void resetModel()}>恢复内置桌宠</Button>}
        </div>
        <p className="text-sm text-[var(--text-muted)]">
          模型文件夹需包含 .model3.json 和对应资源。首次使用请安装此版本支持的 Cubism Core。
        </p>
        <details className="u2-disclosure">
          <summary>安装 Cubism Core</summary>
          <div className="space-y-2 pt-2">
            <p className="text-sm text-[var(--text-muted)]">请阅读 Live2D SDK 许可协议。已安装的 Core 存放在本机。</p>
            <a className="text-sm underline" target="_blank" rel="noreferrer" href="https://www.live2d.com/en/sdk/download/web/">查看 Live2D SDK 许可</a>
            <input ref={coreInput} type="file" accept=".js" hidden onChange={event => {
              const file = event.currentTarget.files?.[0];
              event.currentTarget.value = "";
              if (file) void installCore(file);
            }} />
            <div className="flex flex-wrap gap-2">
              <Button variant="secondary" size="sm" disabled={busy} onClick={() => void installCore()}>下载并安装官方 Core</Button>
              <Button variant="secondary" size="sm" disabled={busy} onClick={() => coreInput.current?.click()}>选取已下载的 SDK 文件</Button>
            </div>
          </div>
        </details>
        {status && <p role="status" className="text-sm">{status}</p>}
        {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      </fieldset>
      {model && (motionGroups.length > 0 || expressions.length > 0) && (
        <details className="u2-disclosure">
          <summary>对话状态与模型动作</summary>
          <div className="grid min-w-0 gap-3 pt-2">
            {Object.entries(PET_ACTIVITY_LABELS).map(([clip, label]) => (
              <fieldset key={clip} className="grid min-w-0 grid-cols-[repeat(auto-fit,minmax(min(100%,10rem),1fr))] gap-2">
                <legend className="text-sm">{label}</legend>
                {motionGroups.length > 0 && <label className="min-w-0 text-sm">动作组
                  <select className="ui-input block w-full" value={preferences.motionGroups?.[clip] ?? ""}
                    onChange={event => update({ motionGroups: { ...preferences.motionGroups, [clip]: event.target.value } })}>
                    <option value="">自动匹配</option>
                    {motionGroups.map(group => <option key={group} value={group}>{group}</option>)}
                  </select>
                </label>}
                {expressions.length > 0 && <label className="min-w-0 text-sm">表情
                  <select className="ui-input block w-full" value={preferences.expressions?.[clip] ?? ""}
                    onChange={event => update({ expressions: { ...preferences.expressions, [clip]: event.target.value } })}>
                    <option value="">模型默认</option>
                    {expressions.map(expression => <option key={expression.Name} value={expression.Name}>{expression.Name}</option>)}
                  </select>
                </label>}
              </fieldset>
            ))}
          </div>
        </details>
      )}
    </div>
  );
}
