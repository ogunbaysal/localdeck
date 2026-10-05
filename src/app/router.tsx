import { HashRouter, Navigate, Route, Routes } from "react-router";
import { ActivityPage } from "@/features/activity/ActivityPage";
import { ProjectDetailPage } from "@/features/project-detail/ProjectDetailPage";
import { PortsPage } from "@/features/ports/PortsPage";
import { ProjectsPage } from "@/features/projects/ProjectsPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { Providers } from "./providers";
import { Shell } from "./shell";

/** The app's top-level routes (plus project detail) inside the shell. Unknown paths fall back to Projects. */
export function AppRoutes() {
  return (
    <Routes>
      <Route element={<Shell />}>
        <Route index element={<ProjectsPage />} />
        <Route path="projects/:projectId" element={<ProjectDetailPage />} />
        <Route path="ports" element={<PortsPage />} />
        <Route path="activity" element={<ActivityPage />} />
        <Route path="settings" element={<SettingsPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}

/** Hash router (no server rewrite needed in the Tauri webview) wrapping providers and routes. */
export function AppRouter() {
  return (
    <HashRouter>
      <Providers>
        <AppRoutes />
      </Providers>
    </HashRouter>
  );
}
