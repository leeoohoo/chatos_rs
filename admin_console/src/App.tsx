import { App as AntdApp, ConfigProvider, theme } from 'antd';
import zhCN from 'antd/locale/zh_CN';

import { AdminAuthProvider } from './app/auth/AuthProvider';
import { AppRoutes } from './app/routing/AppRoutes';

export default function App() {
  return (
    <ConfigProvider
      locale={zhCN}
      theme={{
        algorithm: theme.defaultAlgorithm,
        token: {
          colorPrimary: '#1677ff',
          borderRadius: 8,
          fontSize: 14,
          colorBgLayout: '#f4f6f8',
        },
        components: {
          Card: { paddingLG: 24 },
          Table: { headerBg: '#f7f8fa' },
          Layout: { headerBg: '#fff', siderBg: '#fff' },
        },
      }}
    >
      <AntdApp>
        <AdminAuthProvider>
          <AppRoutes />
        </AdminAuthProvider>
      </AntdApp>
    </ConfigProvider>
  );
}
