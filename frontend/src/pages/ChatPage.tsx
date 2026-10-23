import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import ChatView from "../components/chat/ChatView";
import ConversationSidebar from "../components/chat/ConversationSidebar";
import ConversationBranchesDialog from "../components/chat/ConversationBranchesDialog";
import {
  getDrawerState,
  getWorkspaceMode,
  WORKBENCH_VIEWPORT_CLASS_NAME,
  type WorkspaceDrawer,
} from "../components/layout/workspaceLayout";
import { Drawer } from "../components/ui";
import PageShell from "../components/layout/PageShell";
import ExecutionSidebar from "../features/execution/ExecutionSidebar";
import { Activity, GitBranch } from "lucide-react";
import GitNodeManager from "../features/git/GitNodeManager";
import type {WorkspaceMode as ActivityMode} from "../components/chat/chatRequestOptions";
import { useGlobalVoiceContext } from "../features/voice/GlobalVoiceHost";
import type { ConversationalAnchor } from "../api/voice";
import { getTaskGraphDetail } from "../api/taskWorld";
import { resolveCurrentTaskDestination } from "../components/chat/currentTaskEntry";
import { getMessageCanvas, type MessageCanvasConversation } from "../api/connectorMessages";
import { conversationFamilies } from "../features/connectors/messages/messageForest";

