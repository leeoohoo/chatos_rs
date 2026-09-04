import { lazy } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

const ConfigPage = lazy(() => import('./pages/ConfigPage').then((module) => ({ default: module.ConfigPage })));
const ProjectDetailPage = lazy(() => import('./pages/ProjectDetailPage').then((module) => ({ default: module.ProjectDetailPage })));
const ProjectsPage = lazy(() => import('./pages/ProjectsPage').then((module) => ({ default: module.ProjectsPage })));

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
