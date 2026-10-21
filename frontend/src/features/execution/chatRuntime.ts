import { loadConversation, setConversationWorkspaceMode } from "../../api/conversations";
import { sendMessage, stopGeneration } from "../../api/chat";
import { approveAction, rejectAction, listPendingApprovals, getApproval } from "../../api/approvals";
import { useApprovalStore } from "../../stores/approvalStore";
import { createConversationRuntime } from "./conversationRuntime";

/** One owner per conversation, independent of router and ChatView mounts. */
export const chatRuntime = createConversationRuntime({
  loadConversation, sendMessage, stopGeneration, approveAction, rejectAction, listPendingApprovals, getApproval,
  setWorkspaceMode: setConversationWorkspaceMode,
  onApprovals(conversationId, approvals) {
    const store = useApprovalStore.getState();
    store.setApprovals([...store.approvals.filter((approval) => approval.conversation_id !== conversationId), ...approvals]);
  },
});