export default function ChatPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const location = useLocation();
  const initialSuggestedText = typeof (location.state as { suggestedText?: unknown } | null)?.suggestedText === "string"
    ? (location.state as { suggestedText: string }).suggestedText
    : undefined;
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
  const [activityMode,setActivityMode]=useState<ActivityMode>("chat");
  const [gitOpen,setGitOpen]=useState(false);
  const [familyEntries, setFamilyEntries] = useState<MessageCanvasConversation[]>([]);
  const [familyError, setFamilyError] = useState("");
  const [branchFamilyId,setBranchFamilyId]=useState<string|null>(null);
  const families = useMemo(() => conversationFamilies(familyEntries), [familyEntries]);
  const activeFamily = families.find(family => conversationId && family.conversationIds.includes(conversationId));
  useEffect(() => {
    let active = true;
    const refresh = () => { void getMessageCanvas().then(overview => { if (active) { setFamilyEntries(overview.conversations); setFamilyError(""); } }).catch(error => { if (active) setFamilyError(String(error)); }); };
    refresh(); const timer = window.setInterval(refresh, 5000);
    return () => { active = false; window.clearInterval(timer); };
  }, []);
  const handleActivityMode = useCallback((mode: ActivityMode) => setActivityMode(mode), []);
  useLayoutEffect(() => {
    setGitOpen(false);
    setExecutionCollapsed(true);
    setExecutionDrawerOpen(false);
  }, [activityMode, conversationId]);
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
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing || event.keyCode === 229) return;

      const target = event.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable ||
          target.closest("[contenteditable='true']") !== null ||
          target.closest(".monaco-editor") !== null)
      ) {
        return;
      }

      if (target && target.closest("[role='dialog']")) {
        return;
      }

      const isCtrlOrMeta = event.ctrlKey || event.metaKey;
      if (!isCtrlOrMeta || event.altKey || event.shiftKey) return;

      if (event.key === "b" || event.key === "B") {
        event.preventDefault();
        if (workspaceMode === "narrow") {
          setConversationDrawerOpen((prev) => {
            const next = !prev;
            if (next) setExecutionDrawerOpen(false);
            return next;
          });
        }
      } else if (event.key === "j" || event.key === "J") {
        event.preventDefault();
        if (workspaceMode === "full") {
          setExecutionCollapsed((prev) => !prev);
        } else {
          setExecutionDrawerOpen((prev) => {
            const next = !prev;
            if (next) setConversationDrawerOpen(false);
            return next;
          });
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
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

  const handleToggleExecution = useCallback(() => {
    setGitOpen(false);
    if (workspaceMode === "full") {
      setExecutionCollapsed(false);
    } else {
      openDrawer("execution");
    }
  }, [workspaceMode, openDrawer]);

  const conversationSidebar = (onClose?: () => void) => (
    <ConversationSidebar
      families={families}
      onShowBranches={setBranchFamilyId}
      activeId={conversationId}
      onSelect={(nextId) => onConversationChange(nextId)}
      onNew={() => onConversationChange(null)}
      onClose={onClose}
    />
  );

  return (
    <PageShell
      variant="workspace"
      maxWidth="none"
      className={`${WORKBENCH_VIEWPORT_CLASS_NAME} ui-opt-chat page-canvas`}
      contentClassName="contents"
      data-chat-view={conversationId ? "conversation" : "home"}
      data-mode={workspaceMode}
      style={{ gridTemplateColumns: workspaceMode === "full"
        ? executionCollapsed ? "250px minmax(0, 1fr)" : "250px minmax(0, 1fr) 360px"
        : workspaceMode === "compact" ? "240px minmax(0, 1fr)" : "minmax(0, 1fr)" }}
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

      {branchFamilyId&&families.find(family=>family.id===branchFamilyId)&&<ConversationBranchesDialog key={branchFamilyId} family={families.find(family=>family.id===branchFamilyId)!} entries={familyEntries} activeId={conversationId} onSelect={onConversationChange} onClose={()=>setBranchFamilyId(null)}/>}
      <ChatView
        conversationId={conversationId}
        onWorkspaceModeChange={handleActivityMode}
        onConversationChange={onConversationChange}
        showConversationToggle={workspaceMode === "narrow"}
        showExecutionToggle={false}
        headerActions={<nav className="chat-topbar-panels" aria-label="对话侧栏">
          {activeFamily && activeFamily.conversationIds.length > 1 && <select aria-label="对话路径" value={conversationId || activeFamily.id} className="max-w-36 rounded-lg bg-transparent p-1 text-xs" onChange={event => onConversationChange(event.target.value)}>
            {activeFamily.conversationIds.map((path, index) => <option key={path} value={path}>{path === activeFamily.id ? "主线" : `路径 ${index + 1}`} · {familyEntries.find(entry => entry.id === path)?.title}</option>)}
          </select>}
          {familyError && <span className="text-xs text-[var(--danger)]" title={familyError} role="status">路径读取失败</span>}
          <button type="button" aria-label="执行与审批" aria-controls="chat-execution-panel" aria-pressed={(workspaceMode === "full" ? !executionCollapsed : executionDrawerOpen) && !gitOpen} onClick={() => {
            const opened = workspaceMode === "full" ? !executionCollapsed : executionDrawerOpen;
            if (opened && !gitOpen) { setExecutionCollapsed(true); setExecutionDrawerOpen(false); }
            else { setGitOpen(false); setExecutionCollapsed(false); if (workspaceMode !== "full") openDrawer("execution"); }
          }}><Activity size={15}/><span>执行</span></button>
          {activityMode !== "chat" && <button type="button" aria-label="Git 文件变更" aria-controls="chat-execution-panel" aria-pressed={(workspaceMode === "full" ? !executionCollapsed : executionDrawerOpen) && gitOpen} onClick={() => {
            const opened = workspaceMode === "full" ? !executionCollapsed : executionDrawerOpen;
            if (opened && gitOpen) { setExecutionCollapsed(true); setExecutionDrawerOpen(false); }
            else { setGitOpen(true); setExecutionCollapsed(false); if (workspaceMode !== "full") openDrawer("execution"); }
          }}><GitBranch size={15}/><span>Git</span></button>}
        </nav>}
        conversationToggleRef={conversationToggleRef}
        executionToggleRef={executionToggleRef}
        onToggleConversations={() => openDrawer("conversations")}
        onToggleExecution={handleToggleExecution}
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
        initialSuggestedText={initialSuggestedText}
        renderExecution={(controller) => {
          if (workspaceMode === "full") {
            return executionCollapsed ? null : <div id="chat-execution-panel" className="execution-region min-h-0 flex flex-col border-l border-[var(--border)]">
              {activityMode !== "chat" && gitOpen ? <GitNodeManager visible/> : <ExecutionSidebar {...controller} onToggleCollapse={() => setExecutionCollapsed(true)}/>}
            </div>;
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
              {activityMode!=="chat"&&gitOpen?<GitNodeManager visible={executionDrawerOpen}/>:<ExecutionSidebar
                {...controller}
                onClose={() => setExecutionDrawerOpen(false)}
              />}
            </Drawer>
          );
        }}
      />
    </PageShell>
  );
}
