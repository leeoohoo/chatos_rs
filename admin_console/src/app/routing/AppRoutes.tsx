import { Button, Flex, Result, Spin } from 'antd';
import { lazy, Suspense } from 'react';
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom';

import { useAdminAuth } from '../auth/AuthProvider';
import { defaultAdminPath, isSuperAdmin } from '../auth/access';
import { useAdminI18n } from '../i18n/AdminI18nProvider';
import { AdminLayout } from '../layout/AdminLayout';
import { ModuleErrorBoundary } from '../../shared/components/ModuleErrorBoundary';

const UserServiceModule = lazy(() => import('../../modules/user-service/ModuleRoutes'));
const ProjectManagementModule = lazy(() => import('../../modules/project-management/ModuleRoutes'));
const TaskRunnerModule = lazy(() => import('../../modules/task-runner/ModuleRoutes'));
const PluginManagementModule = lazy(() => import('../../modules/plugin-management/ModuleRoutes'));
const MemoryEngineModule = lazy(() => import('../../modules/memory-engine/ModuleRoutes'));
const ConfigCenterModule = lazy(() => import('../../modules/config-center/ModuleRoutes'));
const UserModelsPage = lazy(() => import('../../modules/user-service/pages/ModelsPage').then((module) => ({ default: module.ModelsPage })));
const ProjectsPage = lazy(() => import('../../modules/project-management/pages/ProjectsPage').then((module) => ({ default: module.ProjectsPage })));

function ModuleBoundary({ className, children }: { className: string; children: React.ReactNode }) {
  const location = useLocation();
  const { t } = useAdminI18n();
  return (
    <div className={`admin-module-root ${className}`}>
      <ModuleErrorBoundary
        resetKey={location.pathname}
        title={t('error.moduleTitle')}
        description={t('error.moduleDescription')}
        retryLabel={t('error.retry')}
      >
        <Suspense fallback={<Flex align="center" justify="center" style={{ minHeight: 320 }}><Spin size="large" /></Flex>}>
          {children}
        </Suspense>
      </ModuleErrorBoundary>
    </div>
  );
}

export function AppRoutes() {
  const { user } = useAdminAuth();
  const defaultPath = defaultAdminPath(user.role);
  return (
    <Routes>
      <Route element={<AdminLayout />}>
        <Route index element={<Navigate to={defaultPath} replace />} />
        <Route path="users/models" element={isSuperAdmin(user.role)
          ? <ModuleBoundary className="user-service-module"><UserModelsPage /></ModuleBoundary>
          : <AccessDenied defaultPath={defaultPath} />} />
        <Route path="users/*" element={isSuperAdmin(user.role)
          ? <ModuleBoundary className="user-service-module"><UserServiceModule /></ModuleBoundary>
          : <AccessDenied defaultPath={defaultPath} />} />
        <Route path="projects/list" element={<ModuleBoundary className="project-management-module"><ProjectsPage /></ModuleBoundary>} />
        <Route path="projects/*" element={<ModuleBoundary className="project-management-module"><ProjectManagementModule /></ModuleBoundary>} />
        <Route path="task-runner/*" element={<ModuleBoundary className="task-runner-module"><TaskRunnerModule /></ModuleBoundary>} />
        <Route path="plugins/*" element={<ModuleBoundary className="plugin-management-module"><PluginManagementModule /></ModuleBoundary>} />
        <Route path="memory/*" element={<ModuleBoundary className="memory-engine-module"><MemoryEngineModule /></ModuleBoundary>} />
        <Route path="config/*" element={isSuperAdmin(user.role)
          ? <ModuleBoundary className="config-center-module"><ConfigCenterModule /></ModuleBoundary>
          : <AccessDenied defaultPath={defaultPath} />} />
        <Route path="*" element={<Navigate to={defaultPath} replace />} />
      </Route>
    </Routes>
  );
}

function AccessDenied({ defaultPath }: { defaultPath: string }) {
  const navigate = useNavigate();
  const { t } = useAdminI18n();
  return (
    <Result
      status="403"
      title={t('access.deniedTitle')}
      subTitle={t('access.deniedDescription')}
      extra={<Button type="primary" onClick={() => navigate(defaultPath, { replace: true })}>{t('access.back')}</Button>}
    />
  );
}
