import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import ChatView from "../components/chat/ChatView";
import { ProcessDisplayProvider } from "../components/chat/ProcessDisplay";
import ConversationSidebar from "../components/chat/ConversationSidebar";
import ConversationBranchesDialog from "../components/chat/ConversationBranchesDialog";
import {
  getDrawerState,
  getWorkspaceMode,
  WORKBENCH_VIEWPORT_CLASS_NAME,
  type WorkspaceDrawer,
} from "../components/layout/workspaceLayout";
import { Drawer, showToast } from "../components/ui";
import PageShell from "../components/layout/PageShell";
import ExecutionSidebar from "../features/execution/ExecutionSidebar";
import { Activity, GitBranch } from "lucide-react";
import GitNodeManager from "../features/git/GitNodeManager";
import type {WorkspaceMode as ActivityMode} from "../components/chat/chatRequestOptions";
import { useGlobalVoiceContext } from "../features/voice/GlobalVoiceHost";
import type { ConversationalAnchor } from "../api/voice";
import { createConversation } from "../api/conversations";
import { getTaskGraphDetail } from "../api/taskWorld";
import { resolveCurrentTaskDestination } from "../components/chat/currentTaskEntry";
import { getConversationGoal } from "../api/conversationGoal";
import { getMessageCanvas, type MessageCanvasConversation } from "../api/connectorMessages";
import { conversationFamilies } from "../features/connectors/messages/messageForest";
import { CONVERSATION_HISTORY_CHANGED } from "../components/chat/conversationHistoryEvents";

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
    session: voiceSession,
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
  const [historyCollapsed, setHistoryCollapsed] = useState(true);
  const [executionDrawerOpen, setExecutionDrawerOpen] = useState(false);
  const [executionCollapsed, setExecutionCollapsed] = useState(true);
  const [activityMode,setActivityMode]=useState<ActivityMode>("auto");
  const [gitOpen,setGitOpen]=useState(false);
  const [familyEntries, setFamilyEntries] = useState<MessageCanvasConversation[]>([]);
  const [familyError, setFamilyError] = useState("");
  const [historyLoading, setHistoryLoading] = useState(true);
  const historyRequest = useRef(0);
  const [branchFamilyId,setBranchFamilyId]=useState<string|null>(null);
  const families = useMemo(() => conversationFamilies(familyEntries), [familyEntries]);
  const activeFamily = families.find(family => conversationId && family.conversationIds.includes(conversationId));
  const refreshHistory = useCallback(async () => {
    const request = ++historyRequest.current;
    try {
      const overview = await getMessageCanvas();
      if (request === historyRequest.current) {
        setFamilyEntries(overview.conversations); setFamilyError("");
        if (conversationId && !overview.conversations.some(conversation => conversation.id === conversationId)) navigate("/chat", { replace: true });
      }
    }
    catch (error) { if (request === historyRequest.current) setFamilyError(error instanceof Error ? error.message : String(error)); }
    finally { if (request === historyRequest.current) setHistoryLoading(false); }
  }, [conversationId, navigate]);
  useEffect(() => {
    let active = true;
    const refresh = () => { if (active) void refreshHistory(); };
    refresh(); window.addEventListener(CONVERSATION_HISTORY_CHANGED, refresh);
    const timer = window.setInterval(refresh, 5000);
    return () => { active = false; historyRequest.current++; window.removeEventListener(CONVERSATION_HISTORY_CHANGED, refresh); window.clearInterval(timer); };
  }, [conversationId, refreshHistory]);
  useEffect(() => { if (conversationRefresh) void refreshHistory(); }, [conversationRefresh, refreshHistory]);
  const handleActivityMode = useCallback((mode: ActivityMode) => setActivityMode(mode), []);
  useLayoutEffect(() => {
    setGitOpen(false);
    setExecutionCollapsed(true);
    setExecutionDrawerOpen(false);
  }, [conversationId]);
  useLayoutEffect(() => {
    if (activityMode === "chat") setGitOpen(false);
  }, [activityMode]);
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
        } else setHistoryCollapsed((prev) => !prev);
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

  const creatingVoiceConversation = useRef(false);
  const startVoiceCall = useCallback(() => {
    if (voiceSession && voiceSession.state !== "ended" && voiceSession.conversational_anchor) {
      requestGlobalVoiceSession();
      return;
    }
    if (conversationId) {
      const known = familyEntries.find((entry) => entry.id === conversationId);
      requestGlobalVoiceSession({
        conversation_id: conversationId,
        title: known?.title ?? "当前对话",
        updated_at: known?.updated_at ?? 0,
      });
      return;
    }
    if (creatingVoiceConversation.current) return;
    creatingVoiceConversation.current = true;
    void createConversation("语音对话").then((created) => {
      requestGlobalVoiceSession({
        conversation_id: created.id,
        title: created.title,
        updated_at: created.updated_at,
      });
      onConversationChange(created.id);
    }).catch((error) => {
      showToast(error instanceof Error ? error.message : "语音对话创建失败", "error");
    }).finally(() => { creatingVoiceConversation.current = false; });
  }, [conversationId, familyEntries, onConversationChange, requestGlobalVoiceSession, voiceSession]);

  const openCurrentTask = useCallback(async () => {
    if (!conversationId) { navigate(await resolveCurrentTaskDestination(null, getTaskGraphDetail)); return; }
    const result = await getConversationGoal(conversationId);
    if (!result.ok) { showToast(`关联任务读取失败：${result.error.message}`, "error"); return; }
    navigate(await resolveCurrentTaskDestination(result.data.binding?.graph_id ?? null, getTaskGraphDetail));
  }, [conversationId, navigate]);

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
      conversations={familyEntries}
      historyError={familyError}
      historyLoading={historyLoading}
      onShowBranches={setBranchFamilyId}
      activeId={conversationId}
      onSelect={(nextId) => onConversationChange(nextId)}
      onNew={() => onConversationChange(null)}
      onClose={onClose}
    />
  );

  return (
    <ProcessDisplayProvider><PageShell
      variant="workspace"
      maxWidth="none"
      className={`${WORKBENCH_VIEWPORT_CLASS_NAME} ui-opt-chat page-canvas`}
      contentClassName="contents"
      data-chat-view={conversationId ? "conversation" : "home"}
      data-mode={workspaceMode}
      style={{ gridTemplateColumns: workspaceMode === "full"
        ? `${historyCollapsed ? "" : "250px "}minmax(0, 1fr)${executionCollapsed ? "" : " 360px"}`
        : workspaceMode === "compact" && !historyCollapsed ? "240px minmax(0, 1fr)" : "minmax(0, 1fr)" }}
      data-history-collapsed={historyCollapsed ? "true" : "false"}
      data-execution-collapsed={
        workspaceMode === "full" && executionCollapsed ? "true" : "false"
      }
    >
      {workspaceMode !== "narrow" && !historyCollapsed && (
        <div className="conversation-region min-h-0 border-r border-[var(--border)]">
          {conversationSidebar(() => setHistoryCollapsed(true))}
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
        showConversationToggle={true}
        showExecutionToggle={false}
        showCanvasLink={!activeFamily}
        headerActions={<nav className="chat-topbar-panels" aria-label="对话侧栏">
          {activeFamily && <button type="button" aria-label="管理对话分支" aria-haspopup="dialog" onClick={() => setBranchFamilyId(activeFamily.id)}><GitBranch size={15}/><span>{activeFamily.conversationIds.length > 1 && conversationId === activeFamily.id ? "主线" : "分支"}</span></button>}
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
        onToggleConversations={() => workspaceMode === "narrow" ? openDrawer("conversations") : setHistoryCollapsed(value => !value)}
        onToggleExecution={handleToggleExecution}
        onVoiceAnchorChange={onVoiceAnchorChange}
        conversationRefreshRevision={
          conversationRefresh?.conversation_id === conversationId
            ? conversationRefresh.revision
            : 0
        }
        onOpenCurrentTask={() => void openCurrentTask()}
        chatVoiceControls={chatVoiceControls}
        onRequestGlobalVoiceSession={startVoiceCall}
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
    </PageShell></ProcessDisplayProvider>
  );
}
