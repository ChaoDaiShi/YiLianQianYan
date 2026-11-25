import type { CapabilityDescriptor } from "../../api/capabilities";

const contexts = [
  { id: "chat", label: "对话" },
  { id: "task_node", label: "任务节点" },
  { id: "personal", label: "个人委托" },
] as const;

export default function CapabilityAvailabilityView({ capability }: { capability: Pick<CapabilityDescriptor, "availability"> }) {
  const { availability } = capability;
  if (!availability) return <p className="text-xs text-[var(--text-secondary)]">当前目录未提供执行环境信息。实际执行仍须检查权限与参数。</p>;

  return (
    <section className="capability-detail-section capability-availability" aria-label="按执行环境可用性">
      <h3>按执行环境可用性</h3>
      <dl>
        {contexts.map(({ id, label }) => (
          <div key={id}>
            <dt>{label}</dt>
            <dd>{availability[id].executable ? "支持执行" : "当前不支持执行"}{availability[id].executable && availability[id].requires_approval && " · 可能需要审批"}</dd>
          </div>
        ))}
      </dl>
      <p>执行时仍检查主体、参数与当前授权，这里不会新增权限。</p>
      <details>
        <summary>查看发现、绑定与限制</summary>
        <dl>
          {contexts.map(({ id, label }) => {
            const context = availability[id];
            return (
              <div key={id}>
                <dt>{label}</dt>
                <dd>
                  {context.discoverable ? "可发现" : "不提供"} · {context.bindable ? "可绑定" : "不可绑定"}
                  {context.reasons.map(reason => <p key={reason}>{reason}</p>)}
                </dd>
              </div>
            );
          })}
        </dl>
      </details>
    </section>
  );
}
