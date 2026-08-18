import { useCallback, useEffect, useMemo, useState } from "react";
import { Bot, Plus, Users } from "lucide-react";
import {
  CapabilityDetailSection,
  CapabilityMetaRow,
  CapabilityStatusBadge,
  formatAgentSource,
} from "../components/capabilities";
import {
  AgentDefinition,
  AgentTeam,
  createAgent,
  listAgents,
  listTeams,
} from "../api/client";
import {
  Button,
  EmptyState,
  ErrorState,
  Input,
  Modal,
  PageHeader,
  Skeleton,
  Textarea,
} from "../components/ui";

export default function AgentsPage() {
  const [agents, setAgents] = useState<AgentDefinition[] | null>(null);
  const [teams, setTeams] = useState<AgentTeam[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [instructions, setInstructions] = useState("");

  const reload = useCallback(async () => {
    setError(null);
    try {
      const [agentResult, teamResult] = await Promise.all([listAgents(), listTeams()]);
      if (agentResult.ok) setAgents(agentResult.data);
      else setError(agentResult.error);
      if (teamResult.ok) setTeams(teamResult.data);
      else setError(teamResult.error);
    } catch (cause) {
      setError(String(cause));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    if (selectedId && agents?.some((agent) => agent.id === selectedId)) return;
    setSelectedId(agents?.[0]?.id ?? null);
  }, [agents, selectedId]);

  const selectedAgent = useMemo(
    () => agents?.find((agent) => agent.id === selectedId) ?? null,
    [agents, selectedId],
  );

  const handleCreate = async () => {
    if (!name.trim()) return;
    const result = await createAgent({ name: name.trim(), instructions: instructions.trim() });
    if (result.ok) {
      setShowCreate(false);
      setName("");
      setInstructions("");
      await reload();
      setSelectedId(result.data.id);
    } else {
      setError(result.error);
    }
  };

  return (
    <div className="capability-page">
      <PageHeader
        title="智能体"
        description="管理承担不同角色和任务的智能体配置。"
        actions={<Button onClick={() => setShowCreate(true)}><Plus className="h-4 w-4" />新建 Agent</Button>}
      />

      <div className="capability-page-body capability-page-body-stack">
        {error && (
          <ErrorState
            title="智能体暂时无法加载"
            description={error}
            action={<Button variant="secondary" size="sm" onClick={() => void reload()}>重试</Button>}
          />
        )}

        <section aria-labelledby="agent-list-title">
          <div className="capability-section-heading">
            <h2 id="agent-list-title"><Bot className="h-4 w-4" />Agent 定义</h2>
            <span>{agents?.length ?? 0} 个</span>
          </div>
          {agents === null && !error ? (
            <div className="capability-card-grid" aria-label="正在加载智能体">
              <Skeleton className="h-32 w-full" />
              <Skeleton className="h-32 w-full" />
              <Skeleton className="h-32 w-full" />
            </div>
          ) : agents && agents.length === 0 ? (
            <EmptyState title="还没有其他智能体" description="创建后，可以让不同智能体负责不同类型的任务。" action={<Button size="sm" onClick={() => setShowCreate(true)}>新建 Agent</Button>} />
          ) : agents ? (
            <div className="capability-card-grid">
              {agents.map((agent) => (
                <button
                  type="button"
                  key={agent.id}
                  aria-selected={agent.id === selectedId}
                  onClick={() => setSelectedId(agent.id)}
                  className={`capability-card capability-card-selectable ${agent.id === selectedId ? "capability-card-selected" : ""}`}
                >
                  <div className="capability-card-heading">
                    <span className="capability-icon"><Bot className="h-4 w-4" /></span>
                    <strong>{agent.name}</strong>
                    <CapabilityStatusBadge label={agent.enabled ? "已启用" : "已禁用"} tone={agent.enabled ? "success" : "default"} />
                  </div>
                  <p className="capability-card-description">{agent.description || "暂无描述"}</p>
                  <div className="capability-card-meta">{formatAgentSource(agent.source)} · 最大迭代 {agent.max_iterations}</div>
                </button>
              ))}
            </div>
          ) : null}
        </section>

        <section className="capability-detail-panel" aria-label="智能体详情">
          {selectedAgent ? (
            <AgentDetail agent={selectedAgent} />
          ) : (
            <EmptyState icon={<Bot className="h-6 w-6" />} title="选择一个智能体" description="查看真实的 Agent 配置字段。" className="min-h-[300px]" />
          )}
        </section>

        <section aria-labelledby="team-list-title">
          <div className="capability-section-heading">
            <h2 id="team-list-title"><Users className="h-4 w-4" />团队</h2>
            <span>{teams?.length ?? 0} 个</span>
          </div>
          {teams === null && !error ? (
            <div className="capability-card-grid" aria-label="正在加载团队">
              <Skeleton className="h-24 w-full" />
              <Skeleton className="h-24 w-full" />
            </div>
          ) : teams && teams.length === 0 ? (
            <EmptyState title="暂无团队" description="团队用于有界的多智能体顺序协作。" className="py-10" />
          ) : teams ? (
            <div className="capability-card-grid">
              {teams.map((team) => (
                <article key={team.id} className="capability-card">
                  <div className="capability-card-heading"><span className="capability-icon"><Users className="h-4 w-4" /></span><strong>{team.name}</strong></div>
                  <p className="capability-card-description">{team.description || "暂无描述"}</p>
                  <div className="capability-card-meta">成员 {team.member_agent_ids.length} · 最大深度 {team.max_depth} · 最大执行 {team.max_agent_executions}</div>
                </article>
              ))}
            </div>
          ) : null}
        </section>
      </div>

      <Modal open={showCreate} onClose={() => setShowCreate(false)} title="新建 Agent">
        <div className="space-y-3">
          <Input label="名称" value={name} onChange={(event) => setName(event.target.value)} autoFocus />
          <Textarea label="指令" value={instructions} onChange={(event) => setInstructions(event.target.value)} rows={4} placeholder="该 Agent 的职责与约束（LLM-only，不直接调用工具）" />
          <div className="flex justify-end gap-2"><Button variant="ghost" onClick={() => setShowCreate(false)}>取消</Button><Button onClick={handleCreate} disabled={!name.trim()}>创建</Button></div>
        </div>
      </Modal>
    </div>
  );
}

function AgentDetail({ agent }: { agent: AgentDefinition }) {
  return (
    <div className="capability-detail-content">
      <div className="capability-detail-heading">
        <div>
          <div className="capability-detail-title-line"><Bot className="h-5 w-5 text-[var(--accent-primary)]" /><h2>{agent.name}</h2><CapabilityStatusBadge label={agent.enabled ? "已启用" : "已禁用"} tone={agent.enabled ? "success" : "default"} /></div>
          <p className="capability-detail-description">{agent.description || "暂无描述"}</p>
        </div>
      </div>

      <CapabilityDetailSection title="配置概览">
        <dl className="capability-meta-list">
          <CapabilityMetaRow label="来源" value={formatAgentSource(agent.source)} />
          <CapabilityMetaRow label="模型" value={agent.model || "未单独配置"} mono />
          <CapabilityMetaRow label="最大迭代" value={String(agent.max_iterations)} />
          <CapabilityMetaRow label="状态" value={agent.enabled ? "已启用" : "已禁用"} />
        </dl>
      </CapabilityDetailSection>

      <CapabilityDetailSection title="工具与能力">
        <div className="capability-token-list">
          {agent.allowed_tools.length > 0 ? agent.allowed_tools.map((tool) => <span key={tool}>{tool}</span>) : <span>未配置工具</span>}
          {agent.capabilities.length > 0 && agent.capabilities.map((capability) => <span key={capability}>{capability}</span>)}
        </div>
      </CapabilityDetailSection>

      <CapabilityDetailSection title="指令" technical>
        <pre className="capability-content-preview">{agent.instructions || "暂无指令"}</pre>
      </CapabilityDetailSection>

      <CapabilityDetailSection title="技术详情" technical>
        <dl className="capability-meta-list">
          <CapabilityMetaRow label="Agent ID" value={agent.id} mono />
          <CapabilityMetaRow label="原始来源" value={agent.source} mono />
        </dl>
      </CapabilityDetailSection>
    </div>
  );
}
