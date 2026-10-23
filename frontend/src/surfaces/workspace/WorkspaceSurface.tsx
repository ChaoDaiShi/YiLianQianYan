import { lazy } from "react";
import { Navigate, Route, Routes, useLocation } from "react-router-dom";
import GlobalVoiceHost, { resolveVoiceContextForRoute } from "../../features/voice/GlobalVoiceHost";
import AppShell from "../../components/layout/AppShell";
import ChatPage from "../../pages/ChatPage";

const ConnectorsPage = lazy(() => import("../../features/connectors/ConnectorsPage"));
const MorePage = lazy(() => import("../../pages/MorePage"));
const AutomationsPage = lazy(() => import("../../features/automation/AutomationsPage"));
const ChannelsPage = lazy(() => import("../../features/channels/ChannelsPage"));
const PluginFeaturePage = lazy(() => import("../../features/plugins/PluginFeaturePage"));
const SystemPage = lazy(() => import("../../pages/SystemPage"));
const LogsPage = lazy(() => import("../../pages/LogsPage"));
const SettingsPage = lazy(() => import("../../pages/SettingsPage"));
const WorkspacesPage = lazy(() => import("../../pages/WorkspacesPage"));
const WorkspaceDetailPage = lazy(() => import("../../pages/WorkspaceDetailPage"));
const AgentsPage = lazy(() => import("../../pages/AgentsPage"));
const CapabilitiesPage = lazy(() => import("../../pages/CapabilitiesPage"));
const MemoryPage = lazy(() => import("../../pages/MemoryPage"));
const KnowledgePage = lazy(() => import("../../pages/KnowledgePage"));
const AccountCenterPage = lazy(() => import("../../pages/AccountCenterPage"));
const SetupWizardPage = lazy(() => import("../../pages/SetupWizardPage"));

export default function WorkspaceSurface() {
  const { pathname } = useLocation();
  return (
    <GlobalVoiceHost contextPatch={resolveVoiceContextForRoute(pathname, "standalone")}>
    <Routes>
      <Route element={<AppShell />}>
        <Route index element={<Navigate to="/chat" replace />} />
        <Route path="chat" element={<ChatPage />} />
        <Route path="chat/:id" element={<ChatPage />} />
        <Route path="connectors" element={<ConnectorsPage />} />
        <Route path="more" element={<MorePage />} />
        <Route path="automations" element={<AutomationsPage />} />
        <Route path="channels" element={<ChannelsPage />} />
        <Route path="more/plugins/:pluginId/:featureId" element={<PluginFeaturePage />} />
        <Route path="system" element={<SystemPage />} />
        <Route path="logs" element={<LogsPage />} />
        <Route path="workspaces" element={<WorkspacesPage />} />
        <Route path="workspaces/:id" element={<WorkspaceDetailPage />} />
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
