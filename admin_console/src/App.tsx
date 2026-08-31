import { App as AntdApp, ConfigProvider, theme } from 'antd';
import enUS from 'antd/locale/en_US';
import zhCN from 'antd/locale/zh_CN';

import { AdminAuthProvider } from './app/auth/AuthProvider';
import { AdminI18nProvider, useAdminI18n } from './app/i18n/AdminI18nProvider';
import { AppRoutes } from './app/routing/AppRoutes';

export default function App() {
  return (
    <AdminI18nProvider>
      <LocalizedApp />
    </AdminI18nProvider>
  );
}

function LocalizedApp() {
  const { locale } = useAdminI18n();
  return (
    <ConfigProvider
      locale={locale === 'zh-CN' ? zhCN : enUS}
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
