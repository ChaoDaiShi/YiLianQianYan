import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { listCreations } from "../creation/api";
import { listLearning } from "../learning/api";
import { formatTaskDate } from "../tasks/taskPresentation";

interface StudyOutput { id: string; kind: "learning" | "creation"; title: string; revision: number; updated_at: number }

export default function WorkspaceStudyOutputs({ workspaceId }: { workspaceId: string }) {
  const [items, setItems] = useState<StudyOutput[] | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  useEffect(() => {
    let active = true;
    setItems(null); setErrors([]);
    void Promise.allSettled([listLearning(workspaceId), listCreations(workspaceId)]).then(([learning, creations]) => {
      if (!active) return;
      const next: StudyOutput[] = [];
      if (learning.status === "fulfilled") next.push(...learning.value.sessions.filter(item => item.revision > 0).map(item => ({ ...item, kind: "learning" as const })));
      if (creations.status === "fulfilled") next.push(...creations.value.creations.map(item => ({ ...item, kind: "creation" as const })));
      setItems(next.sort((left, right) => right.updated_at - left.updated_at));
      setErrors([
        ...(learning.status === "rejected" ? [`学习笔记读取失败：${String(learning.reason)}`] : []),
        ...(creations.status === "rejected" ? [`创作文稿读取失败：${String(creations.reason)}`] : []),
      ]);
    });
    return () => { active = false; };
  }, [workspaceId]);

  if (!items) return <p className="workspace-tree-note" role="status">正在读取学习与创作成果…</p>;
  if (!items.length && !errors.length) return null;
  const root = `/workspaces/${encodeURIComponent(workspaceId)}`;
  return <section className="mb-5" aria-label="学习与创作成果">
    <div className="workspace-section-heading"><h2>学习与创作</h2></div>
    {errors.map(error => <p key={error} className="lc-error mb-2" role="alert">{error}</p>)}
    {items.length > 0 && <div className="lc-output-grid">
      {items.slice(0, 8).map(item => {
        const learning = item.kind === "learning";
        const href = `${root}/${item.kind}?${new URLSearchParams({ [learning ? "session" : "creation"]: item.id })}`;
        const titleId = `study-output-${item.kind}-${item.id}`;
        return <Link key={`${item.kind}:${item.id}`} to={href} className="lc-output-card" data-kind={item.kind} aria-labelledby={titleId}>
          <h3 id={titleId}>{item.title}</h3>
          <p>{learning ? "学习笔记" : "创作文稿"} · v{item.revision} · {formatTaskDate(item.updated_at)}</p>
        </Link>;
      })}
    </div>}
    {items.length > 8 && <p className="workspace-tree-note">显示最近 8 项，可在「资料学习」或「创作」查看其余记录。</p>}
  </section>;
}
