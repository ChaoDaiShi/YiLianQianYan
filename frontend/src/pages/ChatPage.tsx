import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import ChatView from "../components/chat/ChatView";
import ConversationSidebar from "../components/chat/ConversationSidebar";
import {
  getDrawerState,
  getWorkspaceMode,
  WORKBENCH_VIEWPORT_CLASS_NAME,
  type WorkspaceDrawer,
} from "../components/layout/workspaceLayout";
import { Drawer } from "../components/ui";
import ContextRail, { type ContextRailStatusTone } from "../components/layout/ContextRail";
import ExecutionSidebar from "../features/execution/ExecutionSidebar";
import { Activity } from "lucide-react";
import { useGlobalVoiceContext } from "../features/voice/GlobalVoiceHost";
import type { ConversationalAnchor } from "../api/voice";
import { getTaskGraphDetail } from "../api/taskWorld";
import { resolveCurrentTaskDestination } from "../components/chat/currentTaskEntry";

export default function ChatPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const conversationId = id || null;
  const {
    context,
    conversationRefresh,
    updateContext,
    chatVoiceControls,
    requestGlobalVoiceSession,
    speakAssistantMessage,
  } = useGlobalVoiceContext();
  const [workspaceMode, setWorkspaceMode] = useState(() =>
    getWorkspaceMode(window.innerWidth)
  );
  const [conversationDrawerOpen, setConversationDrawerOpen] = useState(false);
  const [executionDrawerOpen, setExecutionDrawerOpen] = useState(false);
  const [executionCollapsed, setExecutionCollapsed] = useState(true);
  const conversationToggleRef = useRef<HTMLButtonElement>(null);
  const executionToggleRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const updateMode = () => setWorkspaceMode(getWorkspaceMode(window.innerWidth));
    window.addEventListener("resize", updateMode);
    return () => window.removeEventListener("resize", updateMode);
  }, []);

  useEffect(() => {
    if (workspaceMode === "full") {
      setConversationDrawerOpen(false);
      setExecutionDrawerOpen(false);
    } else if (workspaceMode === "compact") {
      setConversationDrawerOpen(false);
    }
  }, [workspaceMode]);

  useEffect(() => {
    if (workspaceMode !== "full" || executionCollapsed) return;
    const collapseExecution = (event: KeyboardEvent) => {
      const target = event.target;
      if (event.key === "Escape" && !event.defaultPrevented
        && target instanceof Element
        && (target === document.body || target.closest(".execution-region"))) {
        setExecutionCollapsed(true);
        executionToggleRef.current?.focus();
      }
    };
    window.addEventListener("keydown", collapseExecution);
    return () => window.removeEventListener("keydown", collapseExecution);
  }, [executionCollapsed, workspaceMode]);

  useEffect(() => {
    if (context.conversational_anchor?.conversation_id !== conversationId) {
      updateContext({ conversational_anchor: null, anchor_action: "replace" });
    }
  }, [conversationId, context.conversational_anchor?.conversation_id, updateContext]);

  const onVoiceAnchorChange = useCallback(
    (anchor: ConversationalAnchor | null) => {
      updateContext({ conversational_anchor: anchor, anchor_action: "replace" });
    },
    [updateContext],
  );

  const onConversationChange = useCallback(
    (nextId: string | null) => {
      navigate(nextId ? `/chat/${nextId}` : "/chat");
    },
    [navigate]
  );

  const openCurrentTask = useCallback(async () => {
    navigate(await resolveCurrentTaskDestination(context.active_task, getTaskGraphDetail));
  }, [context.active_task, navigate]);

  const openDrawer = useCallback(
    (target: WorkspaceDrawer) => {
      const next = getDrawerState(workspaceMode, target);
      setConversationDrawerOpen(next.conversationOpen);
      setExecutionDrawerOpen(next.executionOpen);
    },
    [workspaceMode]
  );

  const conversationSidebar = (onClose?: () => void) => (
    <ConversationSidebar
      activeId={conversationId}
      onSelect={(nextId) => onConversationChange(nextId)}
      onNew={() => onConversationChange(null)}
      onClose={onClose}
    />
  );

  return (
    <div
      className={`${WORKBENCH_VIEWPORT_CLASS_NAME} page-canvas`}
      data-chat-view={conversationId ? "conversation" : "home"}
      data-mode={workspaceMode}
      data-execution-collapsed={
        workspaceMode === "full" && executionCollapsed ? "true" : "false"
      }
    >
      {workspaceMode !== "narrow" && (
        <div className="conversation-region min-h-0 border-r border-[var(--border)]">
          {conversationSidebar()}
        </div>
      )}

      {workspaceMode === "narrow" && (
        <Drawer
          open={conversationDrawerOpen}
          side="left"
          title="任务"
          onClose={() => setConversationDrawerOpen(false)}
          returnFocusRef={conversationToggleRef}
          showHeader={false}
        >
          {conversationSidebar(() => setConversationDrawerOpen(false))}
        </Drawer>
      )}

      <ChatView
        conversationId={conversationId}
        onConversationChange={onConversationChange}
        showConversationToggle={workspaceMode === "narrow"}
        showExecutionToggle={workspaceMode !== "full"}
        conversationToggleRef={conversationToggleRef}
        executionToggleRef={executionToggleRef}
        onToggleConversations={() => openDrawer("conversations")}
        onToggleExecution={() => openDrawer("execution")}
        onVoiceAnchorChange={onVoiceAnchorChange}
        conversationRefreshRevision={
          conversationRefresh?.conversation_id === conversationId
            ? conversationRefresh.revision
            : 0
        }
        onOpenCurrentTask={() => void openCurrentTask()}
        chatVoiceControls={chatVoiceControls}
        onRequestGlobalVoiceSession={requestGlobalVoiceSession}
        onManualSpeech={speakAssistantMessage}
        onOpenModelSettings={() => navigate("/settings?section=model")}
        renderExecution={(controller) => {
          if (workspaceMode === "full") {
            const approvals = controller.pendingApprovals.filter(
              (approval) => !conversationId || approval.conversation_id === conversationId,
            ).length;
            const statusTone: ContextRailStatusTone = approvals > 0
              ? "warning"
              : controller.state.connection === "error"
                ? "danger"
                : controller.state.connection === "connected" || controller.state.connection === "connecting"
                  ? "info"
                  : controller.state.terminal && controller.state.order.length > 0
                    ? "success"
                    : "neutral";
            return executionCollapsed ? (
              <ContextRail
                label="对话上下文"
                className="context-rail--chat"
                items={[{
                  id: "execution",
                  label: "展开执行轨迹",
                  icon: Activity,
                  controls: "chat-execution-panel",
                  expanded: false,
                  badge: approvals || undefined,
                  statusTone,
                  onSelect: () => setExecutionCollapsed(false),
                }]}
              />
            ) : (
              <div id="chat-execution-panel" className="execution-region min-h-0 border-l border-[var(--border)]">
                <ExecutionSidebar
                  {...controller}
                  onToggleCollapse={() => setExecutionCollapsed(true)}
                />
              </div>
            );
          }
          return (
            <Drawer
              open={executionDrawerOpen}
              side="right"
              title="执行轨迹"
              onClose={() => setExecutionDrawerOpen(false)}
              returnFocusRef={executionToggleRef}
              showHeader={false}
            >
              <ExecutionSidebar
                {...controller}
                onClose={() => setExecutionDrawerOpen(false)}
              />
            </Drawer>
          );
        }}
      />
    </div>
  );
}
