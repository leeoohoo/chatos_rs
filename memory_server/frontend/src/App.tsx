import {
  ApiOutlined,
  BarChartOutlined,
  DatabaseOutlined,
  FileTextOutlined,
  HistoryOutlined,
  LogoutOutlined,
  NodeIndexOutlined,
  SettingOutlined,
  UserOutlined,
} from '@ant-design/icons';
import {
  Alert,
  App as AntdApp,
  Button,
  Card,
  ConfigProvider,
  Input,
  Layout,
  Menu,
  Segmented,
  Space,
  Spin,
  Tag,
  Typography,
} from 'antd';
import zhCN from 'antd/locale/zh_CN';
import enUS from 'antd/locale/en_US';
import { useEffect, useMemo, useState } from 'react';

import { api } from './api/client';
import { I18nProvider, useI18n } from './i18n';
import { DashboardPage } from './pages/DashboardPage';
import { JobConfigsPage } from './pages/JobConfigsPage';
import { JobRunsPage } from './pages/JobRunsPage';
import { ModelConfigsPage } from './pages/ModelConfigsPage';
import { SessionDetailPage } from './pages/SessionDetailPage';
import { SessionsPage } from './pages/SessionsPage';
import { SummaryLevelsPage } from './pages/SummaryLevelsPage';

const { Sider, Header, Content } = Layout;
const { Title, Text } = Typography;

type TabKey =
  | 'dashboard'
  | 'sessions'
  | 'sessionDetail'
  | 'summaryLevels'
  | 'models'
  | 'jobConfigs'
  | 'jobRuns';

type AuthUser = {
  user_id: string;
  role: string;
};

