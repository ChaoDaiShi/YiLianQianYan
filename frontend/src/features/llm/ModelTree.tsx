import { GitBranch, TreePine } from "lucide-react";
import type { LlmModel } from "../../types";
import { buildModelTree, providerLabel } from "./llmModelUtils";

export default function ModelTree({ models }: { models: LlmModel[] }) {
  const nodes = buildModelTree(models);
  return (
    <div className="rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-3" data-testid="llm-model-tree">
      <div className="mb-2 flex items-center justify-between text-xs text-[var(--text-secondary)]">
        <span className="inline-flex items-center gap-1.5"><TreePine className="h-3.5 w-3.5 text-[var(--accent-primary)]" />模型森林</span>
        <span>{models.length} / 32 枝</span>
      </div>
      {nodes.length === 0 ? (
        <div className="flex h-48 items-center justify-center text-xs text-[var(--text-muted)]">验证模型后，这里会长出第一根枝条</div>
      ) : (
        <svg viewBox="0 0 1000 420" className="h-64 w-full" role="img" aria-label="已配置模型树">
          <path d="M500 398 C500 335 500 292 500 232" fill="none" stroke="var(--accent-primary)" strokeWidth="12" strokeLinecap="round" opacity="0.8" />
          {nodes.map((node) => (
            <g key={node.id}>
              <path d={`M500 235 Q${node.x} ${node.y + 45} ${node.x} ${node.y + 16}`} fill="none" stroke={node.active ? "var(--accent-primary)" : "var(--border-strong)"} strokeWidth={node.active ? 3 : 2} strokeLinecap="round" opacity="0.75" />
              <circle cx={node.x} cy={node.y} r={node.active ? 16 : 13} fill={node.active ? "var(--accent-primary)" : "var(--surface-hover)"} stroke={node.active ? "var(--accent-primary)" : "var(--border-strong)"} strokeWidth="2" />
              <text x={node.x} y={node.y + 4} textAnchor="middle" fontSize="10" fill={node.active ? "var(--accent-fg)" : "var(--text)"}>{node.provider.slice(0, 2).toUpperCase()}</text>
              <text x={node.x} y={node.y + 32} textAnchor="middle" fontSize="11" fill="var(--text)" className="font-medium">{node.label.length > 10 ? `${node.label.slice(0, 10)}…` : node.label}</text>
              <text x={node.x} y={node.y + 47} textAnchor="middle" fontSize="9" fill="var(--text-muted)">{providerLabel(node.provider)}</text>
            </g>
          ))}
          <g>
            <circle cx="500" cy="220" r="19" fill="var(--accent-primary)" />
            <GitBranch x="489" y="209" width="22" height="22" color="var(--accent-fg)" />
            <text x="500" y="414" textAnchor="middle" fontSize="12" fill="var(--text-secondary)">当前模型根系</text>
          </g>
        </svg>
      )}
    </div>
  );
}
