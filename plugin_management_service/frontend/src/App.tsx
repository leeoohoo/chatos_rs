// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { lazy, Suspense, useEffect, useState } from 'react';
import { Spin } from 'antd';
import { useQuery } from '@tanstack/react-query';

import { api, getAuthToken } from './api/client';
import { AppShell, type AppSection } from './components/AppShell';
import { LoginPage } from './pages/LoginPage';

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

export function App() {
  const [authVersion, setAuthVersion] = useState(0);
  const [section, setSection] = useState<AppSection>('mcps');
  const [promptAgentKey, setPromptAgentKey] = useState<string | null>(null);
  const [releasePluginId, setReleasePluginId] = useState<string | null>(null);
  const hasToken = Boolean(getAuthToken());
  const currentUserQuery = useQuery({
    queryKey: ['current-user', authVersion],
    queryFn: () => api.currentUser(),
    enabled: hasToken,
    retry: false,
  });

  useEffect(() => {
    const handler = () => setAuthVersion((value) => value + 1);
    window.addEventListener('plugin-management-auth-changed', handler);
    return () => window.removeEventListener('plugin-management-auth-changed', handler);
  }, []);

  if (!hasToken || currentUserQuery.isError) {
    return <LoginPage onLogin={() => setAuthVersion((value) => value + 1)} />;
  }

  if (currentUserQuery.isLoading || !currentUserQuery.data) {
    return (
      <div className="loading-screen">
        <Spin />
      </div>
    );
  }

  const user = currentUserQuery.data;

  return (
    <AppShell
      user={user}
      section={section}
      onSectionChange={(nextSection) => {
        setPromptAgentKey(null);
        setSection(nextSection);
      }}
    >
      <Suspense fallback={<div className="loading-screen"><Spin /></div>}>
        {section === 'mcps' ? <McpCatalogPage user={user} /> : null}
        {section === 'marketplaces' ? <PluginMarketplacesPage user={user} /> : null}
        {section === 'publishers' ? <PluginPublishersPage user={user} /> : null}
        {section === 'diagnostics' ? <PluginDiagnosticsPage /> : null}
        {section === 'plugins' ? (
          <PluginCatalogAdminPage
            user={user}
            onOpenReleases={(pluginId) => {
              setReleasePluginId(pluginId);
              setSection('releases');
            }}
          />
        ) : null}
        {section === 'releases' ? (
          <PluginReleasesPage user={user} initialPluginId={releasePluginId} />
        ) : null}
        {section === 'agents' && promptAgentKey ? (
          <AgentPromptVersionsPage
            user={user}
            agentKey={promptAgentKey}
            onBack={() => setPromptAgentKey(null)}
          />
        ) : null}
        {section === 'agents' && !promptAgentKey ? (
          <SystemAgentsPage user={user} onOpenPromptSettings={setPromptAgentKey} />
        ) : null}
        {section === 'runtime' ? <RuntimePreviewPage user={user} /> : null}
        {section === 'audit' ? <PluginAuditPage user={user} /> : null}
      </Suspense>
    </AppShell>
  );
}
