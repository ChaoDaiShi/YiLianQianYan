import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Bot, ChevronRight, Plus, Search, Users } from "lucide-react";
import { CapabilityMetaRow, CapabilityStatusBadge, formatAgentSource } from "../components/capabilities";
import { AgentDefinition, AgentTeam, createAgent, listAgents, listTeams } from "../api/client";
import PersonaEditor from "../features/agents/PersonaEditor";
import PageShell from "../components/layout/PageShell";
import { Button, EmptyState, ErrorState, FormField, Input, Modal, Section, showToast, Skeleton, Textarea } from "../components/ui";

export default function AgentsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [agents, setAgents] = useState<AgentDefinition[] | null>(null);
  const [teams, setTeams] = useState<AgentTeam[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState(searchParams.get("agent") || "default");
  const [search, setSearch] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [instructions, setInstructions] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const agentParam = searchParams.get("agent");

  const reload = useCallback(async () => {
    setError(null);
    try {
      const [agentResult, teamResult] = await Promise.all([listAgents(), listTeams()]);
      if (agentResult.ok) setAgents(agentResult.data); else setError(agentResult.error);
      if (teamResult.ok) setTeams(teamResult.data); else setError(teamResult.error);
    } catch (cause) { setError(String(cause)); }
  }, []);
  useEffect(() => { void reload(); }, [reload]);
  useEffect(() => { setSelectedId(agentParam || "default"); }, [agentParam]);
  const selectedAgent = useMemo(() => agents?.find(agent => agent.id === selectedId) ?? null, [agents, selectedId]);
  const query = search.trim().toLowerCase();
  const filteredAgents = agents?.filter(agent => `${agent.name} ${agent.description}`.toLowerCase().includes(query)) ?? [];
  const showDefault = !query || "默认角色 日常工作伙伴 小涟".includes(query);
  const selectAgent = (id: string) => { setSelectedId(id); setSearchParams({ agent: id }); };
  const handleOpenCreate = () => { setCreateError(null); setShowCreate(true); };
  const handleCreate = async () => {
    if (!name.trim()) return;
    setCreating(true); setCreateError(null);
    try {
      const result = await createAgent({ name: name.trim(), description: description.trim() || undefined, user_prompt: instructions.trim() || undefined });
      if (result.ok) {
        setShowCreate(false); setName(""); setDescription(""); setInstructions("");
        showToast("智能体已创建", "success"); await reload(); selectAgent(result.data.id);
      } else setCreateError(result.error);
    } catch (cause) { setCreateError(String(cause)); }
    finally { setCreating(false); }
  };

  return <PageShell variant="workspace" title="智能体" description="为不同工作选择合适的角色与表达方式。"
    className="capability-page page-canvas ui-opt-capabilities ui-opt-agents management-agents-page"
    contentClassName="management-agents-content"
    actions={<Button onClick={handleOpenCreate}><Plus className="h-4 w-4" />新建智能体</Button>}>
    {error && <ErrorState title="智能体暂时无法加载" description={error} action={<Button variant="secondary" size="sm" onClick={() => void reload()}>重试</Button>} />}
    <div className="management-agents-layout">
      <aside className="management-agent-list" aria-label="角色列表">
        <label className="management-agent-search"><Search size={17} aria-hidden="true" /><input aria-label="搜索智能体" placeholder="搜索智能体…" value={search} onChange={event => setSearch(event.target.value)} /></label>
        <div className="management-agent-list-scroll scrollbar-thin" role="listbox" aria-label="智能体列表">
          {showDefault && <button type="button" className="management-agent-row" role="option" aria-selected={selectedId === "default"} onClick={() => selectAgent("default")}>
            <span className="management-agent-avatar management-agent-default" aria-hidden="true"><Bot size={21} /></span>
            <span className="management-agent-row-copy"><strong>默认角色</strong><small>日常对话与工作的伙伴</small></span><ChevronRight size={16} aria-hidden="true" />
          </button>}
          {agents === null && !error && <div className="space-y-3 p-2" aria-label="正在加载智能体"><Skeleton className="h-20 w-full" /><Skeleton className="h-20 w-full" /></div>}
          {filteredAgents.map(agent => <button type="button" key={agent.id} className="management-agent-row" role="option" aria-selected={selectedId === agent.id} onClick={() => selectAgent(agent.id)}>
            <span className="management-agent-avatar" aria-hidden="true"><Bot size={21} /></span>
            <span className="management-agent-row-copy"><strong>{agent.name}</strong><small>{agent.description || "自定义工作角色"}</small></span><ChevronRight size={16} aria-hidden="true" />
          </button>)}
          {agents !== null && !showDefault && filteredAgents.length === 0 && <EmptyState title="没有匹配的智能体" description="换个名称或用途关键词试试。" />}
        </div>
        <details className="management-agent-teams"><summary><Users size={15} aria-hidden="true" />团队 <span>{teams?.length ?? 0}</span></summary>
          <div>{teams === null ? <p>团队列表尚未加载。</p> : teams.length === 0 ? <p>暂无团队。团队用于有界的顺序协作。</p> : teams.map(team => <article key={team.id}><strong>{team.name}</strong><p>{team.description || "暂无描述"}</p><small>成员 {team.member_agent_ids.length} · 最大深度 {team.max_depth} · 最大执行 {team.max_agent_executions}</small></article>)}</div>
        </details>
      </aside>
      <section className="management-agent-editor scrollbar-thin" aria-label="当前角色编辑区">
        {selectedId === "default" || selectedAgent ? <PersonaEditor key={selectedId} agentId={selectedId} agentName={selectedAgent?.name || "默认角色"} onSaved={() => void reload()}>
          <details className="management-disclosure"><summary>能力与资料</summary><div className="management-disclosure-body">
            {selectedAgent ? <><p>当前定义中的工具与能力</p><div className="capability-token-list">{selectedAgent.allowed_tools.map(tool => <span key={tool}>{tool}</span>)}{selectedAgent.capabilities.map(capability => <span key={capability}>{capability}</span>)}{selectedAgent.allowed_tools.length + selectedAgent.capabilities.length === 0 && <span>未配置专属工具与能力</span>}</div></> : <p>使用当前默认运行配置。能力与资料的绑定在画布中管理。</p>}
            <p>这里仅查看配置；能力、资料及授权继续使用现有管理入口。</p>
          </div></details>
          <details className="management-disclosure"><summary>模型与运行配置</summary><div className="management-disclosure-body">
            {selectedAgent ? <AgentRuntimeDetails agent={selectedAgent} /> : <p>使用设置中的默认模型与运行配置。</p>}
          </div></details>
        </PersonaEditor> : <EmptyState title="选择一个智能体" description="在左侧选择角色后编辑名称、风格与用户提示词。" />}
      </section>
    </div>
      <Modal
        open={showCreate}
        onClose={() => {
          if (!creating) setShowCreate(false);
        }}
        title="新建智能体"
      >
        <div className="space-y-4">
          {createError && (
            <p
              role="alert"
              className="rounded-lg border border-[var(--danger)]/25 bg-[var(--danger)]/5 px-3 py-2 text-sm text-[var(--danger)]"
            >
              {createError}
            </p>
          )}

          <Section title="名称与用途">
            <div className="space-y-3">
              <FormField label="名称" required error={!name.trim() && createError ? "名称不可为空" : undefined}>
                <Input
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  placeholder="例如：research-assistant"
                  disabled={creating}
                  autoFocus
                />
              </FormField>
              <FormField label="用途描述">
                <Input
                  value={description}
                  onChange={(event) => setDescription(event.target.value)}
                  placeholder="简要说明该 Agent 的职责范围（可选）"
                  disabled={creating}
                />
              </FormField>
            </div>
          </Section>

          <Section title="用户提示词">
            <FormField label="用户提示词" hint="你为该智能体设定的职责、偏好与表达方式">
              <Textarea
                value={instructions}
                onChange={(event) => setInstructions(event.target.value)}
                rows={4}
                placeholder="该 Agent 的职责与约束..."
                disabled={creating}
              />
            </FormField>
          </Section>

          <div className="flex justify-end gap-2 pt-2">
            <Button
              variant="ghost"
              onClick={() => setShowCreate(false)}
              disabled={creating}
            >
              取消
            </Button>
            <Button
              onClick={handleCreate}
              disabled={!name.trim() || creating}
              loading={creating}
              loadingLabel="创建中…"
            >
              创建
            </Button>
          </div>
        </div>
      </Modal>
    </PageShell>;
}

function AgentRuntimeDetails({ agent }: { agent: AgentDefinition }) {
  return <dl className="capability-meta-list">
    <CapabilityMetaRow label="模型" value={agent.model || "使用默认配置"} mono />
    <CapabilityMetaRow label="最大迭代" value={String(agent.max_iterations)} />
    <CapabilityMetaRow label="来源" value={formatAgentSource(agent.source)} />
    <CapabilityMetaRow label="Agent ID" value={agent.id} mono />
    <div className="flex items-center justify-between gap-3"><dt>定义状态</dt><dd><CapabilityStatusBadge label={agent.enabled ? "已启用" : "已禁用"} tone={agent.enabled ? "success" : "default"} /></dd></div>
  </dl>;
}
