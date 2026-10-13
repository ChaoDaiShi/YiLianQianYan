const identifierTypes = {
  artifact: { prefix: "ART", label: "产物编号" },
  graph: { prefix: "TG", label: "任务图编号" },
  task: { prefix: "TK", label: "任务编号" },
  workflow: { prefix: "WF", label: "工作流编号" },
  team: { prefix: "AT", label: "智能体团队编号" },
  node: { prefix: "ND", label: "节点编号" },
  execution: { prefix: "EX", label: "执行编号" },
  goal: { prefix: "GL", label: "目标编号" },
  report: { prefix: "RP", label: "验收报告编号" },
  evidence: { prefix: "EV", label: "证据编号" },
  repair: { prefix: "FX", label: "修复提案编号" },
  checkpoint: { prefix: "CP", label: "检查点编号" },
  snapshot: { prefix: "SP", label: "快照编号" },
  resource: { prefix: "RS", label: "资源编号" },
  deliverable: { prefix: "DL", label: "交付物编号" },
  criterion: { prefix: "CR", label: "验收条件编号" },
  digest: { prefix: "SHA256", label: "SHA-256 摘要" },
} as const;

export type IdentifierKind = keyof typeof identifierTypes;
export interface IdentifierReference { kind: IdentifierKind; value: string }

export function identifierLabel(kind: IdentifierKind): string {
  return identifierTypes[kind].label;
}

/** A presentation code only; actions and copying keep the original identity. */
export function formatIdentifier(kind: IdentifierKind, value: string): string {
  const uuid = value.match(/(?:^|[-_])([0-9a-f]{8})-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i);
  let code: string;
  if (uuid) code = uuid[1];
  else if (kind === "digest" && /^[0-9a-f]{64}$/i.test(value)) code = value.slice(0, 8);
  else {
    let hash = 0x811c9dc5;
    for (let i = 0; i < value.length; i++) hash = Math.imul(hash ^ value.charCodeAt(i), 0x01000193);
    code = (hash >>> 0).toString(16).padStart(8, "0");
  }
  return `${identifierTypes[kind].prefix}-${code.toUpperCase()}`;
}

/** Format identifiers in explanatory text, without changing the underlying record. */
export function formatIdentifierText(text: string, references: readonly IdentifierReference[]): string {
  const unique = new Map(references.filter(item => item.value).map(item => [item.value, item.kind]));
  const values = [...unique.keys()].sort((a, b) => b.length - a.length);
  if (values.length === 0) return text;
  const pattern = values.map(value => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|");
  return text.replace(new RegExp(pattern, "g"), (value: string, offset: number) => {
    const idCharacter = /[A-Za-z0-9_.-]/;
    if (idCharacter.test(text[offset - 1] || "") || idCharacter.test(text[offset + value.length] || "")) return value;
    return formatIdentifier(unique.get(value)!, value);
  });
}
