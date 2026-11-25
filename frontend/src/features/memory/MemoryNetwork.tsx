import { useEffect, useMemo, useState } from "react";
import { Boxes } from "lucide-react";
import { Badge, Button, ErrorState, Modal, Select } from "../../components/ui";
import { MemoryGraphCanvas } from "./MemoryGraphCanvas";
import MemoryCorrespondenceDialog from "./MemoryCorrespondenceDialog";
import MemorySpaceView, { type SpaceVersion } from "./MemorySpaceView";
import MemorySpaceBoundary from "./MemorySpaceBoundary";
import { formatMemoryTime } from "./memoryCenterModel";
import { memoryGraphModes, projectMemoryGraph, type GraphEdge, type MemoryGraphMode } from "./memoryGraphModel";
import { readMemoryGraph, type MemoryGraphQuery, type MemoryGraphSnapshot } from "./memoryGraphApi";
import { memorySpace } from "./memorySpaceModel";
import { readMemoryRelationship, relationshipKinds } from "./relationship";
import { extractMemoryRelationships } from "./memoryGraphApi";
import { reindexMemories } from "../../api/memory";
import { listConversations } from "../../api/conversations";
import { listWorkspaces } from "../../api/workspaces";
import { MemoryGraphInspector, memoryVersionDiff, memoryVersionStatus, VersionSource } from "./MemoryGraphInspector";
import "./memoryChronicle.css";

type SpacePhase = "flat" | "enter" | "active" | "exit";

