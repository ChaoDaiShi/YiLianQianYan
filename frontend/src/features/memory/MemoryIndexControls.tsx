import { useEffect, useState } from "react";
import { Button } from "../../components/ui";
import { memoryIndexJob, type MemoryIndexJob } from "../../api/memory";

export function MemoryIndexControls() {
  const [job, setJob] = useState<MemoryIndexJob>();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const running = job?.phase === "running";

  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const read = async () => {
      try {
        const next = await memoryIndexJob();
        if (!active) return;
        setJob(next);
        if (next.phase === "running") timer = setTimeout(read, 1000);
      } catch {
        if (active && running) timer = setTimeout(read, 1500);
      }
    };
    void read();
    return () => {
      active = false;
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [running]);

  const change = async () => {
    setPending(true);
    setError("");
    try {
      setJob(await memoryIndexJob(running ? "DELETE" : "POST"));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setPending(false);
    }
  };

  return <div className="memory-page-index-maintenance">
    <Button variant="secondary" size="sm" disabled={pending} onClick={() => void change()}>
      {running ? "暂停重建" : job?.phase === "cancelled" ? "继续重建索引" : "重建索引"}
    </Button>
    {job?.phase === "running" && <p role="status">后台重建中，已处理 {job.indexed} 条。</p>}
    {job?.phase === "completed" && <p role="status">重建完成，本次更新 {job.indexed} 条。</p>}
    {job?.phase === "cancelled" && <p role="status">已暂停，保留本次生成的 {job.indexed} 条索引。</p>}
    {(error || job?.phase === "failed") && <p role="alert">{error || job?.error}</p>}
  </div>;
}