function Shell() {
  const { lang, setLang, t } = useI18n();
  const [tab, setTab] = useState<TabKey>('dashboard');
  const [selectedSessionId, setSelectedSessionId] = useState<string | undefined>(undefined);
  const [authUser, setAuthUser] = useState<AuthUser | null>(null);
  const [bootLoading, setBootLoading] = useState(true);
  const [loginLoading, setLoginLoading] = useState(false);
  const [loginError, setLoginError] = useState<string | null>(null);
  const [username, setUsername] = useState('admin');
  const [password, setPassword] = useState('admin');
  const [adminSessionFilter, setAdminSessionFilter] = useState('');

  const isAdmin = authUser?.role === 'admin';
  const scopeUserId = authUser?.user_id || '';
  const sessionListUserFilter = isAdmin ? adminSessionFilter.trim() || undefined : scopeUserId;

  useEffect(() => {
    const init = async () => {
      const token = localStorage.getItem('memory_auth_token');
      if (!token) {
        setBootLoading(false);
        return;
      }
      try {
        const me = await api.me();
        setAuthUser(me);
      } catch {
        localStorage.removeItem('memory_auth_token');
      } finally {
        setBootLoading(false);
      }
    };

    init();
  }, []);

  const menuItems = useMemo(
    () => [
      { key: 'dashboard', icon: <BarChartOutlined />, label: t('nav.dashboard') },
      { key: 'sessions', icon: <DatabaseOutlined />, label: t('nav.sessions') },
      { key: 'sessionDetail', icon: <FileTextOutlined />, label: t('nav.sessionDetail') },
      { key: 'summaryLevels', icon: <NodeIndexOutlined />, label: t('nav.summaryLevels') },
      { key: 'models', icon: <ApiOutlined />, label: t('nav.models') },
      { key: 'jobConfigs', icon: <SettingOutlined />, label: t('nav.jobConfigs') },
      ...(isAdmin
        ? [{ key: 'jobRuns', icon: <HistoryOutlined />, label: t('nav.jobRuns') }]
        : []),
    ],
    [t, isAdmin],
  );

  const doLogin = async () => {
    setLoginError(null);
    setLoginLoading(true);
    try {
      const data = await api.login(username.trim(), password);
      localStorage.setItem('memory_auth_token', data.token);
      setAuthUser({ user_id: data.user_id, role: data.role });
      setTab('sessions');
    } catch (err) {
      setLoginError((err as Error).message);
    } finally {
      setLoginLoading(false);
    }
  };

  const logout = () => {
    localStorage.removeItem('memory_auth_token');
    setAuthUser(null);
    setSelectedSessionId(undefined);
    setTab('dashboard');
  };

  return (
    <ConfigProvider
      locale={lang === 'en-US' ? enUS : zhCN}
      theme={{
        token: {
          colorPrimary: '#005b4f',
          borderRadius: 10,
          fontFamily: '"IBM Plex Sans","Noto Sans SC","Source Han Sans SC",sans-serif',
        },
      }}
    >
      <AntdApp>
        {bootLoading ? (
          <div className="auth-shell">
            <Spin />
          </div>
        ) : !authUser ? (
          <div className="auth-shell">
            <Card title={t('auth.loginTitle')} style={{ width: 380 }}>
              <Space direction="vertical" style={{ width: '100%' }} size={10}>
                {loginError && <Alert type="error" showIcon message={loginError} />}
                <Input
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder={t('auth.username')}
                  prefix={<UserOutlined />}
                />
                <Input.Password
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={t('auth.password')}
                  onPressEnter={doLogin}
                />
                <Button type="primary" block loading={loginLoading} onClick={doLogin}>
                  {t('auth.login')}
                </Button>
                <Segmented
                  value={lang}
                  options={[
                    { label: t('lang.zh'), value: 'zh-CN' },
                    { label: t('lang.en'), value: 'en-US' },
                  ]}
                  onChange={(value) => setLang(value as 'zh-CN' | 'en-US')}
                />
              </Space>
            </Card>
          </div>
        ) : (
          <Layout style={{ minHeight: '100vh' }}>
            <Sider breakpoint="lg" collapsedWidth={0} theme="light" width={240}>
              <div className="brand">
                <Title level={4} style={{ margin: 0 }}>
                  {t('app.title')}
                </Title>
                <Text type="secondary">{t('app.subtitle')}</Text>
              </div>
              <Menu
                mode="inline"
                selectedKeys={[tab]}
                items={menuItems}
                onSelect={(e) => setTab(e.key as TabKey)}
              />
            </Sider>
            <Layout>
              <Header className="top-header">
                <Space wrap>
                  <Tag color={isAdmin ? 'gold' : 'default'}>
                    {authUser.user_id} ({authUser.role})
                  </Tag>
                  {isAdmin && (
                    <Input
                      value={adminSessionFilter}
                      onChange={(e) => setAdminSessionFilter(e.target.value)}
                      placeholder={t('top.userFilter')}
                      style={{ width: 200 }}
                    />
                  )}
                  <Button icon={<LogoutOutlined />} onClick={logout}>
                    {t('auth.logout')}
                  </Button>
                  <Segmented
                    value={lang}
                    options={[
                      { label: t('lang.zh'), value: 'zh-CN' },
                      { label: t('lang.en'), value: 'en-US' },
                    ]}
                    onChange={(value) => setLang(value as 'zh-CN' | 'en-US')}
                  />
                </Space>
                <Text type="secondary">
                  {t('top.selectedSession')}: {selectedSessionId || t('top.none')}
                </Text>
              </Header>
              <Content className="page-shell">
                {tab === 'dashboard' && <DashboardPage />}
                {tab === 'sessions' && (
                  <SessionsPage
                    filterUserId={sessionListUserFilter}
                    currentUserId={scopeUserId}
                    isAdmin={Boolean(isAdmin)}
                    selectedSessionId={selectedSessionId}
                    onSelectSession={setSelectedSessionId}
                  />
                )}
                {tab === 'sessionDetail' && <SessionDetailPage sessionId={selectedSessionId} />}
                {tab === 'summaryLevels' && <SummaryLevelsPage sessionId={selectedSessionId} />}
                {tab === 'models' && <ModelConfigsPage userId={scopeUserId} />}
                {tab === 'jobConfigs' && (
                  <JobConfigsPage userId={scopeUserId} selectedSessionId={selectedSessionId} />
                )}
                {tab === 'jobRuns' && isAdmin && <JobRunsPage />}
              </Content>
            </Layout>
          </Layout>
        )}
      </AntdApp>
    </ConfigProvider>
  );
}

function App() {
  return (
    <I18nProvider>
      <Shell />
    </I18nProvider>
  );
}

export default App;
