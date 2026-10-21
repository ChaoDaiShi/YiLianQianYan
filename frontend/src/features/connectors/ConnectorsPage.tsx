import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { Button, ErrorState, Input, Modal } from "../../components/ui";
import PageShell from "../../components/layout/PageShell";
import { createConnectorCanvas, listConnectorCanvases, type ConnectorCanvas as CanvasRecord } from "../../api/connectors";
import { listAgents, type AgentDefinition } from "../../api/tasks";
import CanvasManager from "./CanvasManager";
import MessageBranchWorkspace from "./messages/MessageBranchWorkspace";
import ConversationCanvas from "./messages/ConversationCanvas";

export default function ConnectorsPage() {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const [canvases, setCanvases] = useState<CanvasRecord[]>([]);
  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [newOpen, setNewOpen] = useState(false);
  const [name, setName] = useState("");
  const [agentId, setAgentId] = useState("");
  const [managerOpen, setManagerOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<{ id: string; name: string } | null>(null);
  const activeRef = useRef(activeId);
  activeRef.current = activeId;

  const reloadCatalog = useCallback(async () => {
    const result = await listConnectorCanvases();
    setCanvases(result.canvases);
    return result.canvases;
  }, []);

  useEffect(() => {
    let current = true;
    void Promise.all([listConnectorCanvases(), listAgents()]).then(([catalog, result]) => {
      if (!current) return;
      if (!result.ok) throw new Error(result.error);
      setCanvases(catalog.canvases);
      setAgents(result.data);
    }).catch(cause => { if (current) setError(String(cause)); })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, []);

  useEffect(() => { if (params.get("conversation")) setActiveId(null); }, [params]);

  function openNewCanvas() {
    setName(""); setAgentId(agents[0]?.id ?? ""); setError(""); setNewOpen(true);
  }

  async function createCanvas() {
    setBusy(true); setError("");
    try {
      const created = await createConnectorCanvas(name, agentId);
      await reloadCatalog();
      const next = new URLSearchParams(params); next.delete("conversation"); setParams(next);
      setActiveId(created.canvas_id); setNewOpen(false);
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  }

  async function deleted(id: string) {
    await reloadCatalog();
    if (activeRef.current === id) setActiveId(null);
  }

  const activeCanvas = canvases.find(canvas => canvas.id === activeId);
  return <PageShell title="画布" description="从真实消息展开对话树，沿回答探索另一条路径" variant="workspace" contentClassName="p-0 min-h-0 overflow-hidden" actions={<>
    <select aria-label="当前画布" className="settings-select max-w-48" disabled={busy} value={activeCanvas?.id ?? ""} onChange={event => {
      const next = new URLSearchParams(params); next.delete("conversation"); setParams(next);
      setError(""); setActiveId(event.target.value || null);
    }}><option value="">全部对话</option>{canvases.map(canvas => <option key={canvas.id} value={canvas.id}>{canvas.name}</option>)}</select>
    <Button size="sm" aria-label="新建画布" className="whitespace-nowrap" disabled={loading} onClick={openNewCanvas}>新建画布</Button>
    <Button size="sm" aria-label="管理全部画布" className="whitespace-nowrap" variant="secondary" onClick={() => { setDeleteTarget(null); setManagerOpen(true); }}>管理全部画布</Button>
    <Button size="sm" aria-label="删除当前画布" className="whitespace-nowrap" variant="danger" disabled={!activeCanvas || loading || busy} onClick={() => { if (activeCanvas) { setDeleteTarget({ id: activeCanvas.id, name: activeCanvas.name }); setManagerOpen(true); } }}>删除当前画布</Button>
  </>}>
    <div className="flex h-full min-h-0 flex-col">
      {error && <div className="shrink-0 px-5 py-2"><ErrorState title="画布操作未完成" description={error} /></div>}
      <div className="relative flex-1 min-h-0">{activeCanvas ? <MessageBranchWorkspace key={activeCanvas.id} canvasId={activeCanvas.id} agentId={activeCanvas.agent_id} /> : <ConversationCanvas initialConversationId={params.get("conversation")} />}</div>
    </div>
    <Modal open={newOpen} onClose={() => setNewOpen(false)} title="新建画布" footer={<Button disabled={busy || !name.trim() || !agentId} onClick={() => void createCanvas()}>创建画布</Button>}>
      <div className="space-y-4"><Input label="画布名称" value={name} onChange={event => setName(event.target.value)} />
        <label className="block text-sm">所属智能体<select aria-label="画布所属智能体" className="settings-select" value={agentId} onChange={event => setAgentId(event.target.value)}><option value="">选择已创建的智能体</option>{agents.map(agent => <option key={agent.id} value={agent.id}>{agent.name}</option>)}</select></label>
        {agents.length === 0 && <Button variant="secondary" onClick={() => navigate("/agents")}>先创建智能体</Button>}{error && <p role="alert">{error}</p>}
      </div>
    </Modal>
    <CanvasManager open={managerOpen} initialDeleteCanvas={deleteTarget} onClose={() => { setManagerOpen(false); setDeleteTarget(null); }} activeId={activeCanvas?.id ?? null} onDeleted={deleted} onUpdated={async () => { await reloadCatalog(); }} />
  </PageShell>;
}
