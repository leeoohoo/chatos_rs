import { useEffect, useState } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';
import { Spin } from 'antd';
import { useQuery } from '@tanstack/react-query';

import { api, getAuthToken } from './api/client';
import { AppShell } from './components/AppShell';
import { ConfigPage } from './pages/ConfigPage';
import { LoginPage } from './pages/LoginPage';
import { ProjectDetailPage } from './pages/ProjectDetailPage';
import { ProjectsPage } from './pages/ProjectsPage';

export function App() {
  const [authVersion, setAuthVersion] = useState(0);
  const hasToken = Boolean(getAuthToken());
  const currentUserQuery = useQuery({
    queryKey: ['current-user', authVersion],
    queryFn: () => api.currentUser(),
    enabled: hasToken,
    retry: false,
  });

  useEffect(() => {
    const handler = () => setAuthVersion((value) => value + 1);
    window.addEventListener('project-service-auth-changed', handler);
    return () => window.removeEventListener('project-service-auth-changed', handler);
  }, []);

  if (!hasToken || currentUserQuery.isError) {
    return <LoginPage onLogin={() => setAuthVersion((value) => value + 1)} />;
  }

  if (currentUserQuery.isLoading || !currentUserQuery.data) {
    return (
      <div style={{ minHeight: '100vh', display: 'grid', placeItems: 'center' }}>
        <Spin />
      </div>
    );
  }

  return (
    <Routes>
      <Route element={<AppShell user={currentUserQuery.data} />}>
        <Route index element={<Navigate to="/projects" replace />} />
        <Route path="/projects" element={<ProjectsPage />} />
        <Route path="/projects/:projectId" element={<ProjectDetailPage />} />
        <Route path="/config" element={<ConfigPage />} />
        <Route path="*" element={<Navigate to="/projects" replace />} />
      </Route>
    </Routes>
  );
}
