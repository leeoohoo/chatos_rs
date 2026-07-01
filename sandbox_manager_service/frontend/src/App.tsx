// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Navigate, Route, Routes } from 'react-router-dom';

import { AppShell } from './components/AppShell';
import { CreateSandboxPage } from './pages/CreateSandboxPage';
import { DashboardPage } from './pages/DashboardPage';
import { McpTestPage } from './pages/McpTestPage';
import { PoolPage } from './pages/PoolPage';
import { SandboxDetailPage } from './pages/SandboxDetailPage';
import { SandboxesPage } from './pages/SandboxesPage';
import { SettingsPage } from './pages/SettingsPage';

export function App() {
  return (
    <Routes>
      <Route element={<AppShell />}>
        <Route index element={<Navigate to="/dashboard" replace />} />
        <Route path="/dashboard" element={<DashboardPage />} />
        <Route path="/sandboxes" element={<SandboxesPage />} />
        <Route path="/sandboxes/:sandboxId" element={<SandboxDetailPage />} />
        <Route path="/pool" element={<PoolPage />} />
        <Route path="/mcp-test" element={<McpTestPage />} />
        <Route path="/create" element={<CreateSandboxPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="*" element={<Navigate to="/dashboard" replace />} />
      </Route>
    </Routes>
  );
}
