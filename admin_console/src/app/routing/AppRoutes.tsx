import { Flex, Spin } from 'antd';
import { lazy, Suspense } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

import { AdminLayout } from '../layout/AdminLayout';

const UserServiceModule = lazy(() => import('../../modules/user-service/ModuleRoutes'));
const ProjectManagementModule = lazy(() => import('../../modules/project-management/ModuleRoutes'));
const TaskRunnerModule = lazy(() => import('../../modules/task-runner/ModuleRoutes'));
const PluginManagementModule = lazy(() => import('../../modules/plugin-management/ModuleRoutes'));
const MemoryEngineModule = lazy(() => import('../../modules/memory-engine/ModuleRoutes'));
const ConfigCenterModule = lazy(() => import('../../modules/config-center/ModuleRoutes'));

function ModuleBoundary({ className, children }: { className: string; children: React.ReactNode }) {
  return (
    <div className={`admin-module-root ${className}`}>
      <Suspense fallback={<Flex align="center" justify="center" style={{ minHeight: 320 }}><Spin size="large" /></Flex>}>
        {children}
      </Suspense>
    </div>
  );
}

export function AppRoutes() {
  return (
    <Routes>
      <Route element={<AdminLayout />}>
        <Route index element={<Navigate to="/users/models" replace />} />
        <Route path="users/*" element={<ModuleBoundary className="user-service-module"><UserServiceModule /></ModuleBoundary>} />
        <Route path="projects/*" element={<ModuleBoundary className="project-management-module"><ProjectManagementModule /></ModuleBoundary>} />
        <Route path="task-runner/*" element={<ModuleBoundary className="task-runner-module"><TaskRunnerModule /></ModuleBoundary>} />
        <Route path="plugins/*" element={<ModuleBoundary className="plugin-management-module"><PluginManagementModule /></ModuleBoundary>} />
        <Route path="memory/*" element={<ModuleBoundary className="memory-engine-module"><MemoryEngineModule /></ModuleBoundary>} />
        <Route path="config/*" element={<ModuleBoundary className="config-center-module"><ConfigCenterModule /></ModuleBoundary>} />
        <Route path="*" element={<Navigate to="/users/models" replace />} />
      </Route>
    </Routes>
  );
}
