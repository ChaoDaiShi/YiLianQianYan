import { lazy } from "react";
import { Navigate, Route, Routes, useLocation } from "react-router-dom";
import GlobalVoiceHost, { resolveVoiceContextForRoute } from "../../features/voice/GlobalVoiceHost";
import AppShell from "../../components/layout/AppShell";
import ChatPage from "../../pages/ChatPage";
import GoalReminderHost from "../../features/task-world/GoalReminderHost";
import PersonalAssistantNoticeHost from "../../features/automation/PersonalAssistantNoticeHost";
import SoftwareMonitorNotices from "../../features/software/SoftwareMonitorNotices";
import { taskWorkflowDestination } from "../../features/workflows/taskGraphRoutes";

const BrowserPage = lazy(() => import("../../features/browser/BrowserPage"));
const SoftwarePage = lazy(() => import("../../features/software/SoftwarePage"));
const ConnectorsPage = lazy(() => import("../../features/connectors/ConnectorsPage"));
const TaskWorldPage = lazy(() => import("../../features/task-world/TaskWorldPage"));
const MorePage = lazy(() => import("../../pages/MorePage"));
const UserManualPage = lazy(() => import("../../features/help/UserManualPage"));
const AutomationsPage = lazy(() => import("../../features/automation/AutomationsPage"));
const SoftwareChannelsPage = lazy(() => import("../../features/channels/ChannelsPanel"));
const PluginFeaturePage = lazy(() => import("../../features/plugins/PluginFeaturePage"));
const SystemPage = lazy(() => import("../../pages/SystemPage"));
const LogsPage = lazy(() => import("../../pages/LogsPage"));
const SettingsPage = lazy(() => import("../../pages/SettingsPage"));
const WorkspacesPage = lazy(() => import("../../pages/WorkspacesPage"));
const WorkspaceDetailPage = lazy(() => import("../../pages/WorkspaceDetailPage"));
const WorkspaceLearningPage = lazy(() => import("../../features/learning/WorkspaceLearningPage"));
const WorkspaceCreationPage = lazy(() => import("../../features/creation/WorkspaceCreationPage"));
const AgentsPage = lazy(() => import("../../pages/AgentsPage"));
const CapabilitiesPage = lazy(() => import("../../pages/CapabilitiesPage"));
const MemoryPage = lazy(() => import("../../pages/MemoryPage"));
const KnowledgePage = lazy(() => import("../../pages/KnowledgePage"));
const AccountCenterPage = lazy(() => import("../../pages/AccountCenterPage"));
const SetupWizardPage = lazy(() => import("../../pages/SetupWizardPage"));

export default function WorkspaceSurface() {
  const { pathname, search, hash } = useLocation();
  return (
    <GlobalVoiceHost contextPatch={resolveVoiceContextForRoute(pathname, "standalone")}>
    <GoalReminderHost />
    <PersonalAssistantNoticeHost />
    <SoftwareMonitorNotices />
    <Routes>
      <Route element={<AppShell />}>
        <Route index element={<Navigate to="/chat" replace />} />
        <Route path="chat" element={<ChatPage />} />
        <Route path="chat/:id" element={<ChatPage />} />
        <Route path="browser" element={<BrowserPage />} />
        <Route path="software" element={<SoftwarePage />} />
        <Route path="software/channels" element={<SoftwareChannelsPage />} />
        <Route path="connectors" element={<ConnectorsPage />} />
        <Route path="tasks" element={<Navigate to={taskWorkflowDestination(search)} replace />} />
        <Route path="task-world/:graphId" element={<TaskWorldPage />} />
        <Route path="more" element={<MorePage />} />
        <Route path="more/manual" element={<UserManualPage />} />
        <Route path="automations" element={<AutomationsPage />} />
        <Route path="channels" element={<Navigate to={{ pathname: "/software/channels", search, hash }} replace />} />
        <Route path="more/plugins/:pluginId/:featureId" element={<PluginFeaturePage />} />
        <Route path="system" element={<SystemPage />} />
        <Route path="logs" element={<LogsPage />} />
        <Route path="workspaces" element={<WorkspacesPage />} />
        <Route path="workspaces/:id" element={<WorkspaceDetailPage />} />
        <Route path="workspaces/:id/learning" element={<WorkspaceLearningPage />} />
        <Route path="workspaces/:id/creation" element={<WorkspaceCreationPage />} />
        <Route path="agents" element={<AgentsPage />} />
        <Route path="capabilities" element={<CapabilitiesPage />} />
        <Route path="memory" element={<MemoryPage />} />
        <Route path="knowledge" element={<KnowledgePage />} />
        <Route path="account" element={<AccountCenterPage />} />
        <Route path="sync" element={<Navigate to="/account?section=sync" replace />} />
        <Route path="privacy" element={<Navigate to="/account?section=privacy" replace />} />
        <Route path="setup" element={<SetupWizardPage />} />
        <Route path="settings" element={<SettingsPage />} />
        <Route path="*" element={<Navigate to="/chat" replace />} />
      </Route>
    </Routes>
    </GlobalVoiceHost>
  );
}
