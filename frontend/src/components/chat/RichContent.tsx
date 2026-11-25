import { useEffect, useMemo, useState, type ComponentProps, type FormEvent } from "react";
import { Link } from "react-router-dom";
import ConversationMarkdown from "./ConversationMarkdown";
import { Button, Input, Modal, Select } from "../ui";
import { ResourceCard } from "../../features/resources/ResourceCard";
import { postRichAction, readRichMessage, type AppliedPage, type RichAction, type RichBlock, type RichComponentState, type RichMessage } from "../../api/richContent";
import { loadPage, workspacePagePath, type PageDocument } from "../../api/workspacePages";
import "./richContent.css";

type MarkdownSources = ComponentProps<typeof ConversationMarkdown>["sources"];
type ActionHandler = (id: string, version: number, action: RichAction, values?: object) => ReturnType<typeof postRichAction>;

const richFence = /^ {0,3}(?:`{3,}|~{3,})\s*(?:yilian-rich|ag-ui|a2ui)[^\S\n]*$/m;
export const hasRichFence = (text: string) => richFence.test(text);

export function streamingRichMarkdown(text: string) {
  const fence = richFence.exec(text);
  return fence ? text.slice(0, fence.index).trimEnd() : text;
}

export function RichReply({ conversation, message, text, sources }: { conversation: string; message: string; text: string; sources?: MarkdownSources }) {
  const [response, setResponse] = useState<RichMessage | null>(null);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let current = true;
    void readRichMessage(conversation, message).then(result => {
      if (!current) return;
      if (result.ok) { setResponse(result.data); setError(""); }
      else setError(result.error);
    });
    return () => { current = false; };
  }, [attempt, conversation, message, text]);

  const onAction: ActionHandler = async (id, stateVersion, action, values) => {
    const result = await postRichAction(conversation, message, id, stateVersion, action, values);
    if (result.ok && "state_version" in result.data) {
      const saved = result.data;
      setResponse(current => current && ({ ...current, states: { ...current.states, [id]: saved } }));
    }
    return result;
  };

  if (!response) {
    if (error) return <>
      <ConversationMarkdown text={text} sources={sources} />
      <p role="alert" className="rich-component-error">{error}</p>
      <Button size="sm" variant="secondary" onClick={() => setAttempt(value => value + 1)}>重新加载组件</Button>
    </>;
    return <>
      <ConversationMarkdown text={streamingRichMarkdown(text)} sources={sources} />
      <p role="status" className="rich-component-note">正在读取回答组件…</p>
    </>;
  }

  return <RichMessageParts response={response} sources={sources} onAction={onAction} onReload={() => setAttempt(value => value + 1)} />;
}

export function RichMessageParts({ response, sources, onAction, onReload }: {
  response: RichMessage; sources?: MarkdownSources; onAction?: ActionHandler; onReload?: () => void;
}) {
  return <div className="rich-message">{response.parts.map((part, index) =>
    part.type === "markdown"
      ? <ConversationMarkdown key={index} text={part.text} sources={sources} />
      : <div className="rich-message-blocks" key={index}>{part.blocks.map(block =>
        <RichComponent key={block.id} block={block} saved={response.states[block.id]} onAction={onAction} onReload={onReload} />
      )}</div>
  )}</div>;
}

function RichComponent({ block, saved, onAction, onReload }: {
  block: RichBlock; saved?: RichComponentState; onAction?: ActionHandler; onReload?: () => void;
}) {
  const [status, setStatus] = useState<"idle" | "sending" | "saved" | "error">("idle");
  const [error, setError] = useState("");
  const [conflict, setConflict] = useState(false);
  const [applied, setApplied] = useState<PageDocument | null>(null);
  const editable = Boolean(onAction);
  const pending = status === "sending";

  const perform = async (action: RichAction, values?: object) => {
    if (!onAction || pending || !block.available_actions.includes(action)) return false;
    setStatus("sending");
    setError("");
    setConflict(false);
    const result = await onAction(block.id, saved?.state_version ?? 0, action, values);
    if (!result.ok) {
      setStatus("error");
      setError(result.error);
      setConflict(result.status === 409);
      return false;
    }
    if ("page" in result.data) setApplied(result.data.page);
    setStatus("saved");
    return true;
  };

  if (block.type === "text") return <ConversationMarkdown text={block.text} />;
  if (block.type === "math") return <p className="rich-math" aria-label="数学公式">{block.expression}</p>;
  if (block.type === "code") return <div className="rich-component"><strong>{block.language}</strong><pre><code>{block.code}</code></pre></div>;
  if (block.type === "image" || block.type === "file" || block.type === "artifact") return <ResourceCard file={block.resource} />;
  if (block.type === "unavailable") return <p className="rich-component-note">{block.message}</p>;
  if (block.type === "task_status") return <section className="rich-component">
    <header><strong>{block.title}</strong><span className="rich-component-note">{taskStatus(block.status)}</span></header>
    <Link to={"/workspaces/" + encodeURIComponent(block.workspace_id) + "?tab=tasks&task=" + encodeURIComponent(block.task_id)}>查看任务详情</Link>
  </section>;

  return <section className="rich-component">
    {block.type === "chart" && <Chart block={block} selected={typeof saved?.state.selected === "number" ? saved.state.selected : undefined}
      onSelect={editable && !pending ? selected => void perform("select", { selected }) : undefined} />}
    {block.type === "table" && <Table block={block} sort={saved?.state}
      onSort={editable && !pending ? (column, descending) => void perform("sort", { column, descending }) : undefined} />}
    {block.type === "form" && <Form block={block} saved={saved} editable={editable} busy={pending} onSubmit={values => void perform("submit", values)} />}
    {block.type === "interactive_card" && <>
      <strong>{block.title}</strong><p>{block.text}</p>
      {saved?.state.acknowledged ? <p className="rich-component-note">已查看</p> : editable &&
        <Button size="sm" variant="secondary" disabled={pending} onClick={() => void perform("acknowledge")}>{block.action_label}</Button>}
    </>}
    {block.type === "page_proposal" && <PageProposal block={block} applied={applied ?? block.applied_page ?? null} busy={pending} error={error} editable={editable}
      onApply={() => perform("apply_page_proposal")} />}
    {block.type === "page_create_proposal" && <PageCreateProposal block={block} applied={applied ?? block.applied_page ?? null} busy={pending} error={error} editable={editable}
      onApply={() => perform("apply_page_proposal")} />}
    {pending && <p role="status" className="rich-component-note">正在提交…</p>}
    {status === "saved" && !applied && <p role="status" className="rich-component-note">状态已保存</p>}
    {error && <p role="alert" className="rich-component-error">{error}</p>}
    {conflict && block.type !== "page_proposal" && <Button size="sm" variant="secondary" onClick={onReload}>读取最新状态</Button>}
  </section>;
}

function taskStatus(status: string) {
  const labels: Record<string, string> = {
    draft: "草稿", ready: "待执行", running: "执行中", waiting_approval: "等待审批",
    waiting_user: "等待输入", completed: "已完成", blocked: "已阻塞", cancelled: "已取消", failed: "失败",
  };
  return labels[status] ?? status;
}

function Chart({ block, selected, onSelect }: { block: Extract<RichBlock, { type: "chart" }>; selected?: number; onSelect?: (index: number) => void }) {
  const low = Math.min(0, ...block.values);
  const high = Math.max(0, ...block.values);
  const span = high - low || 1;
  const plotWidth = 560;
  const step = plotWidth / block.values.length;
  const x = (index: number) => 20 + step * (index + 0.5);
  const y = (value: number) => 155 - (value - low) / span * 136;
  return <figure className="rich-chart">
    <figcaption>{block.title}</figcaption>
    <svg viewBox="0 0 600 175" role="img" aria-label={block.title + "图形"}>
      <line x1="20" x2="580" y1={y(0)} y2={y(0)} stroke="var(--border-strong, var(--border-soft))" />
      {block.kind === "line" && <polyline points={block.values.map((value, i) => x(i) + "," + y(value)).join(" ")}
        fill="none" stroke="var(--accent-soft-fg)" strokeWidth="2" />}
      {block.values.map((value, index) => {
        const fill = selected === index ? "var(--accent-soft-fg)" : "var(--accent)";
        const activate = onSelect ? () => onSelect(index) : undefined;
        return block.kind === "bar" ? <rect key={index} x={x(index) - step * .33} y={Math.min(y(value), y(0))}
          width={step * .66} height={Math.max(1.5, Math.abs(y(value) - y(0)))} rx="2" fill={fill} onClick={activate}>
          <title>{block.labels[index]}：{value}</title>
        </rect> : <circle key={index} cx={x(index)} cy={y(value)} r={selected === index ? 6 : 4} fill={fill} onClick={activate}>
          <title>{block.labels[index]}：{value}</title>
        </circle>;
      })}
    </svg>
    {onSelect ? <Select label="查看数据点" value={selected === undefined ? "" : String(selected)}
      onChange={event => onSelect(Number(event.target.value))}
      options={[{ value: "", label: "选择数据点", disabled: true }, ...block.labels.map((label, index) => ({ value: String(index), label: label + "：" + block.values[index] }))]} /> :
      selected !== undefined && <p className="rich-component-note">{block.labels[selected]}：{block.values[selected]}</p>}
    <details><summary>数据明细</summary><dl className="rich-chart-values">{block.values.map((value, index) => <div key={index}><dt>{block.labels[index]}</dt><dd>{value}</dd></div>)}</dl></details>
  </figure>;
}

function Table({ block, sort, onSort }: { block: Extract<RichBlock, { type: "table" }>; sort?: RichComponentState["state"]; onSort?: (column: number, descending: boolean) => void }) {
  const column = typeof sort?.column === "number" ? sort.column : undefined;
  const descending = sort?.descending === true;
  const rows = useMemo(() => {
    if (column === undefined) return block.rows;
    return [...block.rows].sort((left, right) => left[column].localeCompare(right[column], "zh-CN", { numeric: true }) * (descending ? -1 : 1));
  }, [block.rows, column, descending]);
  return <div className="rich-table">
    <table>
      <caption>{block.title}</caption>
      <thead><tr>{block.columns.map((label, index) => <th key={index} scope="col" aria-sort={index === column ? descending ? "descending" : "ascending" : undefined}>
        {onSort ? <button type="button" onClick={() => onSort(index, index === column && !descending)}>{label}{index === column ? descending ? " ↓" : " ↑" : ""}</button> : label}
      </th>)}</tr></thead>
      <tbody>{rows.map((row, index) => <tr key={index}>{row.map((value, cell) => <td key={cell}>{value}</td>)}</tr>)}</tbody>
    </table>
  </div>;
}

function Form({ block, saved, editable, busy, onSubmit }: {
  block: Extract<RichBlock, { type: "form" }>; saved?: RichComponentState; editable: boolean; busy: boolean; onSubmit: (values: object) => void;
}) {
  const [values, setValues] = useState<Record<string, string>>({});
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSubmit(Object.fromEntries(block.fields.map(field => [field.name, values[field.name] ?? saved?.state[field.name] ?? ""])));
  };
  return <form className="rich-form" onSubmit={submit}>
    <strong>{block.title}</strong>
    {block.fields.map(field => {
      const value = values[field.name] ?? String(saved?.state[field.name] ?? "");
      const onChange = (next: string) => setValues(existing => ({ ...existing, [field.name]: next }));
      if (!editable) return <div key={field.name}><span>{field.label}</span><p>{value || "未填写"}</p></div>;
      if (field.choices.length) return <Select key={field.name} label={field.label} required={field.required} disabled={busy} value={value}
        options={[{ value: "", label: "请选择" }, ...field.choices.map(choice => ({ value: choice, label: choice }))]}
        onChange={event => onChange(event.target.value)} />;
      return <Input key={field.name} label={field.label} required={field.required} disabled={busy}
        value={value} onChange={event => onChange(event.target.value)} />;
    })}
    {editable && <Button type="submit" size="sm" loading={busy}>提交表单</Button>}
  </form>;
}

function PageProposal({ block, editable, busy, error, applied, onApply }: {
  block: Extract<RichBlock, { type: "page_proposal" }>; editable: boolean; busy: boolean; error: string; applied: AppliedPage | null; onApply: () => Promise<boolean>;
}) {
  const [open, setOpen] = useState(false);
  const [page, setPage] = useState<PageDocument | null>(null);
  const [loadError, setLoadError] = useState("");
  useEffect(() => {
    if (!open) return;
    let current = true;
    void loadPage(block.page_id).then(result => {
      if (!current) return;
      if (result.ok) setPage(result.data);
      else setLoadError(result.error);
    });
    return () => { current = false; };
  }, [open, block.page_id]);
  return <>
    <strong>页面修改建议</strong>
    <p className="rich-component-note">基于版本 {block.revision}，共 {block.changes.length} 项。</p>
    {applied ? <Link to={workspacePagePath(applied.workspace_id, applied.id)}>修改已应用于版本 {applied.revision}，打开页面</Link> :
      editable && <Button size="sm" variant="secondary" onClick={() => { setPage(null); setLoadError(""); setOpen(true); }}>审查修改</Button>}
    {!editable && !applied && block.changes.map((change, index) => "markdown" in change
      ? <ConversationMarkdown key={index} text={change.markdown} />
      : <p key={index}>引用页面 · {change.reference_title}</p>)}
    <Modal open={open} title="审查页面修改" size="xl" onClose={() => { if (!busy) setOpen(false); }}
      footer={<>
        <Button variant="secondary" disabled={busy} onClick={() => setOpen(false)}>关闭</Button>
        <Button loading={busy} disabled={!page || page.revision !== block.revision} onClick={async () => { if (await onApply()) setOpen(false); }}>应用修改</Button>
      </>}>
      {loadError ? <p role="alert" className="rich-component-error">{loadError}</p> : !page ? <p role="status">正在读取页面…</p> : <>
        <h3>{page.title}</h3>
        <p>当前版本 {page.revision} · 提案基于版本 {block.revision}</p>
        {page.revision !== block.revision && <p role="alert">页面已有新版本，请根据最新内容重新生成建议。</p>}
        {block.changes.map((change, index) => {
          if ("reference_title" in change) return <div key={index} className="rich-page-diff"><span>新增页面引用</span><strong>{change.reference_title}</strong></div>;
          const previous = page.blocks.find(candidate => candidate.id === change.block_id);
          return <div className="rich-page-diff" key={index}>
            <div><strong>原内容</strong><pre>{previous?.type === "markdown" ? previous.text : "新增内容块"}</pre></div>
            <div><strong>建议内容</strong><pre>{change.markdown}</pre></div>
          </div>;
        })}
      </>}
      {error && <p role="alert" className="rich-component-error">{error}</p>}
    </Modal>
  </>;
}

function PageCreateProposal({ block, editable, busy, applied, error, onApply }: {
  block: Extract<RichBlock, { type: "page_create_proposal" }>; editable: boolean; busy: boolean;
  applied: AppliedPage | null; error: string; onApply: () => Promise<boolean>;
}) {
  const [open, setOpen] = useState(false);
  return <>
    <strong>新建页面建议 · {block.title}</strong>
    {applied ? <Link to={workspacePagePath(applied.workspace_id, applied.id)}>页面已创建，打开页面</Link> :
      editable ? <Button size="sm" variant="secondary" onClick={() => setOpen(true)}>审查新建页面</Button> :
        <ConversationMarkdown text={block.markdown} />}
    <Modal open={open} title="审查新建页面" size="xl" onClose={() => { if (!busy) setOpen(false); }}
      footer={<>
        <Button variant="secondary" disabled={busy} onClick={() => setOpen(false)}>关闭</Button>
        <Button loading={busy} onClick={async () => { if (await onApply()) setOpen(false); }}>创建页面</Button>
      </>}>
      <h3>{block.title}</h3>
      <p>{block.parent_title ? `父页面 · ${block.parent_title}` : "在工作空间根目录创建"}</p>
      <ConversationMarkdown text={block.markdown} />
      {error && <p role="alert" className="rich-component-error">{error}</p>}
    </Modal>
  </>;
}
