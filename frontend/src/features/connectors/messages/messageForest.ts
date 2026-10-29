import { layoutMessageTree, type MessagePosition } from "./messageTree";

interface ConversationEntry {
  id: string;
  title: string;
  nodes: Array<{ id: string; parent_id: string | null }>;
  branch: { parent_node_id: string } | null;
}

export interface ConversationFamily { id: string; title: string; conversationIds: string[] }

/** A fork is another path in the same family, while its execution/history ID stays intact. */
export function conversationFamilies(conversations: ConversationEntry[]): ConversationFamily[] {
  const owner = new Map(conversations.flatMap(conversation => conversation.nodes.map(node => [node.id, conversation.id] as const)));
  const parent = new Map(conversations.flatMap(conversation => {
    const source = conversation.branch && owner.get(conversation.branch.parent_node_id);
    return source ? [[conversation.id, source] as const] : [];
  }));
  const families = new Map<string, ConversationFamily>();
  const byId = new Map(conversations.map(conversation => [conversation.id, conversation]));
  for (const conversation of conversations) {
    let root = conversation.id;
    while (parent.has(root)) root = parent.get(root)!;
    const family = families.get(root) ?? { id: root, title: byId.get(root)!.title, conversationIds: [] };
    family.conversationIds.push(conversation.id);
    families.set(root, family);
  }
  return [...families.values()];
}

export function layoutMessageForest(conversations: ConversationEntry[]) {
  return layoutMessageTree(conversations.flatMap(conversation => conversation.nodes));
}

/** Separate groups share a viewport, never a collision or layout space. */
export function layoutConversationGroups(conversations: ConversationEntry[]): Record<string, MessagePosition> {
  return Object.assign({}, ...conversationFamilies(conversations).map(family =>
    layoutMessageForest(conversations.filter(entry => family.conversationIds.includes(entry.id)))
  ));
}

export function conversationGroupEntries<T extends ConversationEntry>(conversations: T[], familyId: string): T[] {
  const family = conversationFamilies(conversations).find(entry => entry.id === familyId);
  return conversations.filter(entry => family?.conversationIds.includes(entry.id));
}
