import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowLeft, ArrowRight, BookOpen, FileText, History, NotebookPen, Paperclip, Play, Plus, Square } from "lucide-react";
import { Badge, Button, EmptyState, Input, Modal, Textarea } from "../../components/ui";
import { getKnowledge, listKnowledge, type KnowledgeDetail, type KnowledgeDocument } from "../../api/knowledge";
import { answerLearning, cancelLearning, createLearning, getLearning, getLearningNote, listLearning, saveLearningNote, type LearningDetail, type LearningSession } from "./api";

const statusLabel: Record<LearningSession["status"], string> = { ready: "已生成", generating: "生成中", cancelled: "已停止", failed: "生成失败" };
type LearningTab = "sources" | "notes" | "exercises";

export default function WorkspaceLearning({ workspaceId }: { workspaceId: string }) {
  const [sessions, setSessions] = useState<LearningSession[]>([]);
  const [documents, setDocuments] = useState<KnowledgeDocument[]>([]);
  const [selectedDocuments, setSelectedDocuments] = useState<string[]>([]);
  const [title, setTitle] = useState("");
  const [selectedId, setSelectedId] = useState("");
  const [detail, setDetail] = useState<LearningDetail | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [choices, setChoices] = useState<Record<string, string>>({});
  const [tab, setTab] = useState<LearningTab>("sources");
  const [exerciseIndex, setExerciseIndex] = useState(0);
  const [createOpen, setCreateOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [history, setHistory] = useState<{ revision: number; content: string } | null>(null);
  const [source, setSource] = useState<KnowledgeDetail | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    setLoading(true); setSessions([]); setDetail(null); setSelectedId(""); setSelectedDocuments([]); setDrafts({}); setLoadError(null); setError(null);
    Promise.all([listLearning(workspaceId), listKnowledge()]).then(([learning, knowledge]) => {
      if (!active) return;
      setSessions(learning.sessions); setDocuments(knowledge.documents); setSelectedId(learning.sessions[0]?.id ?? "");
    }).catch((cause: unknown) => { if (active) setLoadError(String(cause)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [workspaceId]);

  useEffect(() => {
    if (!selectedId) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setDetail(null); setChoices({}); setError(null); setSource(null); setExerciseIndex(0); setHistory(null);
    const load = async () => {
      try {
        const result = await getLearning(workspaceId, selectedId);
        if (!active) return;
        setDetail(result);
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
  function updateDetail(next: LearningDetail) {
    setDetail(next);
    setSessions((items) => items.map((item) => item.id === next.session.id ? next.session : item));
  }
  const create = () => act("create", async () => {
    const result = await createLearning(workspaceId, title.trim(), selectedDocuments);
    setSessions((items) => [result.session, ...items]); setSelectedId(result.session.id); setCreateOpen(false); setTitle(""); setSelectedDocuments([]); setTab("notes");
  });
  const draft = drafts[selectedId] ?? detail?.content ?? "";
  const dirty = detail !== null && draft !== (detail.content ?? "");
  const save = () => act("note", async () => {
    if (!detail) return;
    await saveLearningNote(workspaceId, selectedId, draft, detail.session.revision);
    updateDetail(await getLearning(workspaceId, selectedId));
    setDrafts((current) => { const next = { ...current }; delete next[selectedId]; return next; });
  });
  const exercise = detail?.exercises[exerciseIndex];
  const attempts = detail?.answers.filter((answer) => answer.exercise_id === exercise?.id) ?? [];
  const latest = attempts[0];
  const submit = () => act("answer", async () => {
    if (!exercise) return;
    await answerLearning(workspaceId, selectedId, exercise.id, choices[exercise.id], crypto.randomUUID());
    updateDetail(await getLearning(workspaceId, selectedId));
  });

  return <div className="lc-workbench learning-workbench">
    <div className="lc-workbench-heading"><p>从资料开始，把理解留在笔记里。</p><Button size="sm" disabled={loading || Boolean(loadError)} onClick={() => { setError(null); setCreateOpen(true); }}><Plus size={16} />新建学习</Button></div>
    {loadError ? <p role="alert" className="lc-error">{loadError}</p> : null}
    {error && !createOpen && !historyOpen ? <p role="alert" className="lc-error">{error}</p> : null}
    <div className="lc-workbench-columns">
      <aside className="lc-records" aria-label="学习记录">
        <div className="lc-panel-heading"><span>资料学习</span><span>{sessions.length} 条记录</span></div>
        <div className="lc-records-scroll">
          {loading ? <p className="lc-muted lc-padding" role="status">正在读取学习记录…</p> : null}
          {!loading && !loadError && !sessions.length ? <div className="lc-padding"><EmptyState title="还没有学习记录" description="选择知识资料，生成第一份笔记与练习。" /></div> : null}
          {sessions.map((session) => <button key={session.id} type="button" className={`lc-record ${selectedId === session.id ? "is-selected" : ""}`} aria-pressed={selectedId === session.id} disabled={Boolean(pending)} onClick={() => setSelectedId(session.id)}>
            <span className="lc-record-icon"><FileText size={20} /></span><span className="lc-record-copy"><strong>{session.title}</strong><small>{session.sources.length} 份资料 · {statusLabel[session.status]}{drafts[session.id] !== undefined ? " · 有编辑" : ""}</small></span><ArrowRight size={15} />
          </button>)}
        </div>
        <div className="lc-records-foot"><BookOpen size={16} /><Link to="/knowledge">管理知识资料</Link></div>
      </aside>
      <section className="lc-detail" aria-label="学习工作区">
        <div className="lc-tabs" role="tablist" aria-label="学习内容">
          {([{ id: "sources", label: "资料", icon: FileText }, { id: "notes", label: "笔记", icon: NotebookPen }, { id: "exercises", label: "练习", icon: Play }] as const).map(({ id, label, icon: Icon }) => <button type="button" key={id} id={`learning-tab-${id}`} role="tab" aria-selected={tab === id} aria-controls="learning-panel" className={tab === id ? "is-selected" : ""} onClick={() => setTab(id)}><Icon size={18} />{label}</button>)}
        </div>
        <div className="lc-detail-scroll" id="learning-panel" role="tabpanel" aria-labelledby={`learning-tab-${tab}`}>
          {!selectedId && !loading && !loadError ? <EmptyState icon={<BookOpen size={30} />} title="留下一次完整的学习" description="在左侧选择一条记录，或新建资料学习。笔记版本和每次作答都会保存。" /> : null}
          {selectedId && !detail && !error ? <p role="status" className="lc-muted">正在读取学习内容…</p> : null}
          {detail ? <>
            <div className="lc-detail-title"><div><span className="lc-eyebrow">当前学习</span><h2>{detail.session.title}</h2></div><Badge tone={detail.session.status === "ready" ? "success" : detail.session.status === "failed" ? "danger" : "info"}>{statusLabel[detail.session.status]}</Badge></div>
            {detail.session.error ? <p className="lc-error" role="alert">{detail.session.error}</p> : null}
            {detail.session.status === "generating" ? <div className="lc-notice"><p role="status">正在根据选定资料生成笔记与客观练习。每份资料最多采用前 6000 字，完成后会显示真实结果。</p><Button size="sm" variant="secondary" disabled={Boolean(pending)} onClick={() => void act("cancel", async () => { await cancelLearning(workspaceId, selectedId); updateDetail(await getLearning(workspaceId, selectedId)); })}><Square size={14} />停止生成</Button></div> : null}
            {tab === "sources" ? <section className="lc-source-view">
              <p className="lc-muted">本次学习引用的资料。阅读原文时可核对模型生成的笔记与题目。</p>
              <div className="lc-source-list">{detail.session.sources.map((item) => <button className={`lc-source-button ${source?.document.id === item.document_id ? "is-selected" : ""}`} type="button" disabled={Boolean(pending)} key={item.document_id} onClick={() => void act("source", async () => setSource(await getKnowledge(item.document_id)))}><FileText size={18} /><span>{item.name}</span><span>阅读</span></button>)}</div>
              {pending === "source" ? <p role="status" className="lc-muted">正在读取资料…</p> : null}
              {source ? <article className="lc-source-text"><div className="lc-section-heading"><h3>{source.document.name}</h3><Link to={`/knowledge?document=${encodeURIComponent(source.document.id)}`}>在知识库打开</Link></div><p className="lc-plain-text">{source.text}</p></article> : null}
            </section> : null}
            {tab === "notes" && detail.session.status === "ready" ? <section className="lc-note-view">
              <div className="lc-section-heading"><h3>学习笔记 <span>v{detail.session.revision}</span></h3><Button size="sm" variant="ghost" onClick={() => { setHistory(null); setError(null); setHistoryOpen(true); }}><History size={15} />版本历史</Button></div>
              <Textarea label="笔记内容（Markdown）" disabled={Boolean(pending)} value={draft} onChange={(event) => setDrafts((current) => ({ ...current, [selectedId]: event.target.value }))} rows={16} />
              <div className="lc-editor-footer"><span>{dirty ? "有未保存编辑" : "当前版本已保存"} · {draft.length} 字符</span><Button size="sm" disabled={Boolean(pending) || !draft.trim() || !dirty} loading={pending === "note"} onClick={() => void save()}>保存新版本</Button></div>
              <p className="lc-muted">每次保存均新增资源版本，保留原笔记。模型生成内容需结合原文审阅。</p>
            </section> : null}
            {tab === "exercises" && detail.session.status === "ready" ? <section className="lc-practice-view">
              {exercise ? <article className="lc-practice-card">
                <p className="lc-eyebrow">练习 {exerciseIndex + 1} / {detail.exercises.length}</p><h3>{exercise.question}</h3>
                <fieldset className="lc-options" disabled={Boolean(pending)}><legend className="sr-only">选择答案</legend>{exercise.options.map((option) => <label key={option.id} className={choices[exercise.id] === option.id ? "is-selected" : ""}><input type="radio" name={exercise.id} checked={choices[exercise.id] === option.id} onChange={() => setChoices((current) => ({ ...current, [exercise.id]: option.id }))} /><span>{option.label}</span></label>)}</fieldset>
                <div className="lc-exercise-sources"><Paperclip size={15} /><span>来源：</span>{exercise.sources.map((item, index) => <Link key={index} to={`/knowledge?document=${encodeURIComponent(item.document_id)}`}>{detail.session.sources.find((value) => value.document_id === item.document_id)?.name ?? "查看原文"}</Link>)}</div>
                <div className="lc-editor-footer"><Button size="sm" disabled={Boolean(pending) || !choices[exercise.id]} loading={pending === "answer"} onClick={() => void submit()}>{latest ? "再次提交" : "提交答案"}</Button><div className="lc-pager"><Button size="sm" variant="ghost" aria-label="上一题" disabled={Boolean(pending) || exerciseIndex === 0} onClick={() => setExerciseIndex((index) => index - 1)}><ArrowLeft size={16} /></Button><Button size="sm" variant="ghost" aria-label="下一题" disabled={Boolean(pending) || exerciseIndex === detail.exercises.length - 1} onClick={() => setExerciseIndex((index) => index + 1)}><ArrowRight size={16} /></Button></div></div>
                {latest ? <div className="lc-answer-result"><strong>{latest.correct ? "回答正确" : "再回到资料想一想"} · {latest.score}/1 分</strong><p>正确选项：{latest.correct_option_id}。{latest.explanation}</p><p className="lc-muted">已保存 {attempts.length} 次作答：{attempts.map((answer) => `${answer.option_id}（${answer.score}/1）`).join("、")}</p></div> : null}
                <details className="lc-citation-details"><summary>阅读题目依据</summary>{exercise.sources.map((item, index) => <blockquote key={index}>{item.quote}</blockquote>)}</details>
              </article> : <EmptyState title="当前记录没有练习" description="生成的练习将在这里显示。" />}
              <p className="lc-muted">客观题按所选选项与已保存答案是否一致评分。题目及解析由模型生成，不代表权威题库。</p>
            </section> : null}
            {detail.session.status !== "ready" && detail.session.status !== "generating" && tab !== "sources" ? <EmptyState title={detail.session.status === "cancelled" ? "本次生成已停止" : "本次生成失败"} description="可在资料页核对来源，或新建一条学习记录。" /> : null}
          </> : null}
        </div>
      </section>
    </div>
    <Modal title="新建资料学习" size="md" open={createOpen} onClose={() => { if (!pending) setCreateOpen(false); }} footer={<><Button variant="secondary" disabled={Boolean(pending)} onClick={() => setCreateOpen(false)}>取消</Button><Button disabled={Boolean(pending) || !title.trim() || !selectedDocuments.length} loading={pending === "create"} onClick={() => void create()}><Play size={15} />生成笔记与练习</Button></>}>
      <div className="space-y-4"><Input label="学习主题" disabled={Boolean(pending)} value={title} maxLength={300} onChange={(event) => setTitle(event.target.value)} placeholder="这次希望理解什么？" />
        <fieldset className="lc-document-picker"><legend>选择 1–4 份知识资料</legend>{documents.map((document) => <label key={document.id}><input type="checkbox" checked={selectedDocuments.includes(document.id)} disabled={Boolean(pending) || (!selectedDocuments.includes(document.id) && selectedDocuments.length >= 4)} onChange={(event) => setSelectedDocuments((items) => event.target.checked ? [...items, document.id] : items.filter((id) => id !== document.id))} /><FileText size={17} /><span>{document.name}</span></label>)}</fieldset>
        {!documents.length ? <p className="lc-muted">还没有知识资料。<Link className="lc-text-link" to="/knowledge">先导入资料</Link></p> : null}<p className="lc-muted">将使用当前对话模型生成内容，每份资料本次最多采用前 6000 字。关闭此窗口不取消已创建的生成任务。</p>{error ? <p role="alert" className="lc-error">{error}</p> : null}</div>
    </Modal>
    <Modal title="笔记版本历史" size="lg" open={historyOpen} onClose={() => { if (!pending) setHistoryOpen(false); }} footer={<><Button variant="secondary" onClick={() => setHistoryOpen(false)}>关闭</Button>{history ? <Button disabled={Boolean(pending)} onClick={() => { setDrafts((current) => ({ ...current, [selectedId]: history.content })); setHistoryOpen(false); setTab("notes"); }}>载入为编辑草稿</Button> : null}</>}>
      <div className="lc-history-choices">{detail?.notes.map((note) => <Button key={note.revision} size="sm" variant={history?.revision === note.revision ? "primary" : "secondary"} disabled={Boolean(pending)} onClick={() => void act("history", async () => { const old = await getLearningNote(workspaceId, selectedId, note.revision); setHistory({ revision: note.revision, content: old.content }); })}>v{note.revision} · {new Date(note.created_at).toLocaleDateString()}</Button>)}</div>
      {pending === "history" ? <p role="status" className="lc-muted">正在读取历史笔记…</p> : null}{history ? <pre className="lc-history-text">{history.content}</pre> : <p className="lc-muted">选择版本查看原笔记；载入草稿后，点击“保存新版本”才会写入。</p>}{error ? <p role="alert" className="lc-error">{error}</p> : null}
    </Modal>
  </div>;
}
