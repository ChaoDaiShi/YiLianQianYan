import { useState } from "react";
import { Link } from "react-router-dom";
import type { MemoryRecord, MemoryVersion } from "../../api/memory";
import { Badge, Button, Tabs } from "../../components/ui";
import { formatMemoryTime, getMemoryCategoryPresentation, getMemorySourceLabel } from "./memoryCenterModel";
import { relationshipKinds } from "./relationship";
import type { GraphEdge, MemoryGraph } from "./memoryGraphModel";
import { knowledgeDocumentForMemory } from "./knowledgeMemory";

export function memoryVersionDiff(before: string, after: string) {
  const a = [...before], b = [...after];
  let start = 0, tail = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  while (tail < a.length - start && tail < b.length - start &&
    a[a.length - 1 - tail] === b[b.length - 1 - tail]) tail++;
  return { removed: a.slice(start, a.length - tail).join(""), inserted: b.slice(start, b.length - tail).join("") };
}

export function memoryVersionStatus(version: MemoryVersion, currentId?: string) {
  if (version.version_id === currentId) return "当前有效";
  if (version.status === "withdrawn") return "已撤回";
  const now = Date.now();
  if (version.valid_from != null && version.valid_from > now) return "尚未生效";
  if (version.valid_until != null && version.valid_until <= now) return "已失效";
  return "历史记录";
}

export function VersionSource({ version }: { version: MemoryVersion }) {
  if (version.source_removed) return <span>来源已移除</span>;
  if (version.source_conversation_id) {
    const conversation = encodeURIComponent(version.source_conversation_id);
    const message = version.source_message_id ? `?message=${encodeURIComponent(version.source_message_id)}` : "";
    return <Link to={`/chat/${conversation}${message}`}>{message ? "定位来源消息" : "打开来源对话"}</Link>;
  }
  return <span>{version.baseline ? "来源未记录" : getMemorySourceLabel(version.source)}</span>;
}

const tabs = [
  {id:"current",label:"当前",panelId:false as const}, {id:"history",label:"演变",panelId:false as const},
  {id:"relations",label:"关系",panelId:false as const}, {id:"source",label:"来源",panelId:false as const},
];

export function MemoryGraphInspector({ memory, head, history, graph, onBack, onOpen, onFocus, onEvidence, onVersion }: {
  memory: MemoryRecord; head: MemoryVersion | undefined; history: readonly MemoryVersion[]; graph: MemoryGraph;
  onBack?: () => void; onOpen: () => void; onFocus: (id: string) => void;
  onEvidence: (edge: GraphEdge) => void; onVersion: (version: MemoryVersion) => void;
}) {
  const [tab, setTab] = useState("current");
  const id = `memory:${memory.id}`;
  const node = graph.nodes.get(id);
  const edges = graph.edges.filter(edge => edge.source === id || edge.target === id);
  const category = getMemoryCategoryPresentation(memory.category).label;
  const scope = head?.scope === "profile" ? "当前档案" : head?.scope.startsWith("workspace:") ? "工作空间" : head?.scope.startsWith("conversation:") ? "来源对话" : "范围未记录";
  return <aside className="memory-graph-inspector" aria-label="选中记忆详情">
    <div className="memory-inspector-heading">
      <strong>{node?.label ?? memory.content}</strong>
      {onBack && <Button variant="ghost" size="sm" onClick={onBack}>上一节点</Button>}
      <Button variant="ghost" size="sm" onClick={onOpen}>完整详情</Button>
    </div>
    <Tabs tabs={tabs} activeId={tab} onChange={setTab} variant="segment" size="sm" aria-label="节点详情栏目" />
    <div className="memory-inspector-scroll">
      {tab === "current" && <>
        <div className="memory-inspector-badges"><Badge>{category}</Badge><Badge>{head ? "当前有效" : "无当前有效版本"}</Badge><Badge>{scope}</Badge></div>
        <p>{head?.content ?? memory.content}</p>
        <VersionOrRecordSource memory={memory} version={head} />
      </>}
      {tab === "history" && (history.length ? <ol>
        {history.map(version => {
          const previous = history.find(item => item.layer === version.layer - 1);
          const difference = previous ? memoryVersionDiff(previous.content, version.content) : null;
          return <li key={version.version_id}>
            <Button variant="ghost" size="sm" onClick={() => onVersion(version)}>{version.baseline ? "已知基线" : version.layer === 0 ? "原始层" : `第 ${version.layer} 次更改`}</Button>
            <small>{formatMemoryTime(version.recorded_at)} · {memoryVersionStatus(version, head?.version_id)}</small>
            {difference && <p>{difference.removed && <del>{difference.removed}</del>}{difference.inserted && <ins>{difference.inserted}</ins>}{!difference.removed && !difference.inserted && "正文未变"}</p>}
          </li>;
        })}
      </ol> : <p>没有可用的版本记录。</p>)}
      {tab === "relations" && (edges.length ? <ul>
        {edges.map(edge => {
          const target = graph.nodes.get(edge.source === id ? edge.target : edge.source);
          const kind = edge.kind in relationshipKinds ? relationshipKinds[edge.kind as keyof typeof relationshipKinds] : edge.kind === "source" ? "来源引用" : "对应关系";
          return <li key={edge.id}>
            <Button variant="ghost" size="sm" onClick={() => target && onFocus(target.id)}>{target?.label ?? edge.label}</Button>
            <small>{kind} · {edge.label} · {edge.directed ? edge.source === id ? "指向目标" : "由目标指向" : "无方向"} · {edge.inferred ? "待确认" : "当前有效"}</small>
            <Button variant="ghost" size="sm" onClick={() => onEvidence(edge)}>查看依据</Button>
          </li>;
        })}
      </ul> : <p>当前没有可见的直接关系。</p>)}
      {tab === "source" && <VersionOrRecordSource memory={memory} version={head} />}
    </div>
  </aside>;
}

function VersionOrRecordSource({ memory, version }: { memory: MemoryRecord; version?: MemoryVersion }) {
  if (memory.source_removed) return <p>来源已移除</p>;
  const document = knowledgeDocumentForMemory(memory);
  if (document) return <Link to={`/knowledge?document=${encodeURIComponent(document)}`}>打开源文档</Link>;
  let metadata: Record<string, unknown> = {};
  try { metadata = JSON.parse(memory.metadata ?? "{}") ?? {}; } catch { /* plain memory */ }
  const graph = metadata.source_graph_id, workspace = metadata.source_workspace_id;
  if (typeof graph === "string" && typeof workspace === "string") {
    return <Link to={`/workspaces/${encodeURIComponent(workspace)}?tab=tasks&graph=${encodeURIComponent(graph)}`}>查看任务图与验收</Link>;
  }
  if (version) return <VersionSource version={version} />;
  return <p>{getMemorySourceLabel(memory.source)}</p>;
}
