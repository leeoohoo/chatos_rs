import { Flex, Spin } from 'antd';
import { lazy, Suspense } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

import { useAdminAuth } from '../../app/auth/AuthProvider';
import { useConsoleResources } from './app/hooks/useConsoleResources';

const DashboardSection = lazy(() => import('./app/sections/DashboardSection').then((module) => ({ default: module.DashboardSection })));
const DataSectionContainer = lazy(() => import('./app/sections/DataSectionContainer').then((module) => ({ default: module.DataSectionContainer })));
const SourcesSectionContainer = lazy(() => import('./app/sections/SourcesSectionContainer').then((module) => ({ default: module.SourcesSectionContainer })));
const ModelsSectionContainer = lazy(() => import('./app/sections/ModelsSectionContainer').then((module) => ({ default: module.ModelsSectionContainer })));
const PoliciesSectionContainer = lazy(() => import('./app/sections/PoliciesSectionContainer').then((module) => ({ default: module.PoliciesSectionContainer })));
const RunsSectionContainer = lazy(() => import('./app/sections/RunsSectionContainer').then((module) => ({ default: module.RunsSectionContainer })));

export default function MemoryEngineModuleRoutes() {
  const { user } = useAdminAuth();
  const canAccessAdmin = user.role === 'super_admin';
  const resources = useConsoleResources(canAccessAdmin);
  const requireSuperAdmin = (element: React.ReactElement) =>
    canAccessAdmin ? element : <Navigate to="data" replace />;
  return (
    <Suspense fallback={<Flex align="center" justify="center" style={{ minHeight: 320 }}><Spin size="large" /></Flex>}>
      <Routes>
        <Route index element={<Navigate to={canAccessAdmin ? 'dashboard' : 'data'} replace />} />
        <Route path="dashboard" element={requireSuperAdmin(<DashboardSection loading={!resources.initialized || resources.loading} dashboardStats={resources.dashboardStats} jobStats={resources.dashboardJobStats} />)} />
        <Route path="data" element={<DataSectionContainer refreshNonce={0} />} />
        <Route path="sources" element={requireSuperAdmin(<SourcesSectionContainer refreshNonce={0} onCatalogMutated={() => void resources.loadDashboardOverview()} />)} />
        <Route path="models" element={<ModelsSectionContainer refreshNonce={0} onCatalogMutated={() => void resources.loadDashboardOverview()} />} />
        <Route path="policies" element={requireSuperAdmin(<PoliciesSectionContainer refreshNonce={0} />)} />
        <Route path="runs" element={<RunsSectionContainer refreshNonce={0} />} />
        <Route path="*" element={<Navigate to={canAccessAdmin ? 'dashboard' : 'data'} replace />} />
      </Routes>
    </Suspense>
  );
}
