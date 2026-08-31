import { Navigate, Route, Routes } from 'react-router-dom';

import { ConfigPage } from './pages/ConfigPage';
import { ProjectDetailPage } from './pages/ProjectDetailPage';
import { ProjectsPage } from './pages/ProjectsPage';

export default function ProjectManagementModuleRoutes() {
  return (
    <Routes>
      <Route index element={<Navigate to="list" replace />} />
      <Route path="list" element={<ProjectsPage />} />
      <Route path="list/:projectId" element={<ProjectDetailPage />} />
      <Route path="config" element={<ConfigPage />} />
      <Route path="*" element={<Navigate to="list" replace />} />
    </Routes>
  );
}
