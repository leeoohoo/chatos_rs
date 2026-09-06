import { lazy } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

const AgentAccountsPage = lazy(() => import('./pages/AgentAccountsPage').then((module) => ({ default: module.AgentAccountsPage })));
const ModelsPage = lazy(() => import('./pages/ModelsPage').then((module) => ({ default: module.ModelsPage })));
const SettingsPage = lazy(() => import('./pages/SettingsPage').then((module) => ({ default: module.SettingsPage })));
const UsersPage = lazy(() => import('./pages/UsersPage').then((module) => ({ default: module.UsersPage })));

export default function UserServiceModuleRoutes() {
  return (
    <Routes>
      <Route index element={<Navigate to="models" replace />} />
      <Route path="models" element={<ModelsPage />} />
      <Route path="accounts" element={<UsersPage />} />
      <Route path="agents" element={<AgentAccountsPage />} />
      <Route path="settings" element={<SettingsPage />} />
      <Route path="*" element={<Navigate to="models" replace />} />
    </Routes>
  );
}
