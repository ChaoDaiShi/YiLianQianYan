import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Badge, Button, EmptyState, Input, Textarea } from "../../components/ui";
import { listKnowledge, type KnowledgeDocument } from "../../api/knowledge";
import { answerLearning, cancelLearning, createLearning, getLearning, getLearningNote, listLearning, saveLearningNote, type LearningDetail, type LearningSession } from "./api";

export default function WorkspaceLearning({ workspaceId }: { workspaceId: string }) {
  const [sessions, setSessions] = useState<LearningSession[]>([]);
  const [documents, setDocuments] = useState<KnowledgeDocument[]>([]);
  const [selectedDocuments, setSelectedDocuments] = useState<string[]>([]);
  const [title, setTitle] = useState("");
  const [selectedId, setSelectedId] = useState("");
  const [detail, setDetail] = useState<LearningDetail | null>(null);
  const [draft, setDraft] = useState("");
  const [choices, setChoices] = useState<Record<string, string>>({});
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    setSessions([]); setDetail(null); setSelectedId(""); setSelectedDocuments([]); setError(null);
    Promise.all([listLearning(workspaceId), listKnowledge()]).then(([learning, knowledge]) => {
      if (!active) return;
      setSessions(learning.sessions); setDocuments(knowledge.documents); setSelectedId(learning.sessions[0]?.id ?? "");
    }).catch((cause: unknown) => { if (active) setError(String(cause)); });
    return () => { active = false; };
  }, [workspaceId]);

  useEffect(() => {
    if (!selectedId) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setDetail(null); setChoices({}); setError(null);
    const load = async () => {
      try {
        const result = await getLearning(workspaceId, selectedId);
        if (!active) return;
        setDetail(result); setDraft(result.content ?? "");
        setSessions((items) => items.map((item) => item.id === result.session.id ? result.session : item));
        if (result.session.status === "generating") timer = setTimeout(() => void load(), 1800);
      } catch (cause) { if (active) setError(cause instanceof Error ? cause.message : String(cause)); }
    };
    void load();
    return () => { active = false; if (timer) clearTimeout(timer); };
  }, [workspaceId, selectedId]);

  async function act(key: string, operation: () => Promise<void>) {
    if (pending) return;
    setPending(key); setError(null);
    try { await operation(); } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setPending(null); }
  }
  const create = () => act("create", async () => {
    const result = await createLearning(workspaceId, title.trim(), selectedDocuments);
    setSessions((items) => [result.session, ...items]); setSelectedId(result.session.id);
  });
  const save = () => act("note", async () => {
    if (!detail) return;
    await saveLearningNote(workspaceId, selectedId, draft, detail.session.revision);
    const next = await getLearning(workspaceId, selectedId); setDetail(next);
    setSessions((items) => items.map((item) => item.id === next.session.id ? next.session : item));
  });
  const submit = (exercise: string) => act(exercise, async () => {
    await answerLearning(workspaceId, selectedId, exercise, choices[exercise], crypto.randomUUID());
    setDetail(await getLearning(workspaceId, selectedId));
  });

  return <div className="space-y-5">
    <p className="text-xs leading-6 text-[var(--text-secondary)]">学习记录保存在当前项目。选择已有知识资料后生成笔记与客观练习；每份资料本次最多采用前 6000 字。引文可回到原文核对，模型生成内容仍需审阅。</p>
    {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
    <details className="rounded-[var(--radius-md)] border border-[var(--border-soft)] p-3" open={!sessions.length}>
      <summary className="cursor-pointer text-sm font-medium">新建资料学习</summary>
      <div className="mt-3 space-y-3">
        <Input label="学习主题" value={title} onChange={(event) => setTitle(event.target.value)} placeholder="希望掌握哪些内容" />
        <fieldset className="max-h-40 space-y-2 overflow-auto"><legend className="mb-2 text-xs">选择 1–4 份资料</legend>
          {documents.map((document) => <label key={document.id} className="flex gap-2 text-sm"><input type="checkbox" checked={selectedDocuments.includes(document.id)} disabled={!selectedDocuments.includes(document.id) && selectedDocuments.length >= 4} onChange={(event) => setSelectedDocuments((items) => event.target.checked ? [...items, document.id] : items.filter((id) => id !== document.id))} />{document.name}</label>)}
          {!documents.length && <p className="text-xs">还没有知识资料。<Link className="underline text-[var(--accent-primary)]" to="/knowledge">先导入资料</Link></p>}
        </fieldset>
        <Button size="sm" disabled={Boolean(pending) || !title.trim() || !selectedDocuments.length} onClick={() => void create()}>生成笔记与练习</Button>
      </div>
    </details>
    {!!sessions.length && <label className="block text-xs">学习记录<select className="mt-1 w-full rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2 text-sm" value={selectedId} disabled={Boolean(pending)} onChange={(event) => setSelectedId(event.target.value)}>{sessions.map((session) => <option key={session.id} value={session.id}>{session.title} · {session.status === "ready" ? "已生成" : session.status === "generating" ? "生成中" : session.status === "cancelled" ? "已停止" : "生成失败"}</option>)}</select></label>}
    {!sessions.length && <EmptyState title="还没有学习记录" description="选定资料并生成后，笔记、练习和每次作答都会保留。" />}
    {detail && <>
      <div className="flex flex-wrap items-center gap-2"><Badge tone={detail.session.status === "ready" ? "success" : detail.session.status === "failed" ? "danger" : "info"}>{detail.session.status === "ready" ? "笔记与练习已保存" : detail.session.status === "generating" ? "正在根据资料生成…" : detail.session.status === "cancelled" ? "已停止" : "生成失败"}</Badge>{detail.session.sources.map((source) => <Link key={source.document_id} className="text-xs text-[var(--accent-primary)] underline" to={`/knowledge?document=${encodeURIComponent(source.document_id)}`}>{source.name}</Link>)}</div>
      {detail.session.status === "generating" && <Button size="sm" variant="secondary" disabled={Boolean(pending)} onClick={() => void act("cancel", async () => { await cancelLearning(workspaceId, selectedId); setDetail(await getLearning(workspaceId, selectedId)); })}>停止生成</Button>}
      {detail.session.error && <p role="alert" className="text-sm text-[var(--danger)]">{detail.session.error}</p>}
      {detail.session.status === "ready" && <>
        <section className="space-y-3" aria-label="学习笔记">
          <div className="flex items-center justify-between"><h3 className="text-sm font-medium">笔记 · v{detail.session.revision}</h3><select aria-label="阅读历史笔记" className="rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-1 text-xs" defaultValue="" disabled={Boolean(pending)} onChange={(event) => { const revision = Number(event.target.value); if (revision) void act("history", async () => { const old = await getLearningNote(workspaceId, selectedId, revision); setDraft(old.content); }); }}><option value="">读取历史版本…</option>{detail.notes.map((note) => <option key={note.revision} value={note.revision}>v{note.revision}</option>)}</select></div>
          <Textarea label="可编辑笔记" value={draft} onChange={(event) => setDraft(event.target.value)} rows={12} />
          <div className="flex items-center gap-3"><Button size="sm" disabled={Boolean(pending) || !draft.trim()} onClick={() => void save()}>保存为新版本</Button><span className="text-xs text-[var(--text-secondary)]">编辑和恢复历史均新增版本，保留原笔记。</span></div>
        </section>
        <section className="space-y-3" aria-label="资料练习"><h3 className="text-sm font-medium">客观练习</h3><p className="text-xs text-[var(--text-secondary)]">评分规则：所选选项与保存答案一致得 1 分，否则得 0 分。题目与解析由模型生成，不代表权威题库。</p>
          {detail.exercises.map((exercise, index) => {
            const attempts = detail.answers.filter((answer) => answer.exercise_id === exercise.id);
            const latest = attempts[0];
            return <article key={exercise.id} className="space-y-3 rounded-[var(--radius-md)] border border-[var(--border-soft)] p-3">
              <h4 className="text-sm font-medium">{index + 1}. {exercise.question}</h4>
              <fieldset className="space-y-2" disabled={Boolean(pending)}><legend className="sr-only">选择答案</legend>{exercise.options.map((option) => <label key={option.id} className="flex gap-2 text-sm"><input type="radio" name={exercise.id} checked={choices[exercise.id] === option.id} onChange={() => setChoices((current) => ({ ...current, [exercise.id]: option.id }))} />{option.label}</label>)}</fieldset>
              <Button size="sm" variant="secondary" disabled={Boolean(pending) || !choices[exercise.id]} onClick={() => void submit(exercise.id)}>{latest ? "再次作答" : "提交答案"}</Button>
              {latest && <div className="rounded bg-[var(--surface-muted)] p-3 text-sm"><p className="font-medium">确定性评分：{latest.score}/1 · 正确选项 {latest.correct_option_id}</p><p className="mt-1">资料解析：{latest.explanation}</p><p className="mt-2 text-xs text-[var(--text-secondary)]">已保存 {attempts.length} 次作答：{attempts.map((answer) => `${answer.option_id} (${answer.score}/1)`).join("、")}</p></div>}
              <details className="text-xs text-[var(--text-secondary)]"><summary>阅读题目依据</summary>{exercise.sources.map((source, sourceIndex) => <blockquote key={sourceIndex} className="mt-2 border-l-2 border-[var(--border-soft)] pl-3"><Link className="underline" to={`/knowledge?document=${encodeURIComponent(source.document_id)}`}>查看原文</Link><p className="mt-1">{source.quote}</p></blockquote>)}</details>
            </article>;
          })}
        </section>
      </>}
    </>}
  </div>;
}
