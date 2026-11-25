import { useEffect, useState } from "react";
import { managementRequest } from "../../../api/productManagement";
import { Button } from "../../../components/ui";
import SettingsDialogLayout from "./SettingsDialogLayout";
import "../../../styles/quickSetup.css";

type Mode = "auto" | "efficiency" | "balanced" | "performance";
interface Policy {
  revision: number;
  mode: Mode;
  cli_limit: number | null;
  background_interval_ms: number | null;
  memory_reserve_percent: number | null;
}
interface RuntimePolicy {
  policy: Policy;
  running: { cli_limit: number; delegation_limit: number };
  available_cli_slots: number;
  low_memory: boolean;
}
const path = "/api/v1/resources/policy";

export default function ResourcePolicyDialog({ onClose }: { onClose: () => void }) {
  const [saved, setSaved] = useState<RuntimePolicy | null>(null);
  const [draft, setDraft] = useState<Policy | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [load, setLoad] = useState(0);
  useEffect(() => {
    let active = true;
    void managementRequest<RuntimePolicy>("GET", path)
      .then((value) => {
        if (active) {
          setSaved(value);
          setDraft(value.policy);
          setError("");
        }
      })
      .catch((cause) => {
        if (active) setError(String(cause));
      });
    return () => {
      active = false;
    };
  }, [load]);
  async function save() {
    if (!draft) return;
    setBusy(true);
    try {
      const data = await managementRequest<RuntimePolicy>("PUT", path, draft);
      setSaved(data);
      setDraft(data.policy);
      setError("");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }
  const changed = Boolean(draft && saved && JSON.stringify(draft) !== JSON.stringify(saved.policy));
  return (
    <SettingsDialogLayout
      title="运行资源"
      onClose={onClose}
      unsaved={changed}
      closeDisabled={busy}
      error={error}
      action={
        <Button disabled={!changed || busy} onClick={() => void save()}>
          保存设置
        </Button>
      }
    >
      {error && (
        <Button variant="secondary" onClick={() => setLoad((n) => n + 1)}>
          重新读取
        </Button>
      )}
      {draft && saved ? (
        <fieldset disabled={busy} className="profile-step">
          <label>
            资源模式
            <select value={draft.mode} onChange={(e) => setDraft({ ...draft, mode: e.target.value as Mode })}>
              <option value="auto">根据设备自动选择</option>
              <option value="efficiency">节能</option>
              <option value="balanced">平衡</option>
              <option value="performance">性能优先</option>
            </select>
          </label>
          <label>
            外部 CLI 并发上限
            <input
              type="number"
              min="1"
              max="8"
              placeholder="跟随模式"
              value={draft.cli_limit ?? ""}
              onChange={(e) =>
                setDraft({ ...draft, cli_limit: e.target.value === "" ? null : Number(e.target.value) })
              }
            />
          </label>
          <label>
            开始降速的可用内存比例（%）
            <input type="number" min="1" max="45" placeholder="默认 10" value={draft.memory_reserve_percent ?? ""}
              onChange={e=>setDraft({...draft,memory_reserve_percent:e.target.value==="" ? null : Number(e.target.value)})}/>
          </label>
          <label>
            后台索引批次间隔（毫秒）
            <input
              type="number"
              min="0"
              max="5000"
              placeholder="跟随模式"
              value={draft.background_interval_ms ?? ""}
              onChange={(e) =>
                setDraft({
                  ...draft,
                  background_interval_ms: e.target.value === "" ? null : Number(e.target.value),
                })
              }
            />
          </label>
          <p>
            {saved.low_memory && "可用内存偏低，后台索引与桌宠动效正在降速。"}
            本次运行外部 CLI 可用名额 {saved.available_cli_slots} / {saved.running.cli_limit}
            。并发上限在下次启动时生效。
          </p>
        </fieldset>
      ) : (
        !error && <p role="status">正在读取资源配置…</p>
      )}
    </SettingsDialogLayout>
  );
}