export default function MemoryNetwork({ query, refreshKey, mode, onModeChange, onSelect, onReview, onVisible, onAdd, onClearFilters, scope, onScopeChange }: {
  query: MemoryGraphQuery;
  refreshKey: number;
  mode: MemoryGraphMode;
  onModeChange: (mode: MemoryGraphMode) => void;
  onSelect: (id: string) => void;
  onReview: () => void;
  onVisible: (snapshot: MemoryGraphSnapshot) => void;
  onAdd: () => void;
  onClearFilters: () => void;
  scope: string;
  onScopeChange: (scope: string) => void;
}) {
  const { q, category, source, withdrawn } = query;
  const [focusId, setFocusId] = useState<string | null>(null);
  const [previousFocus, setPreviousFocus] = useState<string | null>(null);
  const [scopeOptions, setScopeOptions] = useState<{value:string;label:string}[]>([]);
  const [phase, setPhase] = useState<SpacePhase>("flat");
  const spatial = phase !== "flat";
  const [showPending, setShowPending] = useState(false);
  const [selected, setSelected] = useState<{ memoryId: string; versionId: string } | null>(null);
  const [dialog, setDialog] = useState<"add" | "links" | string | null>(null);
  const [limit, setLimit] = useState(100);
  const [nonce, setNonce] = useState(0);
  const [snapshot, setSnapshot] = useState<MemoryGraphSnapshot | null>(null);
  const [fitKey, setFitKey] = useState("");
  const [error, setError] = useState<{message:string;status?:number} | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const retry = () => setNonce(value => value + 1);

  useEffect(() => {
    let subscribed = true;
    void Promise.allSettled([listWorkspaces(), listConversations()]).then(([projects,chats]) => {
      if (!subscribed) return;
      const workspaces = projects.status === "fulfilled" && projects.value.ok ? projects.value.data.filter(item => item.status === "active") : [];
      const conversations = chats.status === "fulfilled" ? chats.value : [];
      setScopeOptions([
        ...workspaces.map(item => ({value:`workspace:${item.id}`,label:`工作空间 · ${item.name}`})),
        ...conversations.map(item => ({value:`conversation:${item.id}`,label:`对话 · ${item.title}`})),
      ]);
    });
    return () => { subscribed = false; };
  }, []);

  useEffect(() => {
    let active = true;
    const load = () => {
      setLoading(true);
      setError(null);
      const workspace_id = scope.startsWith("workspace:") ? scope.slice(10) : undefined;
      const conversation_id = scope.startsWith("conversation:") ? scope.slice(13) : undefined;
      void readMemoryGraph({ q, category, source, withdrawn, workspace_id, conversation_id }, limit)
        .then(data => {
          if (!active) return;
          setSnapshot(data);
          setFitKey(JSON.stringify([q, category, source, scope]));
          onVisible(data);
        })
        .catch(cause => { if (active) { setSnapshot(null); setError({message:String(cause?.message || cause),status:cause?.status}); } })
        .finally(() => { if (active) setLoading(false); });
    };
    load();
    return () => { active = false; };
  }, [q, category, source, withdrawn, scope, limit, nonce, refreshKey, onVisible]);

  const graph = useMemo(() => snapshot && projectMemoryGraph(snapshot, mode, showPending), [snapshot, mode, showPending]);
  const chosen = selected && snapshot?.histories[selected.memoryId]?.find(version => version.version_id === selected.versionId);
  const historicalRelationship = chosen?.relationship_known && chosen.relationship != null ? readMemoryRelationship({
    id: chosen.version_id, category: chosen.category, content: chosen.content, source: chosen.source,
    created_at: chosen.recorded_at, updated_at: chosen.recorded_at, metadata: JSON.stringify({ relationship: chosen.relationship }),
  }) : null;
  const record = snapshot?.relations.find(relation => relation.id === dialog);
  const oldRelation = snapshot?.relation_versions.find(version => `history:${version.relation_id}:${version.revision}` === dialog);
  const oldEndpoints = oldRelation && Object.values(snapshot?.histories ?? {}).flat()
    .filter(version => version.version_id === oldRelation.source_version_id || version.version_id === oldRelation.target_version_id);
  const extraction = snapshot?.extracted_relations?.find(relation => relation.id === dialog);
  const change = dialog?.startsWith("event:") ? snapshot?.change_events?.find(item => item.id === dialog.slice(6)) : undefined;
  const changeTarget = change && snapshot?.histories[change.memory_id]?.find(version => version.version_id === change.version_id);
  const changeSource = change?.related_memory_id && snapshot?.histories[change.related_memory_id]?.find(version => version.version_id === change.related_version_id);
  const versionEvent = chosen && snapshot?.change_events?.find(item => item.version_id === chosen.version_id);
  const extractedText = extraction && snapshot?.histories[extraction.source_memory_id]?.find(version => version.revision === extraction.source_memory_revision);
  const focusMemory = focusId?.startsWith("memory:") ? focusId.slice(7) : null;
  const graphFocus = focusMemory && snapshot?.memories.find(memory => memory.id === focusMemory);
  const activeVersion = focusMemory && snapshot?.histories[focusMemory]?.find(version => version.version_id === snapshot.current_version_ids?.[focusMemory]);
  const beforeChosen = selected && chosen && snapshot?.histories[selected.memoryId]?.find(version => version.layer === chosen.layer - 1);
  const chosenDiff = chosen && beforeChosen ? memoryVersionDiff(beforeChosen.content, chosen.content) : null;
  function selectNode(id: string | null) {
    if (focusId && id && id !== focusId) setPreviousFocus(focusId);
    setFocusId(id);
  }
  function openEvidence(edge: GraphEdge) {
    if (edge.evidence.type === "memory") onSelect(edge.evidence.id);
    else setDialog(edge.evidence.id);
  }
  const similarities = snapshot?.semantic?.similarities;
  const related = useMemo(() => (similarities ?? []).flatMap(pair => {
    const other = pair.source_memory_id === focusMemory ? pair.target_memory_id : pair.target_memory_id === focusMemory ? pair.source_memory_id : null;
    const record = other && snapshot?.memories.find(memory => memory.id === other);
    return record ? [{record,score:pair.score}] : [];
  }).sort((a,b) => b.score-a.score), [similarities, snapshot, focusMemory]);
  const openVersion = (memoryId: string, version: SpaceVersion) => setSelected({ memoryId, versionId: version.version_id });
  async function extract(sourceId = focusMemory, relationId?: string) {
    if (!sourceId || busy) return;
    setBusy(true); setNotice("");
    try {
      const result = await extractMemoryRelationships(sourceId, relationId);
      setNotice(result.candidate_ids.length ? `已生成 ${result.candidate_ids.length} 条待审阅关系` : "这条记忆没有新增可审阅关系，可继续阅读原文。");
      if (result.candidate_ids.length) { setDialog(null); onReview(); }
    } catch (cause) { setNotice(`提取失败：${String((cause as Error).message || cause)}。原文和已保存关系仍可查看。`); }
    finally { setBusy(false); }
  }
  async function rebuild() {
    if (busy) return;
    setBusy(true); setNotice("");
    try {
      const result = await reindexMemories();
      setNotice(result.ok ? `本次索引成功 ${result.succeeded ?? 0} 条，失败 ${result.failed ?? 0} 条。` : result.error ?? "索引失败，仍可查看正文。");
      retry();
    } catch (cause) { setNotice(String((cause as Error).message || cause)); }
    finally { setBusy(false); }
  }

  return <section className="memory-network-workspace memory-chronicle" aria-label="记忆关系图" aria-busy={loading}>
    <header className="memory-chronicle-header">
      <div className="memory-chronicle-heading">
        <i className="memory-chronicle-seal" aria-hidden="true" />
        <h2>记忆图谱</h2>
      </div>
    <div className="memory-network-modes">
      <div className="memory-network-filter">
        <Select size="sm" aria-label="关系图视图" value={mode} options={Object.entries(memoryGraphModes).map(([value, label]) => ({ value, label }))}
          onChange={event => { setFocusId(null); onModeChange(event.target.value as MemoryGraphMode); }} />
      </div>
      <Button className="memory-chronicle-switch" size="sm" variant="secondary" disabled={!spatial && !graph?.overview.length} onClick={() => { setPhase(phase === "flat" || phase === "exit" ? "enter" : "exit"); setSelected(null); }}>
        <Boxes size={15} aria-hidden="true" />{phase === "active" || phase === "enter" ? "返回二维" : "立体查看"}
      </Button>
      {!!scopeOptions.length && <div className="memory-network-filter">
        <Select size="sm" aria-label="图谱范围" value={scope} options={[{value:"",label:"档案通用记忆"},...scopeOptions]}
          onChange={event => { onScopeChange(event.target.value); setSnapshot(null); setFocusId(null); setPreviousFocus(null); setSelected(null); setDialog(null); setPhase("flat"); }} />
      </div>}
      {!withdrawn && <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={showPending} onChange={event => setShowPending(event.target.checked)} />显示待确认</label>}
      {!withdrawn && mode === "network" && <Button size="sm" variant="secondary" disabled={(snapshot?.memories.length ?? 0) < 2} onClick={() => setDialog("add")}>建立对应关系</Button>}
      {!withdrawn && mode === "network" && !!snapshot?.relations.length && <Button size="sm" variant="ghost" onClick={() => setDialog("links")}>管理对应关系</Button>}
      {snapshot?.next_offset != null && limit < 250 && <Button size="sm" variant="ghost" disabled={loading} onClick={() => setLimit(value => Math.min(value + 75, 250))}>读取更多记忆</Button>}
      {snapshot?.next_offset != null && limit >= 250 && <span className="text-xs">已显示前 250 条，缩小搜索范围可查看其余记录。</span>}
    </div>
    </header>
    <div className="memory-chronicle-status">
    <div className="memory-graph-legend" aria-label="图例">
      <span>{withdrawn ? "当前展示已撤回记忆" : phase === "active" ? "立体查看 · 记忆与历史版本"
        : showPending ? "当前有效记忆 · 已加入待确认关系" : <>当前态投影 · <span>当前展示有效记忆</span></>}</span>
      <span><i />已确认关系</span><span><i className="memory-legend-pending" />待确认关系</span>
      <span aria-hidden={!spatial} style={{visibility:spatial ? "visible" : "hidden"}}><i className="memory-legend-history" />版本轨迹</span>
      <span aria-hidden={!spatial} style={{visibility:spatial ? "visible" : "hidden"}}><i className="memory-legend-event" />经审阅的关联变更</span>
    </div>
    {!withdrawn && snapshot?.semantic && <p className="memory-semantic-hint" role="status">
      {!snapshot.semantic.configured ? "未配置向量模型 · 当前按文本和已审阅关系展示"
        : `${snapshot.semantic.indexed_memory_ids.length}/${snapshot.memories.length} 条已有有效语义向量 · 相似性仅用于布局和查找`}
      {snapshot.semantic.configured && snapshot.semantic.indexed_memory_ids.length < snapshot.memories.length &&
        <Button size="sm" variant="ghost" loading={busy} onClick={() => void rebuild()}>重试缺失索引</Button>}
    </p>}
    </div>
    {notice && <p className="memory-semantic-hint" role="status">{notice}</p>}
    {error ? <ErrorState title={error.status === 401 || error.status === 403 ? "没有权限查看此图谱" : "记忆图谱暂时无法加载"} description={error.message} onRetry={retry} />
      : loading && !snapshot ? <p role="status">正在读取记忆图谱…</p>
      : graph && snapshot && <MemoryGraphCanvas graph={graph} focusId={focusId} onFocus={node => selectNode(node?.id ?? null)}
        onRecord={onSelect} onCorrespondence={setDialog}
        fitKey={`${fitKey}/${mode}`}
        emptyAction={q || category || source ? <Button size="sm" onClick={onClearFilters}>清除搜索与筛选</Button> : !withdrawn ? <Button size="sm" onClick={snapshot.memories.length ? onReview : onAdd}>{snapshot.memories.length ? "前往待审阅" : "添加第一条记忆"}</Button> : undefined}
        groupedOverview={mode === "relations" || mode === "network"} sharedWorld affinities={similarities} label={memoryGraphModes[mode]}
        panel={graphFocus && <MemoryGraphInspector key={graphFocus.id} memory={graphFocus} head={activeVersion || undefined}
          history={snapshot.histories[graphFocus.id] ?? []} graph={graph}
          onBack={previousFocus && graph.nodes.has(previousFocus) ? () => { setFocusId(previousFocus); setPreviousFocus(null); } : undefined}
          onOpen={() => onSelect(graphFocus.id)} onFocus={selectNode} onEvidence={openEvidence}
          onVersion={version => openVersion(graphFocus.id, version)} />}
        actions={!withdrawn && graphFocus ? <span className="memory-network-actions">
          <Button variant="ghost" size="sm" loading={busy} onClick={() => void extract()}>从此记忆提取关系</Button>
          <Button variant="ghost" size="sm" onClick={() => setDialog("similar")}>查找相似记忆</Button>
        </span> : undefined}
        emptyMessage={!snapshot.memories.length
          ? q || category || source ? "没有匹配的记忆记录" : "这里暂时还没有记忆记录"
          : "没有匹配的关系记录"}
        space={spatial ? projection => {
          const scene = memorySpace(snapshot, projection, showPending);
          return <MemorySpaceBoundary onReturn={() => setPhase("flat")}><MemorySpaceView {...scene} focusedId={focusMemory} onVersion={openVersion}
            phase={phase} cameraToolbar={projection.cameraToolbar} onTransitionEnd={value => setPhase(value === "enter" ? "active" : "flat")}
            onFocus={id => { if (id && graph.nodes.has(`memory:${id}`)) selectNode(`memory:${id}`); }}
            onEntity={selectNode}
            onEdge={id => { const found = projection.edges.find(edge => edge.id === id);
              if (found?.evidence.type === "memory") onSelect(found.evidence.id);
              else setDialog(found?.evidence.id ?? (id.startsWith("event:") ? id : scene.edges.find(edge => edge.id === id)?.historical ? `history:${id}` : id.split(":")[0]));
            }} initialViewport={projection.viewport} /></MemorySpaceBoundary>;
        } : undefined} />}
    {chosen && <Modal open title="记忆版本详情" size="md" onClose={() => setSelected(null)}>
      <div className="memory-network-version-body">
        <div className="flex flex-wrap gap-2"><Badge>{chosen.layer === 0 ? chosen.baseline ? "底层 · 迁移基线" : "底层 · 原始版本" : `第 ${chosen.layer} 层 · 第 ${chosen.layer} 次变更`}</Badge>
          <Badge>{memoryVersionStatus(chosen, snapshot?.current_version_ids?.[selected!.memoryId])}</Badge></div>
        {chosen.baseline && <p>这是现有记录中可核实的起点，更早的内容或改动次数未知。</p>}
        <p>{chosen.content}</p>
        {chosenDiff && (chosenDiff.removed || chosenDiff.inserted) && <p aria-label="版本正文差异"><del>{chosenDiff.removed}</del> <ins>{chosenDiff.inserted}</ins></p>}
        <dl>
          <dt>记录时间</dt><dd>{formatMemoryTime(chosen.recorded_at)}</dd>
          <dt>来源</dt><dd><VersionSource version={chosen} /></dd>
          {chosen.reason && !chosen.baseline && <><dt>变更记录</dt><dd>{chosen.reason === "created" ? "创建记忆" : chosen.reason}</dd></>}
          {versionEvent && <details><summary>变更事件编号</summary><p>{versionEvent.id}</p></details>}
          {historicalRelationship && <><dt>该版本的关系</dt><dd>{relationshipKinds[historicalRelationship.kind]} · {historicalRelationship.directed ? "有方向" : "无方向"} · {historicalRelationship.certainty === "inferred" ? "待确认" : "已确认"}</dd></>}
          {!chosen.relationship_known && <><dt>关系状态</dt><dd>此版本的关系结构未记录</dd></>}
        </dl>
      </div>
    </Modal>}
    {oldRelation && <Modal open title="历史关系依据" size="md" onClose={() => setDialog(null)}>
      <div className="memory-network-version-body">
        <Badge>记录时已确认</Badge><p>{oldRelation.label}</p>
        <p>{formatMemoryTime(oldRelation.recorded_at)}</p>
        {oldEndpoints && oldEndpoints.map(version => <div key={version.version_id}>
          <strong>V{version.revision} · {version.baseline ? "已知基线" : version.layer === 0 ? "原始版本" : `第 ${version.layer} 次更改`}</strong>
          <p>{version.content}</p><VersionSource version={version} />
        </div>)}
      </div>
    </Modal>}
    {change && changeTarget && changeSource && <Modal open title="关联变更依据" size="md" onClose={() => setDialog(null)}>
      <div className="memory-network-version-body">
        <Badge>经用户审阅</Badge>
        <p>{changeTarget.reason}</p>
        <dl><dt>用户选定的来源版本</dt><dd>{changeSource.content}</dd>
          <dt>确认后的目标版本</dt><dd>{changeTarget.content}</dd>
          <dt>记录时间</dt><dd>{formatMemoryTime(change.recorded_at)}</dd></dl>
        <details><summary>变更事件编号</summary><p>{change.id}</p></details>
      </div>
    </Modal>}
    {extraction && <Modal open title="关系来源与依据" size="md" onClose={() => setDialog(null)}>
      <div className="memory-network-version-body">
        <Badge>{extraction.relationship.certainty === "inferred" ? "待确认" : "已确认"}</Badge>
        <p>{extraction.relationship.subject} · {extraction.relationship.label} · {extraction.relationship.object}</p>
        {extractedText && <blockquote>{[...extractedText.content].slice(extraction.evidence_start,extraction.evidence_end).join("")}</blockquote>}
        <Button size="sm" variant="secondary" onClick={() => { setDialog(null); onSelect(extraction.source_memory_id); }}>查看来源记忆</Button>
        <Button size="sm" variant="secondary" loading={busy} onClick={() => void extract(extraction.source_memory_id, extraction.id)}>提交关系纠正审阅</Button>
        {notice && <p role="status">{notice}</p>}
      </div>
    </Modal>}
    {dialog === "similar" && <Modal open title="相似记忆" size="md" onClose={() => setDialog(null)}>
      <div className="memory-network-version-body">
        <p>基于当前有效文本的语义向量计算，只表示内容接近。</p>
        {related.length ? related.map(({record,score}) => <Button key={record.id} size="sm" variant="secondary" onClick={() => { setDialog(null); onSelect(record.id); }}>
          {record.content.slice(0,90)} · 相似度 {score.toFixed(2)}
        </Button>) : <p>{!snapshot?.semantic?.configured ? "未配置向量模型。" : focusMemory && !snapshot.semantic.indexed_memory_ids.includes(focusMemory) ? "这条记忆尚无有效向量，可重试索引。" : "当前范围内没有满足条件的相似记忆。"}</p>}
      </div>
    </Modal>}
    {dialog === "links" && snapshot && <Modal open title="管理对应关系" onClose={() => setDialog(null)}>
      <div className="memory-network-version-body">{snapshot.relations.map(item =>
        <Button key={item.id} size="sm" variant="secondary" onClick={() => setDialog(item.id)}>{item.label} · {item.status === "confirmed" ? "已确认" : item.status === "pending" ? "待确认" : "已撤回"}</Button>)}</div>
    </Modal>}
    {snapshot && (dialog === "add" || record) && <MemoryCorrespondenceDialog key={dialog} snapshot={snapshot} record={record ?? null} selection={focusMemory}
      onClose={() => setDialog(null)} onChanged={() => { setDialog(null); retry(); }} onReview={() => { setDialog(null); onReview(); }} />}
  </section>;
}
