import type { GoalViewState } from "./goalViewModel";

export default function GoalBlockers({ state, loading = false }: { state: GoalViewState; loading?: boolean }) {
  return (
    <section className="goal-view-card" aria-label="待处理事项">
      <div className="goal-view-section-heading">
        <div>
          <h2>需要处理</h2>
          <p>来自当前执行状态和验收报告。</p>
        </div>
        <span className="goal-view-count">{state.blockers.length}</span>
      </div>
      {state.blockers.length ? (
        <ul className="goal-view-blocker-list">
          {state.blockers.map((blocker, index) => (
            <li key={`${blocker.code}:${blocker.nodeId || "goal"}:${index}`}>
              <strong>{blocker.nodeId ? "任务步骤" : "目标"}</strong>
              <span>{blocker.message}</span>
            </li>
          ))}
        </ul>
      ) : state.phase === "completed" ? (
        <p className="goal-view-empty">当前目标和交付物已通过验收。</p>
      ) : loading ? (
        <p className="goal-view-empty">正在读取目标和验收状态。</p>
      ) : state.phase === "planning" ? (
        <p className="goal-view-empty">此任务图尚未关联可验证目标。</p>
      ) : (
        <p className="goal-view-empty">当前没有报告待处理事项。</p>
      )}
    </section>
  );
}
