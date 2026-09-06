import { Select, Space, Spin, Typography } from 'antd';
import { lazy, Suspense, useState } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

const Dashboard = lazy(() => import('./Dashboard').then((module) => ({ default: module.Dashboard })));
const ConfigEditor = lazy(() => import('./pages').then((module) => ({ default: module.ConfigEditor })));
const ReleaseHistory = lazy(() => import('./ReleaseHistory').then((module) => ({ default: module.ReleaseHistory })));
const Instances = lazy(() => import('./Instances').then((module) => ({ default: module.Instances })));
const AuditLog = lazy(() => import('./AuditLog').then((module) => ({ default: module.AuditLog })));
const QueueOperationsPanel = lazy(() => import('./QueueOperationsPanel').then((module) => ({ default: module.QueueOperationsPanel })));

export default function ConfigCenterModuleRoutes() {
  const [environment, setEnvironment] = useState(localStorage.getItem('chatos.configuration-center.environment') || 'production');
  const updateEnvironment = (value: string) => {
    localStorage.setItem('chatos.configuration-center.environment', value);
    setEnvironment(value);
  };
  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space style={{ width: '100%', justifyContent: 'flex-end' }}>
        <Typography.Text type="secondary">环境</Typography.Text>
        <Select value={environment} onChange={updateEnvironment} style={{ width: 160 }} options={['local', 'development', 'staging', 'production'].map((value) => ({ value, label: value }))} />
      </Space>
      <Suspense fallback={<div className="centered"><Spin size="large" /></div>}>
        <Routes>
          <Route index element={<Navigate to="dashboard" replace />} />
          <Route path="dashboard" element={<Dashboard environment={environment} />} />
          <Route path="definitions" element={<ConfigEditor environment={environment} />} />
          <Route path="releases" element={<ReleaseHistory environment={environment} />} />
          <Route path="queues" element={<QueueOperationsPanel environment={environment} />} />
          <Route path="instances" element={<Instances />} />
          <Route path="audit" element={<AuditLog />} />
          <Route path="*" element={<Navigate to="dashboard" replace />} />
        </Routes>
      </Suspense>
    </Space>
  );
}
