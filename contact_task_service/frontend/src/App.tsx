import {
  LogoutOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import {
  Alert,
  App as AntdApp,
  Button,
  Card,
  ConfigProvider,
  Input,
  Layout,
  Select,
  Space,
  Spin,
  Table,
  Tag,
  Typography,
} from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { useEffect, useMemo, useState } from 'react';

import { api, authStore } from './api';
import type { AuthUser, ContactTask, TaskExecutionMessage } from './types';

const { Header, Content } = Layout;
const { Title, Text, Paragraph } = Typography;

function App() {
  const [authUser, setAuthUser] = useState<AuthUser | null>(null);
  const [bootLoading, setBootLoading] = useState(true);
  const [loading, setLoading] = useState(false);
  const [loginLoading, setLoginLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [username, setUsername] = useState('admin');
  const [password, setPassword] = useState('admin');
  const [tasks, setTasks] = useState<ContactTask[]>([]);
  const [status, setStatus] = useState<string | undefined>(undefined);
  const [userIdFilter, setUserIdFilter] = useState('');
  const [contactAgentId, setContactAgentId] = useState('');
  const [projectId, setProjectId] = useState('');
  const [executionMessagesByTaskId, setExecutionMessagesByTaskId] = useState<Record<string, TaskExecutionMessage[]>>({});
  const [executionLoadingByTaskId, setExecutionLoadingByTaskId] = useState<Record<string, boolean>>({});
  const [executionErrorByTaskId, setExecutionErrorByTaskId] = useState<Record<string, string | null>>({});
  const [expandedTaskIds, setExpandedTaskIds] = useState<string[]>([]);

  useEffect(() => {
    const init = async () => {
      if (!authStore.getToken()) {
        setBootLoading(false);
        return;
      }
      try {
        const me = await api.me();
        setAuthUser(me);
      } catch {
        authStore.clear();
      } finally {
        setBootLoading(false);
      }
    };
    void init();
  }, []);

  const isAdmin = authUser?.role === 'admin';

  const refresh = async () => {
    if (!authUser) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const rows = await api.listTasks({
        user_id: isAdmin ? userIdFilter.trim() || undefined : undefined,
        contact_agent_id: contactAgentId.trim() || undefined,
        project_id: projectId.trim() || undefined,
        status,
      });
      setTasks(rows);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (authUser) {
      void refresh();
    }
  }, [authUser]);

  const doLogin = async () => {
    setLoginLoading(true);
    setError(null);
    try {
      const data = await api.login(username.trim(), password);
      authStore.setToken(data.token);
      setAuthUser({ username: data.username, role: data.role });
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoginLoading(false);
    }
  };

  const logout = () => {
    authStore.clear();
    setAuthUser(null);
    setTasks([]);
    setExpandedTaskIds([]);
    setExecutionMessagesByTaskId({});
    setExecutionLoadingByTaskId({});
    setExecutionErrorByTaskId({});
  };

  const loadExecutionMessages = async (taskId: string, force = false) => {
    if (!taskId) {
      return;
    }
    if (!force && (executionLoadingByTaskId[taskId] || executionMessagesByTaskId[taskId])) {
      return;
    }
    setExecutionLoadingByTaskId((prev) => ({ ...prev, [taskId]: true }));
    setExecutionErrorByTaskId((prev) => ({ ...prev, [taskId]: null }));
    try {
      const items = await api.listTaskExecutionMessages(taskId);
      setExecutionMessagesByTaskId((prev) => ({ ...prev, [taskId]: items }));
    } catch (err) {
      setExecutionErrorByTaskId((prev) => ({
        ...prev,
        [taskId]: (err as Error).message,
      }));
    } finally {
      setExecutionLoadingByTaskId((prev) => ({ ...prev, [taskId]: false }));
    }
  };

  useEffect(() => {
    if (!authUser || expandedTaskIds.length === 0) {
      return;
    }
    for (const taskId of expandedTaskIds) {
      void loadExecutionMessages(taskId, true);
    }
  }, [tasks]);

  useEffect(() => {
    if (!authUser) {
      return;
    }
    const runningExpandedTaskIds = expandedTaskIds.filter((taskId) =>
      tasks.some((task) => task.id === taskId && task.status === 'running'),
    );
    if (runningExpandedTaskIds.length === 0) {
      return;
    }
    const timer = window.setInterval(() => {
      void refresh();
      for (const taskId of runningExpandedTaskIds) {
        void loadExecutionMessages(taskId, true);
      }
    }, 5000);
    return () => window.clearInterval(timer);
  }, [authUser, expandedTaskIds, tasks, status, userIdFilter, contactAgentId, projectId]);

  const columns = useMemo(
    () => [
      {
        title: '任务',
        dataIndex: 'title',
        key: 'title',
        render: (value: string, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text strong>{value}</Text>
            <Text type="secondary">{record.id.slice(0, 8)}</Text>
          </Space>
        ),
      },
      {
        title: '用户 / 联系人 / 项目',
        key: 'scope',
        render: (_: unknown, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text>{record.user_id}</Text>
            <Text type="secondary">{record.contact_agent_id} / {record.project_id}</Text>
          </Space>
        ),
      },
      {
        title: '优先级',
        dataIndex: 'priority',
        key: 'priority',
        render: (value: string) => (
          <Tag color={value === 'high' ? 'red' : value === 'low' ? 'default' : 'blue'}>
            {value}
          </Tag>
        ),
      },
      {
        title: '状态',
        dataIndex: 'status',
        key: 'status',
        render: (value: string) => (
          <Tag
            color={
              value === 'running'
                ? 'gold'
                : value === 'completed'
                  ? 'green'
                  : value === 'failed'
                    ? 'red'
                    : value === 'pending_confirm'
                      ? 'orange'
                      : 'default'
            }
          >
            {value}
          </Tag>
        ),
      },
      {
        title: '执行模型',
        dataIndex: 'model_config_id',
        key: 'model_config_id',
        render: (value?: string | null) => value || '-',
      },
      {
        title: '更新时间',
        dataIndex: 'updated_at',
        key: 'updated_at',
        render: (value: string) => new Date(value).toLocaleString(),
      },
    ],
    [],
  );

  return (
    <ConfigProvider
      locale={zhCN}
      theme={{
        token: {
          colorPrimary: '#8b3a2e',
          borderRadius: 12,
          fontFamily: '"IBM Plex Sans","Noto Sans SC",sans-serif',
        },
      }}
    >
      <AntdApp>
        {bootLoading ? (
          <div className="auth-wrap">
            <Spin />
          </div>
        ) : !authUser ? (
          <div className="auth-wrap">
            <Card title="任务服务登录" style={{ width: 380 }}>
              <Space direction="vertical" size={12} style={{ width: '100%' }}>
                {error && <Alert type="error" showIcon message={error} />}
                <Input value={username} onChange={(e) => setUsername(e.target.value)} placeholder="用户名" />
                <Input.Password
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="密码"
                  onPressEnter={doLogin}
                />
                <Button type="primary" loading={loginLoading} onClick={doLogin}>
                  登录
                </Button>
              </Space>
            </Card>
          </div>
        ) : (
          <Layout className="page-shell">
            <Header style={{ background: 'transparent', padding: 0, marginBottom: 20 }}>
              <Card>
                <Space style={{ width: '100%', justifyContent: 'space-between' }} wrap>
                  <div>
                    <Title level={3} style={{ margin: 0 }}>Contact Task Service</Title>
                    <Text type="secondary">
                      任务域看板，AI 执行仍由 chatos 负责。
                    </Text>
                  </div>
                  <Space>
                    <Tag color={isAdmin ? 'gold' : 'default'}>
                      {authUser.username} ({authUser.role})
                    </Tag>
                    <Button icon={<LogoutOutlined />} onClick={logout}>退出</Button>
                  </Space>
                </Space>
              </Card>
            </Header>
            <Content>
              <Card
                title="任务看板"
                extra={(
                  <Space wrap>
                    {isAdmin && (
                      <Input
                        value={userIdFilter}
                        onChange={(e) => setUserIdFilter(e.target.value)}
                        placeholder="筛选 user_id"
                        style={{ width: 180 }}
                      />
                    )}
                    <Input
                      value={contactAgentId}
                      onChange={(e) => setContactAgentId(e.target.value)}
                      placeholder="筛选 contact_agent_id"
                      style={{ width: 210 }}
                    />
                    <Input
                      value={projectId}
                      onChange={(e) => setProjectId(e.target.value)}
                      placeholder="筛选 project_id"
                      style={{ width: 180 }}
                    />
                    <Select
                      allowClear
                      value={status}
                      onChange={(value) => setStatus(value)}
                      placeholder="状态"
                      style={{ width: 180 }}
                      options={[
                        { value: 'pending_confirm', label: 'pending_confirm' },
                        { value: 'pending_execute', label: 'pending_execute' },
                        { value: 'running', label: 'running' },
                        { value: 'completed', label: 'completed' },
                        { value: 'failed', label: 'failed' },
                        { value: 'cancelled', label: 'cancelled' },
                      ]}
                    />
                    <Button type="primary" icon={<ReloadOutlined />} loading={loading} onClick={() => { void refresh(); }}>
                      刷新
                    </Button>
                  </Space>
                )}
              >
                {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
                <Paragraph type="secondary" style={{ marginTop: 0 }}>
                  admin 可看所有任务；普通用户仅能看到自己的任务。调度接口在后端提供，不在看板前端直接触发。
                </Paragraph>
                <Table<ContactTask>
                  rowKey="id"
                  loading={loading}
                  columns={columns}
                  dataSource={tasks}
                  pagination={{ pageSize: 20, showSizeChanger: false }}
                  expandable={{
                    expandedRowKeys: expandedTaskIds,
                    expandRowByClick: true,
                    onExpand: (expanded, record) => {
                      setExpandedTaskIds((prev) => (
                        expanded
                          ? Array.from(new Set([...prev, record.id]))
                          : prev.filter((item) => item !== record.id)
                      ));
                      if (expanded) {
                        void loadExecutionMessages(record.id, true);
                      }
                    },
                    expandedRowRender: (record) => (
                      <Space direction="vertical" size={8} style={{ width: '100%' }}>
                        <Text strong>任务内容</Text>
                        <Paragraph style={{ marginBottom: 0 }}>{record.content}</Paragraph>
                        {record.result_summary && (
                          <>
                            <Text strong>执行结果摘要</Text>
                            <Paragraph style={{ marginBottom: 0 }}>{record.result_summary}</Paragraph>
                          </>
                        )}
                        {record.last_error && (
                          <>
                            <Text strong>最后错误</Text>
                            <Alert type="error" showIcon message={record.last_error} />
                          </>
                        )}
                        <Text strong>执行过程</Text>
                        <Space>
                          <Button
                            size="small"
                            icon={<ReloadOutlined />}
                            loading={executionLoadingByTaskId[record.id]}
                            onClick={() => { void loadExecutionMessages(record.id, true); }}
                          >
                            刷新执行记录
                          </Button>
                        </Space>
                        {executionErrorByTaskId[record.id] && (
                          <Alert
                            type="error"
                            showIcon
                            message={executionErrorByTaskId[record.id] || '加载执行过程失败'}
                          />
                        )}
                        {executionLoadingByTaskId[record.id] && (
                          <Space>
                            <Spin size="small" />
                            <Text type="secondary">执行记录加载中...</Text>
                          </Space>
                        )}
                        {!executionLoadingByTaskId[record.id]
                          && !executionErrorByTaskId[record.id]
                          && (executionMessagesByTaskId[record.id]?.length ?? 0) === 0 && (
                          <Text type="secondary">暂无执行记录。</Text>
                        )}
                        {(executionMessagesByTaskId[record.id] || []).map((message) => (
                          <Card
                            key={message.id}
                            size="small"
                            bodyStyle={{ padding: 12 }}
                            style={{ width: '100%' }}
                          >
                            <Space direction="vertical" size={6} style={{ width: '100%' }}>
                              <Space wrap>
                                <Tag color={message.role === 'assistant' ? 'blue' : message.role === 'tool' ? 'purple' : 'default'}>
                                  {message.role}
                                </Tag>
                                {message.message_source && <Tag>{message.message_source}</Tag>}
                                <Text type="secondary">{new Date(message.created_at).toLocaleString()}</Text>
                              </Space>
                              <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                {message.content}
                              </Paragraph>
                              {message.reasoning && (
                                <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                  {message.reasoning}
                                </Paragraph>
                              )}
                            </Space>
                          </Card>
                        ))}
                      </Space>
                    ),
                  }}
                />
              </Card>
            </Content>
          </Layout>
        )}
      </AntdApp>
    </ConfigProvider>
  );
}

export default App;
