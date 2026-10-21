export interface ConversationCanvasMatch { canvas: { id: string }; hasGraph: boolean; hasBinding: boolean }
export interface ConversationCanvasAttachment {
  bind: (canvas: string, binding: { kind: "conversation_agent"; source_id: string; target_id: string }) => Promise<unknown>;
  createGraph: (canvas: string, conversation: string) => Promise<unknown>;
}
export function chooseConversationCanvas(matches: ConversationCanvasMatch[], active: string | null): string | null {
  const graphs = matches.filter(match => match.hasGraph);
  const targets = graphs.length > 0 ? graphs : matches.filter(match => match.hasBinding);
  return (targets.find(match => match.canvas.id === active) || targets[0])?.canvas.id ?? null;
}
export async function connectConversationToCanvas(canvas: string, agent: string, conversation: string, dependencies: ConversationCanvasAttachment): Promise<void> {
  await dependencies.bind(canvas, { kind: "conversation_agent", source_id: `conversation:${conversation}`, target_id: `agent:${agent}` });
  await dependencies.createGraph(canvas, conversation);
}
