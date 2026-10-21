import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
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
import PersonaDialog from "../features/agents/PersonaDialog";
import PageShell from "../components/layout/PageShell";
import {
  Button,
  EmptyState,
  ErrorState,
  FormField,
  Input,
  Modal,
  Section,
  showToast,
  Skeleton,
  Textarea,
} from "../components/ui";

export default function AgentsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [agents, setAgents] = useState<AgentDefinition[] | null>(null);
  const [teams, setTeams] = useState<AgentTeam[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [personaAgentId, setPersonaAgentId] = useState<string | null>(null);
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
    if (agentParam && agents?.some((agent) => agent.id === agentParam)) {
      setSelectedId(agentParam);
      return;
    }
    if (selectedId && agents?.some((agent) => agent.id === selectedId)) return;
    setSelectedId(agents?.[0]?.id ?? null);
  }, [agentParam, agents, selectedId]);

  const selectedAgent = useMemo(
    () => agents?.find((agent) => agent.id === selectedId) ?? null,
    [agents, selectedId],
  );

  const handleOpenCreate = () => {
    setCreateError(null);
    setShowCreate(true);
  };

  const handleCreate = async () => {
    if (!name.trim()) return;
    setCreating(true);
    setCreateError(null);
    try {
      const result = await createAgent({
        name: name.trim(),
        description: description.trim() || undefined,
        user_prompt: instructions.trim() || undefined,
      });
      if (result.ok) {
        setShowCreate(false);
        setName("");
        setDescription("");
        setInstructions("");
        showToast("智能体已创建", "success");
        await reload();
        setSelectedId(result.data.id);
        setSearchParams({ agent: result.data.id });
      } else {
        setCreateError(result.error);
      }
    } catch (cause) {
      setCreateError(String(cause));
    } finally {
      setCreating(false);
    }
  };

  return (
    <PageShell
      title="智能体"
      description="管理承担不同角色和任务的智能体配置。"
      className="capability-page page-canvas ui-opt-capabilities ui-opt-agents"
      actions={
        <><Button variant="secondary" onClick={() => setPersonaAgentId("default")}>默认角色</Button><Button onClick={handleOpenCreate}>
          <Plus className="h-4 w-4" />
          新建 Agent
        </Button></>
      }
    >
      <div className="capability-page-body capability-page-body-stack">
        {error && (
          <ErrorState
            title="智能体暂时无法加载"
            description={error}
            action={
              <Button variant="secondary" size="sm" onClick={() => void reload()}>
                重试
              </Button>
            }
          />
        )}

        <section aria-labelledby="agent-list-title">
          <div className="capability-section-heading">
            <h2 id="agent-list-title">
              <Bot className="h-4 w-4" />
              Agent 定义
            </h2>
            <span>{agents?.length ?? 0} 个</span>
          </div>

          {agents === null && !error ? (
            <div className="capability-card-grid agent-card-grid" aria-label="正在加载智能体">
              <Skeleton className="h-36 w-full" />
              <Skeleton className="h-36 w-full" />
              <Skeleton className="h-36 w-full" />
            </div>
          ) : agents && agents.length === 0 ? (
            <EmptyState
              title="还没有其他智能体"
              description="创建后，可以让不同智能体负责不同类型的任务。"
              action={
                <Button size="sm" onClick={handleOpenCreate}>
                  新建 Agent
                </Button>
              }
            />
          ) : agents ? (
            <div
              className="capability-card-grid agent-card-grid"
              role="listbox"
              aria-label="智能体列表"
              aria-activedescendant={selectedId ? `agent-option-${selectedId}` : undefined}
            >
              {agents.map((agent) => {
                const isSelected = agent.id === selectedId;
                const allCapabilities = [...agent.allowed_tools, ...agent.capabilities];
                const displayedCapabilities = allCapabilities.slice(0, 3);
                const remainingCapabilities = allCapabilities.length - displayedCapabilities.length;
                const modelDisplay = agent.model || "使用默认配置";

                return (
                  <button
                    type="button"
                    id={`agent-option-${agent.id}`}
                    role="option"
                    key={agent.id}
                    aria-selected={isSelected}
                    tabIndex={isSelected ? 0 : -1}
                    onClick={() => {
                      setSelectedId(agent.id);
                      setSearchParams({ agent: agent.id });
                    }}
                    className={`capability-card agent-card capability-card-selectable ${
                      isSelected ? "capability-card-selected agent-card-selected" : ""
                    }`}
                  >
                    <div className="flex items-start gap-3 w-full">
                      <div className="agent-role-icon shrink-0" aria-hidden="true">
                        <Bot className="h-5 w-5" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center justify-between gap-2">
                          <strong className="text-sm font-semibold text-[var(--text)] line-clamp-2 leading-snug">
                            {agent.name}
                          </strong>
                          <CapabilityStatusBadge
                            label={agent.enabled ? "已启用" : "已禁用"}
                            tone={agent.enabled ? "success" : "default"}
                          />
                        </div>
                        <p className="text-xs text-[var(--text-muted)] line-clamp-1 mt-0.5">
                          {agent.description || "暂无描述"}
                        </p>
                        <p className="text-[11px] text-[var(--text-faint)] mt-1 font-mono">
                          模型：{modelDisplay}
                        </p>
                      </div>
                    </div>

                    <div className="agent-instructions-inset w-full">
                      {agent.user_prompt?.trim() || "未配置专属指令"}
                    </div>

                    <div className="flex items-center justify-between gap-2 w-full pt-1 border-t border-[var(--border-soft)] text-[11px] text-[var(--text-muted)]">
                      <span>{formatAgentSource(agent.source)} · 最大迭代 {agent.max_iterations}</span>
                      <div className="flex items-center gap-1">
                        {displayedCapabilities.map((item) => (
                          <span
                            key={item}
                            className="inline-block max-w-[80px] truncate rounded bg-[var(--surface-muted)] px-1.5 py-0.5 text-[10px] text-[var(--text-secondary)] border border-[var(--border-soft)]"
                          >
                            {item}
                          </span>
                        ))}
                        {remainingCapabilities > 0 && (
                          <span
                            className="agent-badge-overflow"
                            title={`查看剩余 ${remainingCapabilities} 项工具与能力`}
                            onClick={(e) => {
                              e.stopPropagation();
                              setSelectedId(agent.id);
                            }}
                          >
                            +{remainingCapabilities}
                          </span>
                        )}
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          ) : null}
        </section>

        <section className="capability-detail-panel" aria-label="智能体详情">
          {selectedAgent ? (
            <><div className="flex justify-end p-3"><Button variant="secondary" onClick={() => setPersonaAgentId(selectedAgent.id)}>编辑角色与用户提示词</Button></div><AgentDetail agent={selectedAgent} /></>
          ) : (
            <EmptyState
              icon={<Bot className="h-6 w-6" />}
              title="选择一个智能体"
              description="查看真实的 Agent 配置字段。"
              className="min-h-[300px]"
            />
          )}
        </section>

        <section aria-labelledby="team-list-title">
          <div className="capability-section-heading">
            <h2 id="team-list-title">
              <Users className="h-4 w-4" />
              团队
            </h2>
            <span>{teams?.length ?? 0} 个</span>
          </div>

          {teams === null && !error ? (
            <div className="capability-card-grid" aria-label="正在加载团队">
              <Skeleton className="h-24 w-full" />
              <Skeleton className="h-24 w-full" />
            </div>
          ) : teams && teams.length === 0 ? (
            <EmptyState
              title="暂无团队"
              description="团队用于有界的多智能体顺序协作。"
              className="py-10"
            />
          ) : teams ? (
            <div className="capability-card-grid">
              {teams.map((team) => (
                <article key={team.id} className="capability-card">
                  <div className="capability-card-heading">
                    <span className="capability-icon">
                      <Users className="h-4 w-4" />
                    </span>
                    <strong>{team.name}</strong>
                  </div>
                  <p className="capability-card-description">{team.description || "暂无描述"}</p>
                  <div className="capability-card-meta">
                    成员 {team.member_agent_ids.length} · 最大深度 {team.max_depth} · 最大执行 {team.max_agent_executions}
                  </div>
                </article>
              ))}
            </div>
          ) : null}
        </section>
      </div>

      <Modal
        open={showCreate}
        onClose={() => {
          if (!creating) setShowCreate(false);
        }}
        title="新建 Agent"
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
      <PersonaDialog open={personaAgentId !== null} agentId={personaAgentId ?? "default"} onClose={() => setPersonaAgentId(null)} onSaved={() => void reload()} />
    </PageShell>
  );
}

function AgentDetail({ agent }: { agent: AgentDefinition }) {
  const [full, setFull] = useState(false);
  return (
    <div className="capability-detail-content">
      <div className="capability-detail-heading">
        <div>
          <div className="capability-detail-title-line">
            <Bot className="h-5 w-5 text-[var(--accent-primary)]" />
            <h2>{agent.name}</h2>
            <CapabilityStatusBadge
              label={agent.enabled ? "已启用" : "已禁用"}
              tone={agent.enabled ? "success" : "default"}
            />
          </div>
          <p className="capability-detail-description">{agent.description || "暂无描述"}</p>
        </div>
      </div>

      <CapabilityDetailSection title="配置概览">
        <dl className="capability-meta-list">
          <CapabilityMetaRow label="来源" value={formatAgentSource(agent.source)} />
          <CapabilityMetaRow label="模型" value={agent.model || "使用默认配置"} mono />
          <CapabilityMetaRow label="最大迭代" value={String(agent.max_iterations)} />
          <CapabilityMetaRow label="状态" value={agent.enabled ? "已启用" : "已禁用"} />
        </dl>
      </CapabilityDetailSection>

      <CapabilityDetailSection title="工具与能力">
        <div className="capability-token-list">
          {agent.allowed_tools.length > 0
            ? agent.allowed_tools.map((tool) => <span key={tool}>{tool}</span>)
            : <span>未配置工具</span>}
          {agent.capabilities.length > 0 &&
            agent.capabilities.map((capability) => <span key={capability}>{capability}</span>)}
        </div>
      </CapabilityDetailSection>

      <CapabilityDetailSection title="用户提示词" technical>
        <pre className="capability-content-preview">{agent.user_prompt?.slice(0, 1200) || "暂无用户提示词"}</pre><Button variant="ghost" size="sm" onClick={() => setFull(true)}>查看更多</Button><Modal open={full} onClose={() => setFull(false)} title="用户提示词全文" size="xl"><pre className="whitespace-pre-wrap break-words">{agent.user_prompt}</pre></Modal>
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
