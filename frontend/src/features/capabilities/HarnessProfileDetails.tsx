import { Link } from "react-router-dom";
import type { HarnessProfile, HarnessSnapshot } from "../../api/harnesses";
import { Badge, Button, Modal } from "../../components/ui";
import { formatToolDisplayName } from "../../components/chat/toolDisplay";

function capabilityPath(category: "skill" | "tool" | "workflow" | "plugin", id: string) {
  return `/capabilities?${new URLSearchParams({ category, [category]: id })}`;
}

export default function HarnessProfileDetails({ profile, snapshot, onClose }: {
  profile: HarnessProfile;
  snapshot: HarnessSnapshot;
  onClose: () => void;
}) {
  const { catalog, system_groups, settings } = snapshot;
  const systemDefault = profile.id === "default";
  const active = settings.active_id === profile.id;
  const registered = new Set(catalog.tools.map(tool => tool.id));
  const groups = [...system_groups, ...settings.groups];
  const groupIds = new Set(profile.tool_group_ids);
  const tools = new Set(systemDefault ? registered : groups.filter(group => groupIds.has(group.id)).flatMap(group => group.tools));
  const agent = catalog.agents.find(item => item.id === profile.agent_id);
  const pluginAllowed = (pluginId: string | null) => !pluginId || systemDefault || profile.plugin_ids.includes(pluginId);
  const agentReady = !profile.agent_id || Boolean(agent && pluginAllowed(agent.plugin_id));
  const chatToolAllowed = (id: string) => agentReady && tools.has(id) && registered.has(id) && (!agent || agent.allowed_tools.includes(id));
  const workflows = systemDefault ? catalog.workflows.map(item => item.id) : profile.workflow_ids;
  const skills = profile.skill_ids ?? catalog.skills.filter(skill => pluginAllowed(skill.plugin_id)).map(skill => skill.id);

  return <Modal open title={profile.name} onClose={onClose} size="lg" footer={<Button variant="secondary" onClick={onClose}>关闭</Button>}>
    <div className="min-w-0 space-y-4 text-sm">
      <div className="flex flex-wrap gap-2">
        <Badge tone="default">{systemDefault ? "系统内置" : "已配置"}</Badge>
        {active && <Badge tone="info">当前组合</Badge>}
      </div>
      <p className="whitespace-pre-wrap break-words">{profile.description ||
        (systemDefault ? "使用当前运行时注册的工具与可用技能，适用于未指定能力范围的对话和工作流。" : "组合中包含的技能、工具和工作流适用于新的对话轮次和工作流运行。")}</p>
      <p className="text-[var(--text-secondary)]">智能体负责决定如何调用工具。技能提供可加载的操作步骤，工作流按节点顺序执行。工具权限按工作空间、参数和当次调用核验。</p>
      <section className="space-y-1 border-t border-[var(--border-soft)] pt-3">
        <h4 className="font-medium">对话智能体</h4>
        {agent ? <p><Link className="underline" to={`/agents?agent=${encodeURIComponent(agent.id)}`}>{agent.name}</Link> · {agentReady ? "已绑定" : "来源插件未纳入此组合"}</p>
          : <p>{profile.agent_id ? "绑定的智能体当前已停用或不可用" : "使用默认智能体"}</p>}
      </section>
      <section className="space-y-2 border-t border-[var(--border-soft)] pt-3">
        <h4 className="font-medium">工具与权限</h4>
        {!tools.size ? <p className="text-[var(--text-secondary)]">未选工具组，组合没有工具入口。</p> :
          <details>
            <summary className="focus-ring-token cursor-pointer">{[...tools].filter(id => registered.has(id)).length} 个已注册 · {[...tools].filter(chatToolAllowed).length} 个在组合对话范围内</summary>
            <ul className="mt-2 grid gap-2 sm:grid-cols-2">
              {[...tools].sort().map(id => <li key={id} className="min-w-0 break-words">
                {registered.has(id) ? <Link className="underline" to={capabilityPath("tool", id)}>{formatToolDisplayName(id)}</Link> : formatToolDisplayName(id)}
                <span className="text-[var(--text-secondary)]"> · {chatToolAllowed(id) ? "对话范围内" : registered.has(id) ? "受智能体范围限制" : "当前未注册"}</span>
              </li>)}
            </ul>
          </details>}
        <p><Link className="underline" to="/settings?section=permissions">查看权限与审批</Link></p>
      </section>
      <section className="space-y-2 border-t border-[var(--border-soft)] pt-3">
        <h4 className="font-medium">技能</h4>
        {skills.length ? <>
          {!chatToolAllowed("load_skill") && <p role="status" className="text-[var(--warning-fg)]">组合或绑定的智能体未提供「加载技能」工具，对话无法加载以下技能。</p>}
          <details>
            <summary className="focus-ring-token cursor-pointer">查看 {skills.length} 个技能及来源</summary>
            <ul className="mt-2 grid gap-2 sm:grid-cols-2">{skills.map(id => {
              const skill = catalog.skills.find(item => item.id === id);
              return <li key={id} className="min-w-0 break-words">{skill ? <Link className="underline" to={capabilityPath("skill", id)}>{skill.name}</Link> : id}
                <span className="text-[var(--text-secondary)]"> · {!skill ? "当前不可用，请检查安装与运行依赖" : !pluginAllowed(skill.plugin_id) ? "来源插件未纳入组合" : "运行环境已就绪"}</span>
              </li>;
            })}</ul>
          </details>
        </> : <p className="text-[var(--text-secondary)]">{profile.skill_ids == null ? "当前组合没有可用技能。" : "此组合未选用技能。"}</p>}
      </section>
      {!systemDefault && <section className="space-y-2 border-t border-[var(--border-soft)] pt-3">
        <h4 className="font-medium">插件来源</h4>
        {profile.plugin_ids.length ? <ul className="space-y-1">{profile.plugin_ids.map(id => {
          const plugin = catalog.plugins.find(item => item.id === id);
          return <li key={id} className="break-words"><Link className="underline" to={capabilityPath("plugin", id)}>{plugin?.name ?? id}</Link> · {plugin ? "已安装并启用" : "当前未启用或未安装"}</li>;
        })}</ul> : <p className="text-[var(--text-secondary)]">未选用插件来源。</p>}
      </section>}
      <section className="space-y-2 border-t border-[var(--border-soft)] pt-3">
        <h4 className="font-medium">运行工作流</h4>
        {workflows.length ? <ul className="space-y-2">{workflows.map(id => {
          const workflow = catalog.workflows.find(item => item.id === id);
          const missingTools = workflow?.tool_ids.filter(tool => !tools.has(tool) || !registered.has(tool)) ?? [];
          const blocker = !workflow ? "工作流已移除" : !workflow.ready ? "工作流配置或来源不可用" :
            !pluginAllowed(workflow.plugin_id) ? "来源插件未纳入组合" :
            missingTools.length ? `工具缺失或超出组合范围：${missingTools.map(formatToolDisplayName).join("、")}` : "";
          return <li key={id} className="min-w-0 break-words">
            {workflow ? <Link className="underline" to={capabilityPath("workflow", id)}>{workflow.name}</Link> : id}
            {workflow?.description && <p className="text-[var(--text-secondary)]">{workflow.description}</p>}
            <p className={blocker ? "text-[var(--warning-fg)]" : "text-[var(--text-secondary)]"}>{blocker || "工作流定义与工具范围已就绪，执行时核验权限。"}</p>
          </li>;
        })}</ul> : <p className="text-[var(--text-secondary)]">组合中没有可运行的工作流。</p>}
      </section>
      <nav className="flex flex-wrap gap-3 border-t border-[var(--border-soft)] pt-3" aria-label="使用入口">
        {active && <Link className="underline" to="/chat">前往对话</Link>}
        <Link className="underline" to="/capabilities?category=workflow&workflow_tab=runtime">工作流与最近运行</Link>
        <Link className="underline" to="/logs">执行记录</Link>
      </nav>
    </div>
  </Modal>;
}
