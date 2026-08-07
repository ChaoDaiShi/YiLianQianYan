import { useState, useEffect, useCallback } from "react";
import { Cpu, HardDrive, Monitor, Disc, AlertTriangle, RefreshCw } from "lucide-react";
import { getSystemInfo } from "../api/client";
import { PageHeader, Panel, Button, Spinner, EmptyState } from "../components/ui";

interface SystemData {
  hostname: string;
  os: string;
  kernel: string;
  uptime_secs: number;
  cpu: { name: string; cores: number; usage_pct: string; per_core: string[] };
  memory: {
    total_gb: string;
    used_gb: string;
    usage_pct: string;
    swap_total_gb: string;
    swap_used_gb: string;
  };
  disks: {
    mount: string;
    name: string;
    total_gb: string;
    used_gb: string;
    available_gb: string;
    usage_pct: string;
  }[];
  gpu: { name: string; vram_gb: string; driver: string; resolution: string }[];
}

function UsageBar({ pct }: { pct: string }) {
  const v = Math.min(100, Math.max(0, parseFloat(pct) || 0));
  const color =
    v > 80 ? "bg-[var(--danger)]" : v > 50 ? "bg-[var(--warning)]" : "bg-[var(--success)]";
  return (
    <div className="w-full h-1.5 bg-[var(--panel-2)] rounded-full overflow-hidden border border-[var(--border)]">
      <div className={`h-full rounded-full transition-all duration-500 ${color}`} style={{ width: `${v}%` }} />
    </div>
  );
}

export default function SystemPage() {
  const [data, setData] = useState<SystemData | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    const res = await getSystemInfo();
    if (res) {
      setData(res);
      setError("");
    } else {
      setError("无法连接后端系统监控接口");
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 3000);
    return () => clearInterval(t);
  }, [refresh]);

  const fmtUptime = (s: number) => {
    const d = Math.floor(s / 86400);
    const h = Math.floor((s % 86400) / 3600);
    const m = Math.floor((s % 3600) / 60);
    return `${d > 0 ? `${d}天 ` : ""}${h}时${m}分`;
  };

  return (
    <div className="flex flex-col h-full">
      <PageHeader
        title="系统监控"
        description={
          data
            ? `${data.hostname} · ${data.os} · 运行 ${fmtUptime(data.uptime_secs)}`
            : "实时资源仪表"
        }
        actions={
          <Button variant="secondary" size="sm" onClick={refresh}>
            <RefreshCw className="w-3.5 h-3.5" />
            刷新
          </Button>
        }
      />

      <div className="flex-1 overflow-y-auto p-6 space-y-4 scrollbar-thin">
        {loading && !data && (
          <div className="flex justify-center py-20">
            <Spinner className="w-8 h-8" />
          </div>
        )}

        {error && !data && (
          <EmptyState
            icon={<AlertTriangle className="w-10 h-10 text-[var(--warning)]" />}
            title="监控不可用"
            description={error}
            action={
              <Button variant="secondary" onClick={refresh}>
                重试
              </Button>
            }
          />
        )}

        {data && (
          <>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <Panel>
                <div className="flex items-center justify-between mb-3">
                  <h3 className="font-medium flex items-center gap-2 font-mono text-sm">
                    <Cpu className="w-4 h-4 text-[var(--accent)]" />
                    CPU
                  </h3>
                  <span className="text-2xl font-bold font-mono text-[var(--accent)]">
                    {data.cpu.usage_pct}%
                  </span>
                </div>
                <UsageBar pct={data.cpu.usage_pct} />
                <p className="text-xs text-[var(--text-muted)] mt-2">{data.cpu.name}</p>
                <p className="text-xs text-[var(--text-faint)]">
                  {data.cpu.cores} 核心 · {data.cpu.per_core.length} 线程
                </p>
                <div className="flex flex-wrap gap-1 mt-2">
                  {data.cpu.per_core.slice(0, 16).map((u, i) => (
                    <span
                      key={i}
                      className="px-1.5 py-0.5 rounded text-[10px] font-mono bg-[var(--panel-2)] border border-[var(--border)] text-[var(--text-muted)]"
                    >
                      {u}%
                    </span>
                  ))}
                </div>
              </Panel>

              <Panel>
                <div className="flex items-center justify-between mb-3">
                  <h3 className="font-medium flex items-center gap-2 font-mono text-sm">
                    <HardDrive className="w-4 h-4 text-[var(--accent)]" />
                    内存
                  </h3>
                  <span className="text-2xl font-bold font-mono text-[var(--accent)]">
                    {data.memory.usage_pct}%
                  </span>
                </div>
                <UsageBar pct={data.memory.usage_pct} />
                <div className="flex justify-between text-xs text-[var(--text-muted)] mt-2 font-mono">
                  <span>已用 {data.memory.used_gb} GB</span>
                  <span>总量 {data.memory.total_gb} GB</span>
                </div>
              </Panel>
            </div>

            <Panel>
              <h3 className="font-medium mb-3 flex items-center gap-2 font-mono text-sm">
                <Monitor className="w-4 h-4 text-[var(--accent)]" />
                GPU
              </h3>
              <div className="space-y-2">
                {data.gpu.map((g, i) => (
                  <div
                    key={i}
                    className="flex items-center justify-between p-3 bg-[var(--panel-2)] rounded-lg border border-[var(--border)]"
                  >
                    <div>
                      <p className="font-medium text-sm">{g.name}</p>
                      <p className="text-xs text-[var(--text-muted)] font-mono">
                        显存 {g.vram_gb} GB · {g.driver} · {g.resolution}
                      </p>
                    </div>
                  </div>
                ))}
                {data.gpu.length === 0 && (
                  <p className="text-sm text-[var(--text-faint)]">未检测到 GPU</p>
                )}
              </div>
            </Panel>

            <Panel>
              <h3 className="font-medium mb-3 flex items-center gap-2 font-mono text-sm">
                <Disc className="w-4 h-4 text-[var(--accent)]" />
                磁盘
              </h3>
              <div className="space-y-3">
                {data.disks.map((d, i) => (
                  <div key={i}>
                    <div className="flex items-center justify-between mb-1">
                      <span className="text-sm font-medium">
                        {d.mount}{" "}
                        <span className="text-xs text-[var(--text-faint)]">({d.name})</span>
                      </span>
                      <span className="text-sm font-mono text-[var(--text-muted)]">
                        {d.used_gb} / {d.total_gb} GB
                      </span>
                    </div>
                    <UsageBar pct={d.usage_pct} />
                  </div>
                ))}
              </div>
            </Panel>
          </>
        )}
      </div>
    </div>
  );
}
