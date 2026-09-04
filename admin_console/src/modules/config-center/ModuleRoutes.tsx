import { Select, Space, Typography } from 'antd';
import { lazy, useState } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

import { AuditLog, ConfigEditor, Dashboard, Instances, ReleaseHistory } from './pages';

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
    </Space>
  );
}
