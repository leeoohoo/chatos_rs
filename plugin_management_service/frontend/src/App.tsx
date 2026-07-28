// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useState } from 'react';
import { Spin } from 'antd';
import { useQuery } from '@tanstack/react-query';

import { api, getAuthToken } from './api/client';
import { AppShell, type AppSection } from './components/AppShell';
import { LoginPage } from './pages/LoginPage';
import { McpCatalogPage } from './pages/McpCatalogPage';
import { RuntimePreviewPage } from './pages/RuntimePreviewPage';
import { SystemAgentsPage } from './pages/SystemAgentsPage';
import { PluginCatalogAdminPage } from './pages/PluginCatalogAdminPage';
import { PluginAuditPage } from './pages/PluginAuditPage';
import { PluginMarketplacesPage } from './pages/PluginMarketplacesPage';
import { PluginPublishersPage } from './pages/PluginPublishersPage';
import { PluginReleasesPage } from './pages/PluginReleasesPage';
import { AgentPromptVersionsPage } from './pages/agentPrompts/AgentPromptVersionsPage';

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
      {section === 'mcps' ? <McpCatalogPage user={user} /> : null}
      {section === 'marketplaces' ? <PluginMarketplacesPage user={user} /> : null}
      {section === 'publishers' ? <PluginPublishersPage user={user} /> : null}
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
    </AppShell>
  );
}
