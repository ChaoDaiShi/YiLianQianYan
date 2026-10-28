import { connectorRequest, type ConnectorViewport } from "./connectors";
import { streamChatRequest, type EventHandler } from "./chat";
import type { AttachmentReference, ConversationOrigin } from "../types";
import type { ChatRequestOptions } from "../components/chat/chatRequestOptions";

export interface ConnectorMessage {
  attachments?:AttachmentReference[];
  id: string;
  parent_id: string | null;
  role: "user" | "assistant" | "process";
  content: string;
  conversation_id: string;
  created_at: number;
  status: string;
  error: string | null;
  reasoning?: string;
}
export interface MessageGraphLayout {
  nodes: Array<{ id: string; x: number; y: number }>;
  viewport: ConnectorViewport;
}
export interface ConnectorMessageGraph {
  canvas_id: string;
  conversation_id: string;
  selected_node_id: string | null;
  layout: MessageGraphLayout;
  nodes: ConnectorMessage[];
  runs: Array<{ conversation_id: string; parent_node_id: string | null; status: string; error: string | null }>;
}
export interface MessageSources {
  conversations: Array<{ id: string; title: string }>;
  graphs: Array<{ conversation_id: string }>;
}
export interface MessageCanvasConversation {
  origin?:ConversationOrigin;
  id: string;
  title: string;
  status: string;
  error: string | null;
  created_at: number;
  updated_at: number;
  nodes: ConnectorMessage[];
  branch: { parent_node_id: string } | null;
}
export interface MessageCanvasOverview {
  conversations: MessageCanvasConversation[];
  layout: MessageGraphLayout;
}
export const getMessageCanvas = () => connectorRequest<MessageCanvasOverview>("GET", "/api/message-canvas");
export const saveMessageCanvasLayout = (layout: MessageGraphLayout) => connectorRequest("PUT", "/api/message-canvas/layout", layout);
export const streamMessageCanvasBranch = (conversation_id: string, parent_node_id: string, message: string, onEvent: EventHandler, resource_ids: string[] = [], options?: ChatRequestOptions) =>
  streamChatRequest("/api/message-canvas/branches", { conversation_id, parent_node_id, message, resource_ids, ...options }, onEvent, conversation_id);
const graphPath = (canvas: string, root: string) => `/api/connectors/${encodeURIComponent(canvas)}/messages/${encodeURIComponent(root)}`;
export const getMessageSources = (canvas: string) => connectorRequest<MessageSources>("GET", `/api/connectors/${encodeURIComponent(canvas)}/message-sources`);
export const getMessageGraph = (canvas: string, root: string) => connectorRequest<ConnectorMessageGraph>("GET", graphPath(canvas, root));
export const createMessageGraph = (canvas: string, root: string) => connectorRequest<ConnectorMessageGraph>("POST", graphPath(canvas, root));
export const selectMessageNode = (canvas: string, root: string, selected_node_id: string | null) => connectorRequest("PUT", `${graphPath(canvas, root)}/selection`, { selected_node_id });
export const saveMessageLayout = (canvas: string, root: string, layout: MessageGraphLayout) => connectorRequest("PUT", `${graphPath(canvas, root)}/layout`, layout);
export const streamMessageTurn = (canvas: string, root: string, parent_node_id: string | null, message: string, onEvent: EventHandler, resource_ids: string[] = [], options?: ChatRequestOptions) =>
  streamChatRequest(`${graphPath(canvas, root)}/turns`, { parent_node_id, message, resource_ids, ...options }, onEvent, root);
