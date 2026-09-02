import { Flex, Spin } from 'antd';
import { lazy, Suspense } from 'react';
import { Navigate as RouterNavigate, Route as RouterRoute, Routes as RouterRoutes } from 'react-router-dom';

import { useAdminAuth } from '../../app/auth/AuthProvider';
import { I18nProvider } from './i18n/I18nProvider';

const TasksPage = lazy(async () => ({ default: (await import('./pages/TasksPage')).TasksPage }));
const ModelsPage = lazy(async () => ({ default: (await import('./pages/ModelsPage')).ModelsPage }));
const ProjectsPage = lazy(async () => ({ default: (await import('./pages/ProjectsPage')).ProjectsPage }));
const RunsPage = lazy(async () => ({ default: (await import('./pages/RunsPage')).RunsPage }));
const PromptsPage = lazy(async () => ({ default: (await import('./pages/PromptsPage')).PromptsPage }));
const McpCatalogPage = lazy(async () => ({ default: (await import('./pages/McpCatalogPage')).McpCatalogPage }));
const SettingsPage = lazy(async () => ({ default: (await import('./pages/SettingsPage')).SettingsPage }));
const ToolingPage = lazy(async () => ({ default: (await import('./pages/ToolingPage')).ToolingPage }));
const UsersPage = lazy(async () => ({ default: (await import('./pages/UsersPage')).UsersPage }));

export default function TaskRunnerModuleRoutes() {
  const { user } = useAdminAuth();
  const requireAdmin = (element: React.ReactElement) =>
    ['admin', 'super_admin'].includes(user.role) ? element : <RouterNavigate to="tasks" replace />;
  return (
    <I18nProvider>
      <Suspense fallback={<Flex align="center" justify="center" style={{ minHeight: 320 }}><Spin size="large" /></Flex>}>
        <RouterRoutes>
          <RouterRoute index element={<RouterNavigate to="tasks" replace />} />
          <RouterRoute path="tasks" element={<TasksPage />} />
          <RouterRoute path="projects" element={<ProjectsPage />} />
          <RouterRoute path="models" element={<ModelsPage />} />
          <RouterRoute path="runs" element={<RunsPage />} />
          <RouterRoute path="prompts" element={<PromptsPage />} />
          <RouterRoute path="mcp" element={<McpCatalogPage />} />
          <RouterRoute path="tooling" element={<ToolingPage />} />
          <RouterRoute path="users" element={requireAdmin(<UsersPage />)} />
          <RouterRoute path="settings" element={requireAdmin(<SettingsPage />)} />
          <RouterRoute path="*" element={<RouterNavigate to="tasks" replace />} />
        </RouterRoutes>
      </Suspense>
    </I18nProvider>
  );
}
