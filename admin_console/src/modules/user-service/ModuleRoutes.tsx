import { Navigate, Route, Routes } from 'react-router-dom';

import { AgentAccountsPage } from './pages/AgentAccountsPage';
import { ModelsPage } from './pages/ModelsPage';
import { SettingsPage } from './pages/SettingsPage';
import { UsersPage } from './pages/UsersPage';

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
