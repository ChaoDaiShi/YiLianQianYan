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
import ExecutionSidebar from "../features/execution/ExecutionSidebar";
import { useGlobalVoiceContext } from "../features/voice/GlobalVoiceHost";
import type { ConversationalAnchor } from "../api/voice";
import { getTaskGraphDetail } from "../api/taskWorld";
import { resolveCurrentTaskDestination } from "../components/chat/currentTaskEntry";

export default function ChatPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const conversationId = id || null;
  const { context, conversationRefresh, updateContext } = useGlobalVoiceContext();
  const [workspaceMode, setWorkspaceMode] = useState(() =>
    getWorkspaceMode(window.innerWidth)
  );
  const [conversationDrawerOpen, setConversationDrawerOpen] = useState(false);
  const [executionDrawerOpen, setExecutionDrawerOpen] = useState(false);
  const [executionCollapsed, setExecutionCollapsed] = useState(false);
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
        renderExecution={(controller) =>
          workspaceMode === "full" ? (
            <div className="execution-region min-h-0 border-l border-[var(--border)]">
              <ExecutionSidebar
                {...controller}
                collapsed={executionCollapsed}
                onToggleCollapse={() => setExecutionCollapsed((value) => !value)}
              />
            </div>
          ) : (
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
          )
        }
      />
    </div>
  );
}
