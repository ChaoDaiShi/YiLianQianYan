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

export default function ChatPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const conversationId = id || null;
  const [workspaceMode, setWorkspaceMode] = useState(() =>
    getWorkspaceMode(window.innerWidth)
  );
  const [conversationDrawerOpen, setConversationDrawerOpen] = useState(false);
  const [executionDrawerOpen, setExecutionDrawerOpen] = useState(false);
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

  const onConversationChange = useCallback(
    (nextId: string | null) => {
      navigate(nextId ? `/chat/${nextId}` : "/chat");
    },
    [navigate]
  );

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
    <div className={WORKBENCH_VIEWPORT_CLASS_NAME} data-mode={workspaceMode}>
      {workspaceMode !== "narrow" && (
        <div className="min-h-0 border-r border-[var(--border)]">
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
        renderExecution={(controller) =>
          workspaceMode === "full" ? (
            <div className="min-h-0 border-l border-[var(--border)]">
              <ExecutionSidebar {...controller} />
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
