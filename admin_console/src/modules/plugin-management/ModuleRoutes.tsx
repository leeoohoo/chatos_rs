import { Flex, Spin } from 'antd';
import { lazy, Suspense } from 'react';
import { Navigate, Route, Routes, useNavigate, useParams, useSearchParams } from 'react-router-dom';

import { useAdminAuth } from '../../app/auth/AuthProvider';
import { I18nProvider } from './i18n/I18nProvider';
import type { CurrentUser } from './types';

const McpCatalogPage = lazy(() => import('./pages/McpCatalogPage').then((module) => ({ default: module.McpCatalogPage })));
const RuntimePreviewPage = lazy(() => import('./pages/RuntimePreviewPage').then((module) => ({ default: module.RuntimePreviewPage })));
const SystemAgentsPage = lazy(() => import('./pages/SystemAgentsPage').then((module) => ({ default: module.SystemAgentsPage })));
const PluginCatalogAdminPage = lazy(() => import('./pages/PluginCatalogAdminPage').then((module) => ({ default: module.PluginCatalogAdminPage })));
const PluginAuditPage = lazy(() => import('./pages/PluginAuditPage').then((module) => ({ default: module.PluginAuditPage })));
const PluginDiagnosticsPage = lazy(() => import('./pages/PluginDiagnosticsPage').then((module) => ({ default: module.PluginDiagnosticsPage })));
const PluginMarketplacesPage = lazy(() => import('./pages/PluginMarketplacesPage').then((module) => ({ default: module.PluginMarketplacesPage })));
const PluginPublishersPage = lazy(() => import('./pages/PluginPublishersPage').then((module) => ({ default: module.PluginPublishersPage })));
const PluginReleasesPage = lazy(() => import('./pages/PluginReleasesPage').then((module) => ({ default: module.PluginReleasesPage })));
const AgentPromptVersionsPage = lazy(() => import('./pages/agentPrompts/AgentPromptVersionsPage').then((module) => ({ default: module.AgentPromptVersionsPage })));

export default function PluginManagementModuleRoutes() {
  const { user: adminUser } = useAdminAuth();
  const user: CurrentUser = {
    principal_type: adminUser.principal_type || 'human_user',
    user_id: adminUser.id,
    username: adminUser.username,
    display_name: adminUser.display_name,
    role: adminUser.role,
  };
  const requireSuperAdmin = (element: React.ReactElement) =>
    user.role === 'super_admin' ? element : <Navigate to="mcp" replace />;
  return (
    <I18nProvider>
      <Suspense fallback={<Flex align="center" justify="center" style={{ minHeight: 320 }}><Spin size="large" /></Flex>}>
        <Routes>
          <Route index element={<Navigate to="mcp" replace />} />
          <Route path="mcp" element={<McpCatalogPage user={user} />} />
          <Route path="catalog" element={requireSuperAdmin(<PluginCatalogRoute user={user} />)} />
          <Route path="releases" element={requireSuperAdmin(<PluginReleasesRoute user={user} />)} />
          <Route path="marketplaces" element={<PluginMarketplacesPage user={user} />} />
          <Route path="publishers" element={<PluginPublishersPage user={user} />} />
          <Route path="agents" element={requireSuperAdmin(<SystemAgentsRoute user={user} />)} />
          <Route path="agents/:agentKey/prompts" element={requireSuperAdmin(<AgentPromptRoute user={user} />)} />
          <Route path="runtime" element={requireSuperAdmin(<RuntimePreviewPage user={user} />)} />
          <Route path="diagnostics" element={<PluginDiagnosticsPage />} />
          <Route path="audit" element={requireSuperAdmin(<PluginAuditPage user={user} />)} />
          <Route path="*" element={<Navigate to="mcp" replace />} />
        </Routes>
      </Suspense>
    </I18nProvider>
  );
}

function PluginCatalogRoute({ user }: { user: CurrentUser }) {
  const navigate = useNavigate();
  return <PluginCatalogAdminPage user={user} onOpenReleases={(pluginId) => navigate(`/plugins/releases?plugin_id=${encodeURIComponent(pluginId)}`)} />;
}

function PluginReleasesRoute({ user }: { user: CurrentUser }) {
  const [searchParams] = useSearchParams();
  return <PluginReleasesPage user={user} initialPluginId={searchParams.get('plugin_id')} />;
}

function SystemAgentsRoute({ user }: { user: CurrentUser }) {
  const navigate = useNavigate();
  return <SystemAgentsPage user={user} onOpenPromptSettings={(agentKey) => navigate(`/plugins/agents/${encodeURIComponent(agentKey)}/prompts`)} />;
}

function AgentPromptRoute({ user }: { user: CurrentUser }) {
  const navigate = useNavigate();
  const { agentKey = '' } = useParams();
  return <AgentPromptVersionsPage user={user} agentKey={decodeURIComponent(agentKey)} onBack={() => navigate('/plugins/agents')} />;
}
