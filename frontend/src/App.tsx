import { lazy, Suspense } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { ThemeProvider } from "./theme";
import AppShell from "./components/layout/AppShell";
import RouteLoadingSurface from "./components/layout/RouteLoadingSurface";
import ChatPage from "./pages/ChatPage";

const TaskCenterPage = lazy(() => import("./pages/TaskCenterPage"));
const SystemPage = lazy(() => import("./pages/SystemPage"));
const LogsPage = lazy(() => import("./pages/LogsPage"));
const SettingsPage = lazy(() => import("./pages/SettingsPage"));
const SkillsPage = lazy(() => import("./pages/SkillsPage"));
const PluginsPage = lazy(() => import("./pages/PluginsPage"));
const WorkflowsPage = lazy(() => import("./pages/WorkflowsPage"));
const WorkspacesPage = lazy(() => import("./pages/WorkspacesPage"));
const WorkspaceDetailPage = lazy(() => import("./pages/WorkspaceDetailPage"));
const AgentsPage = lazy(() => import("./pages/AgentsPage"));
const CapabilitiesPage = lazy(() => import("./pages/CapabilitiesPage"));
const MemoryPage = lazy(() => import("./pages/MemoryPage"));
const KnowledgePage = lazy(() => import("./pages/KnowledgePage"));

export default function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <Suspense fallback={<RouteLoadingSurface />}>
          <Routes>
            <Route element={<AppShell />}>
              <Route index element={<Navigate to="/chat" replace />} />
              <Route path="chat" element={<ChatPage />} />
              <Route path="chat/:id" element={<ChatPage />} />
              <Route path="tasks" element={<TaskCenterPage />} />
              <Route path="system" element={<SystemPage />} />
              <Route path="logs" element={<LogsPage />} />
              <Route path="skills" element={<SkillsPage />} />
              <Route path="plugins" element={<PluginsPage />} />
              <Route path="workflows" element={<WorkflowsPage />} />
              <Route path="workspaces" element={<WorkspacesPage />} />
              <Route path="workspaces/:id" element={<WorkspaceDetailPage />} />
              <Route path="agents" element={<AgentsPage />} />
              <Route path="capabilities" element={<CapabilitiesPage />} />
              <Route path="memory" element={<MemoryPage />} />
              <Route path="knowledge" element={<KnowledgePage />} />
              <Route path="settings" element={<SettingsPage />} />
              <Route path="*" element={<Navigate to="/chat" replace />} />
            </Route>
          </Routes>
        </Suspense>
      </BrowserRouter>
    </ThemeProvider>
  );
}
