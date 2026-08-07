import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { ThemeProvider } from "./theme";
import AppShell from "./components/layout/AppShell";
import ChatPage from "./pages/ChatPage";
import SkillsPage from "./pages/SkillsPage";
import PluginsPage from "./pages/PluginsPage";
import WorkflowsPage from "./pages/WorkflowsPage";
import KnowledgePage from "./pages/KnowledgePage";
import SettingsPage from "./pages/SettingsPage";
import SystemPage from "./pages/SystemPage";
import LogsPage from "./pages/LogsPage";

export default function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <Routes>
          <Route element={<AppShell />}>
            <Route index element={<Navigate to="/chat" replace />} />
            <Route path="chat" element={<ChatPage />} />
            <Route path="chat/:id" element={<ChatPage />} />
            <Route path="system" element={<SystemPage />} />
            <Route path="logs" element={<LogsPage />} />
            <Route path="skills" element={<SkillsPage />} />
            <Route path="plugins" element={<PluginsPage />} />
            <Route path="workflows" element={<WorkflowsPage />} />
            <Route path="knowledge" element={<KnowledgePage />} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="*" element={<Navigate to="/chat" replace />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </ThemeProvider>
  );
}
