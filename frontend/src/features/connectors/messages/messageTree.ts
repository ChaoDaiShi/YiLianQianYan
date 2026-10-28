export interface TreeEntry { id: string; parent_id: string | null }
export interface MessagePosition { x: number; y: number }
interface ReplyEntry extends TreeEntry { role: string; conversation_id: string; created_at: number }

export function nextAssistantParent(nodes: ReplyEntry[], options: { selectedId?: string | null; turnConversationId?: string | null; previousParentId?: string | null }): string | null {
  const answers = nodes.filter(node => node.role === "assistant").sort((a, b) => a.created_at - b.created_at);
  if (options.turnConversationId) {
    const produced = answers.filter(node => node.conversation_id === options.turnConversationId);
    if (produced.length) return produced[produced.length - 1].id;
  }
  const previous = answers.find(node => node.id === options.previousParentId);
  if (previous) return previous.id;
  const selected = answers.find(node => node.id === options.selectedId);
  if (selected) return selected.id;
  const parents = new Set(nodes.map(node => node.parent_id));
  const leaves = answers.filter(node => !parents.has(node.id));
  return leaves[leaves.length - 1]?.id ?? answers[answers.length - 1]?.id ?? null;
}

export function ancestorPath(nodes: TreeEntry[], selected: string | null): string[] {
  const byId = new Map(nodes.map(node => [node.id, node]));
  const path: string[] = [];
  let current = selected;
  while (current) {
    const node = byId.get(current);
    if (!node) break;
    path.unshift(current);
    current = node.parent_id;
  }
  return path;
}

export function layoutMessageTree(nodes: TreeEntry[]): Record<string, MessagePosition> {
  const children = new Map<string | null, TreeEntry[]>();
  for (const node of nodes) children.set(node.parent_id, [...(children.get(node.parent_id) ?? []), node]);
  const result: Record<string, MessagePosition> = {};
  function place(node: TreeEntry, column: number, depth: number): number {
    result[node.id] = { x: 48 + column * 324, y: 40 + depth * 192 };
    let nextColumn = column;
    for (const child of children.get(node.id) ?? []) nextColumn = place(child, nextColumn, depth + 1);
    return Math.max(column + 1, nextColumn);
  }
  let column = 0;
  for (const root of children.get(null) ?? []) column = place(root, column, 0) + 2;
  return result;
}

/** Preserve existing positions and allocate space for new messages without overlaps. */
export function placeNewMessages(automatic: Record<string, MessagePosition>, existing: Record<string, MessagePosition>): Record<string, MessagePosition> {
  const result = { ...existing };
  for (const [id, point] of Object.entries(automatic)) {
    if (result[id]) continue;
    const candidate = { ...point };
    while (Object.values(result).some(other => Math.abs(other.x - candidate.x) < 320 && Math.abs(other.y - candidate.y) < 188)) candidate.x += 324;
    result[id] = candidate;
  }
  return result;
}
