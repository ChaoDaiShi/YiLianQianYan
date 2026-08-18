import { useCallback, useEffect, useState } from "react";
import { Activity, Cpu, Database, Disc, HardDrive, Monitor, RefreshCw, Server } from "lucide-react";
import { getSystemInfo, healthCheck, type RuntimeHealth } from "../api/client";
import {
  Button,
  ErrorState,
  PageHeader,
  Panel,
  Skeleton,
} from "../components/ui";
import {
  formatLogTimestamp,
  SystemMetricCard,
  SystemSection,
  SystemStatusBadge,
} from "../components/system";

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

function formatUptime(seconds: number) {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${days > 0 ? `${days}天 ` : ""}${hours}时${minutes}分`;
}

function percent(value: string) {
  return Math.min(100, Math.max(0, Number.parseFloat(value) || 0));
}

function SystemMetricSkeleton() {
  return <Skeleton className="h-32 w-full" />;
}

export default function SystemPage() {
  const [data, setData] = useState<SystemData | null>(null);
  const [health, setHealth] = useState<RuntimeHealth | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [lastRefresh, setLastRefresh] = useState<number | null>(null);

  const refresh = useCallback(async () => {
    const [systemResult, healthResult] = await Promise.all([getSystemInfo(), healthCheck()]);
    if (systemResult) {
      setData(systemResult as SystemData);
      setError("");
    } else {
      setError("状态暂时无法获取");
    }
    setHealth(healthResult);
    setLastRefresh(Date.now());
    setLoading(false);
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 3000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  return (
    <div className="system-center-page flex h-full min-h-0 flex-col">
      <PageHeader
        title="系统监控"
        description="查看应用、服务和资源的当前运行状态。"
        actions={(
          <Button variant="secondary" size="sm" onClick={() => void refresh()} aria-label="刷新系统状态">
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
            刷新
          </Button>
        )}
      />

      <div className="system-center-scroll min-h-0 flex-1 overflow-y-auto px-4 pb-6 pt-4 scrollbar-thin">
        {error && !data ? (
          <ErrorState
            title="系统状态暂时无法加载"
            description={error}
            action={<Button variant="secondary" onClick={() => void refresh()}>重试</Button>}
          />
        ) : null}

        {loading && !data ? (
          <div className="system-monitor-stack" aria-label="正在加载系统状态">
            <SystemSection title="系统资源">
              <div className="system-monitor-grid">
                <SystemMetricSkeleton />
                <SystemMetricSkeleton />
              </div>
            </SystemSection>
            <SystemSection title="服务状态">
              <Skeleton className="h-24 w-full" />
            </SystemSection>
          </div>
        ) : null}

        {data ? (
          <div className="system-monitor-stack">
            <SystemSection title="系统资源" description="当前设备返回的实时资源数据。">
              <div className="system-monitor-grid">
                <SystemMetricCard
                  label="CPU"
                  value={`${data.cpu.usage_pct}%`}
                  detail={`${data.cpu.name} · ${data.cpu.cores} 核心 · ${data.cpu.per_core.length} 线程`}
                  icon={<Cpu className="h-4 w-4" />}
                  barPercent={percent(data.cpu.usage_pct)}
                />
                <SystemMetricCard
                  label="Memory"
                  value={`${data.memory.usage_pct}%`}
                  detail={`已用 ${data.memory.used_gb} GB / 总量 ${data.memory.total_gb} GB`}
                  icon={<HardDrive className="h-4 w-4" />}
                  barPercent={percent(data.memory.usage_pct)}
                />
              </div>
            </SystemSection>

            {data.disks.length > 0 ? (
              <SystemSection title="磁盘">
                <Panel className="system-disk-list">
                  {data.disks.map((disk) => (
                    <div key={`${disk.mount}-${disk.name}`} className="system-disk-row">
                      <div className="flex min-w-0 items-center gap-2">
                        <Disc className="h-4 w-4 shrink-0 text-[var(--accent-primary)]" aria-hidden="true" />
                        <div className="min-w-0">
                          <div className="truncate text-sm font-medium text-[var(--text-primary)]">{disk.mount} · {disk.name}</div>
                          <div className="text-xs text-[var(--text-secondary)]">可用 {disk.available_gb} GB</div>
                        </div>
                      </div>
                      <div className="flex min-w-[150px] items-center gap-3">
                        <div className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-[var(--surface-muted)]" aria-hidden="true">
                          <div className="h-full rounded-full bg-[var(--accent-purple)]" style={{ width: `${percent(disk.usage_pct)}%` }} />
                        </div>
                        <span className="w-20 text-right text-xs text-[var(--text-secondary)]">{disk.used_gb} / {disk.total_gb} GB</span>
                      </div>
                    </div>
                  ))}
                </Panel>
              </SystemSection>
            ) : null}

            {data.gpu.length > 0 ? (
              <SystemSection title="GPU">
                <Panel className="system-gpu-list">
                  {data.gpu.map((gpu) => (
                    <div key={`${gpu.name}-${gpu.driver}`} className="system-gpu-row">
                      <Monitor className="h-4 w-4 shrink-0 text-[var(--accent-blue)]" aria-hidden="true" />
                      <div className="min-w-0">
                        <div className="truncate text-sm font-medium text-[var(--text-primary)]">{gpu.name}</div>
                        <div className="truncate text-xs text-[var(--text-secondary)]">显存 {gpu.vram_gb} GB · {gpu.driver} · {gpu.resolution}</div>
                      </div>
                    </div>
                  ))}
                </Panel>
              </SystemSection>
            ) : null}

            <SystemSection title="服务状态" description="仅显示已有健康接口返回的状态。">
              <Panel className="system-health-grid">
                <div className="system-health-row">
                  <div className="flex items-center gap-2"><Server className="h-4 w-4 text-[var(--accent-blue)]" aria-hidden="true" />Backend</div>
                  <SystemStatusBadge status={health?.status} label="Backend" />
                </div>
                <div className="system-health-row">
                  <div className="flex items-center gap-2"><Database className="h-4 w-4 text-[var(--accent-purple)]" aria-hidden="true" />Database</div>
                  <SystemStatusBadge status={health?.database} label="Database" />
                </div>
                {health?.version ? <div className="system-health-meta"><span>Version</span><span>{health.version}</span></div> : null}
                {health?.policy_version ? <div className="system-health-meta"><span>Policy</span><span>{health.policy_version}</span></div> : null}
              </Panel>
            </SystemSection>

            <SystemSection title="详细信息">
              <Panel className="system-detail-grid">
                <div><span>主机</span><strong>{data.hostname}</strong></div>
                <div><span>操作系统</span><strong>{data.os}</strong></div>
                <div><span>内核</span><strong>{data.kernel}</strong></div>
                <div><span>运行时间</span><strong>{formatUptime(data.uptime_secs)}</strong></div>
                {lastRefresh ? <div><span>最近刷新</span><strong>{formatLogTimestamp(lastRefresh)}</strong></div> : null}
                <div className="system-detail-note"><Activity className="h-4 w-4" aria-hidden="true" />数据来自现有系统接口</div>
              </Panel>
            </SystemSection>
          </div>
        ) : null}
      </div>
    </div>
  );
}
