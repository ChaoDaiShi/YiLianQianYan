import { useCallback, useEffect, useState } from "react";
import { Bot, Plus, Users } from "lucide-react";
import {
  Button,
  Badge,
  Input,
  Textarea,
  Modal,
  PageHeader,
  Panel,
  EmptyState,
  Spinner,
} from "../components/ui";
import {
  AgentDefinition,
  AgentTeam,
  listAgents,
  createAgent,
  listTeams,
} from "../api/client";

export default function AgentsPage() {
  const [agents, setAgents] = useState<AgentDefinition[] | null>(null);
  const [teams, setTeams] = useState<AgentTeam[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [instructions, setInstructions] = useState("");

  const reload = useCallback(async () => {
    const [a, t] = await Promise.all([listAgents(), listTeams()]);
    if (a.ok) setAgents(a.data);
    else setError(a.error);
    if (t.ok) setTeams(t.data);
    else setError(t.error);
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const handleCreate = async () => {
    if (!name.trim()) return;
    const res = await createAgent({ name, instructions });
    if (res.ok) {
      setShowCreate(false);
      setName("");
      setInstructions("");
      void reload();
    } else {
      setError(res.error);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Agents / Teams"
        description="管理 Agent 定义与多智能体团队。Agent 定义只描述能力，实际执行仍受安全网关约束。"
        actions={
          <Button onClick={() => setShowCreate(true)}>
            <Plus className="h-4 w-4" />
            新建 Agent
          </Button>
        }
      />

      <div className="mx-4 mt-4 flex-1 overflow-y-auto pb-4">
        {error && <p className="mb-3 text-sm text-[var(--danger)]">{error}</p>}

        <h3 className="mb-2 flex items-center gap-1.5 text-sm font-semibold text-[var(--text)]">
          <Bot className="h-4 w-4" />
          Agent 定义
        </h3>
        {agents === null ? (
          <div className="flex justify-center py-8">
            <Spinner />
          </div>
        ) : agents.length === 0 ? (
          <EmptyState title="暂无 Agent" description="创建第一个 Agent 定义。" />
        ) : (
          <div className="mb-5 grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {agents.map((agent) => (
              <Panel key={agent.id}>
                <div className="flex items-start justify-between gap-2">
                  <h4 className="font-medium text-[var(--text)]">{agent.name}</h4>
                  <Badge tone={agent.enabled ? "success" : "default"}>
                    {agent.enabled ? "启用" : "禁用"}
                  </Badge>
                </div>
                {agent.description && (
                  <p className="mt-1 line-clamp-2 text-sm text-[var(--text-muted)]">
                    {agent.description}
                  </p>
                )}
                <p className="mt-2 text-xs text-[var(--text-faint)]">
                  来源：{agent.source} · 最大迭代：{agent.max_iterations}
                </p>
                {agent.allowed_tools.length > 0 && (
                  <p className="mt-1 text-xs text-[var(--text-faint)]">
                    工具：{agent.allowed_tools.join("、")}
                  </p>
                )}
              </Panel>
            ))}
          </div>
        )}

        <h3 className="mb-2 flex items-center gap-1.5 text-sm font-semibold text-[var(--text)]">
          <Users className="h-4 w-4" />
          团队
        </h3>
        {teams === null ? (
          <div className="flex justify-center py-8">
            <Spinner />
          </div>
        ) : teams.length === 0 ? (
          <EmptyState title="暂无团队" description="团队用于有界的多智能体顺序协作。" />
        ) : (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {teams.map((team) => (
              <Panel key={team.id}>
                <h4 className="font-medium text-[var(--text)]">{team.name}</h4>
                {team.description && (
                  <p className="mt-1 line-clamp-2 text-sm text-[var(--text-muted)]">
                    {team.description}
                  </p>
                )}
                <p className="mt-2 text-xs text-[var(--text-faint)]">
                  成员：{team.member_agent_ids.length} · 最大深度：{team.max_depth} · 最大执行：
                  {team.max_agent_executions}
                </p>
              </Panel>
            ))}
          </div>
        )}
      </div>

      <Modal open={showCreate} onClose={() => setShowCreate(false)} title="新建 Agent">
        <div className="space-y-3">
          <Input label="名称" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
          <Textarea
            label="指令"
            value={instructions}
            onChange={(e) => setInstructions(e.target.value)}
            rows={4}
            placeholder="该 Agent 的职责与约束（LLM-only，不直接调用工具）"
          />
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setShowCreate(false)}>
              取消
            </Button>
            <Button onClick={handleCreate} disabled={!name.trim()}>
              创建
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}
