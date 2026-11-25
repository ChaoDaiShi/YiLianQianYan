import type { WatchSchedule } from "../../api/automations";
import { Input, Select } from "../../components/ui";

export function localDateTime(timestamp = Date.now() + 3_600_000) {
  if (!Number.isFinite(timestamp)) return "";
  const date = new Date(timestamp);
  const two = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())}T${two(date.getHours())}:${two(date.getMinutes())}`;
}

export function defaultAttentionSchedule(): WatchSchedule {
  return {
    kind: "watch",
    source: { kind: "http", url: "", send_preview: false },
    min_interval_secs: 60,
    expires_at: Date.now() + 3_600_000,
    max_checks: 20,
    max_retries: 3,
  };
}

export default function AttentionFields({ schedule, onChange }: {
  schedule: WatchSchedule;
  onChange: (value: WatchSchedule) => void;
}) {
  const { source } = schedule;
  return <div className="min-w-0 space-y-3 border-l border-[var(--border-soft)] pl-3">
    <Select label="关注来源" value={source.kind} options={[
      { value: "http", label: "网址" }, { value: "file", label: "本地文件" },
    ]} onChange={event => onChange({
      ...schedule,
      source: event.target.value === "file" ? { kind: "file", path: "" } : { kind: "http", url: "", send_preview: false },
    })} />
    {source.kind === "file" ? <Input label="关注文件绝对路径" value={source.path}
      onChange={event => onChange({ ...schedule, source: { ...source, path: event.target.value } })}
      hint="使用授权目录内不超过 1 MiB 的文件。模型判断会收到文件路径、时间和内容指纹。" />
      : <>
          <Input label="关注网址" type="url" value={source.url}
            onChange={event => onChange({ ...schedule, source: { ...source, url: event.target.value } })}
            hint="按保存的网址发起 GET，继续使用当前网络读取权限。" />
          <label className="flex items-start gap-2 text-sm">
            <input className="mt-1" type="checkbox" checked={source.send_preview}
              onChange={event => onChange({ ...schedule, source: { ...source, send_preview: event.target.checked } })} />
            <span>允许向当前模型发送最多 2,048 字符的网页响应摘录</span>
          </label>
        </>}
    <Input label="关注截止时间（此设备时区）" type="datetime-local" value={localDateTime(schedule.expires_at)}
      onChange={event => onChange({ ...schedule, expires_at: new Date(event.target.value).getTime() })} />
    <div className="grid min-w-0 gap-2 sm:grid-cols-3">
      <Input label="最短间隔（秒）" type="number" min={1} value={schedule.min_interval_secs}
        onChange={event => onChange({ ...schedule, min_interval_secs: Number(event.target.value) })} />
      <Input label="检查次数上限" type="number" min={1} value={schedule.max_checks}
        onChange={event => onChange({ ...schedule, max_checks: Number(event.target.value) })} />
      <Input label="失败重试上限" type="number" min={0} value={schedule.max_retries}
        onChange={event => onChange({ ...schedule, max_retries: Number(event.target.value) })} />
    </div>
    <p className="text-xs text-[var(--text-secondary)]">模型决定实际复查时间，并可推进已保存目标。新目标提议由你选择后进入任务规划。</p>
  </div>;
}
