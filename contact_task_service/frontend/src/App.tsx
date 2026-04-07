import {
  LogoutOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import {
  Alert,
  App as AntdApp,
  Button,
  Card,
  Collapse,
  ConfigProvider,
  Input,
  Layout,
  Pagination,
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
import type {
  AuthUser,
  ContactTask,
  MemoryContactSummary,
  MemoryProjectSummary,
  TaskExecutionMessage,
  TaskResultBrief,
} from './types';

const { Header, Content } = Layout;
const { Title, Text, Paragraph } = Typography;

const BUILTIN_MCP_LABELS: Record<string, string> = {
  builtin_code_maintainer_read: '查看',
  builtin_code_maintainer_write: '读写',
  builtin_task_planner: '任务',
  builtin_terminal_controller: '终端',
  builtin_remote_connection_controller: '远程连接',
  builtin_notepad: 'Notepad',
  builtin_agent_builder: 'Agent Builder',
  builtin_ui_prompter: 'UI Prompter',
  builtin_task_executor: '任务执行',
};

const ASSET_TYPE_LABELS: Record<string, string> = {
  skill: '技能',
  plugin: '插件',
  common: 'Commons',
};

const EXECUTION_PAGE_SIZE = 8;

function formatBuiltinMcpLabel(id: string): string {
  return BUILTIN_MCP_LABELS[id] || id;
}

function formatAssetTypeLabel(assetType?: string | null): string {
  if (!assetType) {
    return '资产';
  }
  return ASSET_TYPE_LABELS[assetType] || assetType;
}

function stringifyPretty(value: unknown): string {
  if (value == null) {
    return '';
  }
  if (typeof value === 'string') {
    return value;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function truncateText(value?: string | null, maxLength = 140): string {
  const normalized = (value || '').trim();
  if (!normalized) {
    return '-';
  }
  if (normalized.length <= maxLength) {
    return normalized;
  }
  return `${normalized.slice(0, maxLength)}...`;
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function extractToolEntries(value: unknown): Array<Record<string, unknown>> {
  if (Array.isArray(value)) {
    return value.filter((item): item is Record<string, unknown> => isObjectRecord(item));
  }
  if (isObjectRecord(value)) {
    return [value];
  }
  return [];
}

function getToolEntryLabel(entry: Record<string, unknown>, index: number): string {
  const directName = typeof entry.name === 'string' ? entry.name : null;
  const directType = typeof entry.type === 'string' ? entry.type : null;
  const fnBlock = isObjectRecord(entry.function) ? entry.function : null;
  const fnName = fnBlock && typeof fnBlock.name === 'string' ? fnBlock.name : null;
  return fnName || directName || directType || `工具调用 ${index + 1}`;
}

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
  const [resultBriefByTaskId, setResultBriefByTaskId] = useState<Record<string, TaskResultBrief | null | undefined>>({});
  const [resultBriefLoadingByTaskId, setResultBriefLoadingByTaskId] = useState<Record<string, boolean>>({});
  const [resultBriefErrorByTaskId, setResultBriefErrorByTaskId] = useState<Record<string, string | null>>({});
  const [expandedTaskIds, setExpandedTaskIds] = useState<string[]>([]);
  const [executionSectionExpandedByTaskId, setExecutionSectionExpandedByTaskId] = useState<Record<string, boolean>>({});
  const [executionPageByTaskId, setExecutionPageByTaskId] = useState<Record<string, number>>({});
  const [contactNameByScopeKey, setContactNameByScopeKey] = useState<Record<string, string>>({});
  const [projectNameByScopeKey, setProjectNameByScopeKey] = useState<Record<string, string>>({});

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

  useEffect(() => {
    if (!authUser || tasks.length === 0) {
      setContactNameByScopeKey({});
      setProjectNameByScopeKey({});
      return;
    }

    let cancelled = false;

    const loadScopeDisplayNames = async () => {
      const userIds = Array.from(
        new Set(
          tasks
            .map((task) => task.user_id?.trim())
            .filter((value): value is string => Boolean(value)),
        ),
      );

      const contactEntries = await Promise.all(
        userIds.map(async (userId) => {
          try {
            const rows = await api.listMemoryContacts(userId);
            return rows.map((item) => {
              const scopeKey = `${item.user_id}:${item.agent_id}`;
              const displayName = item.agent_name_snapshot?.trim() || item.agent_id;
              return [scopeKey, displayName] as const;
            });
          } catch {
            return [] as Array<readonly [string, string]>;
          }
        }),
      );

      const projectEntries = await Promise.all(
        userIds.map(async (userId) => {
          try {
            const rows = await api.listMemoryProjects(userId);
            return rows.map((item) => {
              const scopeKey = `${item.user_id}:${item.project_id}`;
              const displayName = item.name?.trim() || item.project_id;
              return [scopeKey, displayName] as const;
            });
          } catch {
            return [] as Array<readonly [string, string]>;
          }
        }),
      );

      if (cancelled) {
        return;
      }

      setContactNameByScopeKey(Object.fromEntries(contactEntries.flat()));
      setProjectNameByScopeKey(Object.fromEntries(projectEntries.flat()));
    };

    void loadScopeDisplayNames();

    return () => {
      cancelled = true;
    };
  }, [authUser, tasks]);

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
    setResultBriefByTaskId({});
    setResultBriefLoadingByTaskId({});
    setResultBriefErrorByTaskId({});
    setExecutionSectionExpandedByTaskId({});
    setExecutionPageByTaskId({});
    setContactNameByScopeKey({});
    setProjectNameByScopeKey({});
  };

  const getContactDisplayName = useMemo(() => (
    (record: ContactTask): string => {
      const scopeKey = `${record.user_id}:${record.contact_agent_id}`;
      return contactNameByScopeKey[scopeKey] || record.contact_agent_id || '-';
    }
  ), [contactNameByScopeKey]);

  const getProjectDisplayName = useMemo(() => (
    (record: ContactTask): string => {
      if (record.project_id === '0') {
        return '未指定项目';
      }
      const scopeKey = `${record.user_id}:${record.project_id}`;
      return projectNameByScopeKey[scopeKey] || record.project_id || '-';
    }
  ), [projectNameByScopeKey]);

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

  const loadTaskResultBrief = async (taskId: string, force = false) => {
    if (!taskId) {
      return;
    }
    if (!force && (resultBriefLoadingByTaskId[taskId] || Object.prototype.hasOwnProperty.call(resultBriefByTaskId, taskId))) {
      return;
    }
    setResultBriefLoadingByTaskId((prev) => ({ ...prev, [taskId]: true }));
    setResultBriefErrorByTaskId((prev) => ({ ...prev, [taskId]: null }));
    try {
      const item = await api.getTaskResultBrief(taskId);
      setResultBriefByTaskId((prev) => ({ ...prev, [taskId]: item }));
    } catch (err) {
      setResultBriefErrorByTaskId((prev) => ({
        ...prev,
        [taskId]: (err as Error).message,
      }));
    } finally {
      setResultBriefLoadingByTaskId((prev) => ({ ...prev, [taskId]: false }));
    }
  };

  useEffect(() => {
    if (!authUser || expandedTaskIds.length === 0) {
      return;
    }
    for (const taskId of expandedTaskIds) {
      void loadExecutionMessages(taskId, true);
      void loadTaskResultBrief(taskId, true);
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
        void loadTaskResultBrief(taskId, true);
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
            <Text>用户: {record.user_id}</Text>
            <Text>
              联系人:
              {' '}
              {getContactDisplayName(record)}
            </Text>
            <Text type="secondary">
              项目:
              {' '}
              {getProjectDisplayName(record)}
            </Text>
            <Text type="secondary">
              IDs:
              {' '}
              {record.contact_agent_id}
              {' / '}
              {record.project_id}
            </Text>
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
        title: '计划资源',
        key: 'planned_resources',
        render: (_: unknown, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text>{record.planned_builtin_mcp_ids?.length ?? 0} 个 MCP</Text>
            <Text type="secondary">{record.planned_context_assets?.length ?? 0} 个上下文资产</Text>
          </Space>
        ),
      },
      {
        title: '更新时间',
        dataIndex: 'updated_at',
        key: 'updated_at',
        render: (value: string) => new Date(value).toLocaleString(),
      },
    ],
    [getContactDisplayName, getProjectDisplayName],
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
                        void loadTaskResultBrief(record.id, true);
                      }
                    },
                    expandedRowRender: (record) => {
                      const executionMessages = executionMessagesByTaskId[record.id] || [];
                      const executionPage = executionPageByTaskId[record.id] || 1;
                      const safeExecutionPage = Math.min(
                        executionPage,
                        Math.max(1, Math.ceil(executionMessages.length / EXECUTION_PAGE_SIZE)),
                      );
                      const pagedExecutionMessages = executionMessages.slice(
                        (safeExecutionPage - 1) * EXECUTION_PAGE_SIZE,
                        safeExecutionPage * EXECUTION_PAGE_SIZE,
                      );

                      return (
                      <Space direction="vertical" size={8} style={{ width: '100%' }}>
                        <Text strong>任务内容</Text>
                        <Paragraph style={{ marginBottom: 0 }}>{record.content}</Paragraph>
                        <Text strong>计划使用的内置 MCP</Text>
                        {(record.planned_builtin_mcp_ids?.length ?? 0) > 0 ? (
                          <Space wrap>
                            {record.planned_builtin_mcp_ids.map((mcpId) => (
                              <Tag key={mcpId} color="processing">
                                {formatBuiltinMcpLabel(mcpId)}
                                {' '}
                                ({mcpId})
                              </Tag>
                            ))}
                          </Space>
                        ) : (
                          <Text type="secondary">未配置计划使用的内置 MCP。</Text>
                        )}
                        <Text strong>计划使用的上下文资产</Text>
                        {(record.planned_context_assets?.length ?? 0) > 0 ? (
                          <Space direction="vertical" size={8} style={{ width: '100%' }}>
                            {record.planned_context_assets.map((asset) => (
                              <Card
                                key={`${asset.asset_type}:${asset.asset_id}`}
                                size="small"
                                bodyStyle={{ padding: 12 }}
                                style={{ width: '100%' }}
                              >
                                <Space direction="vertical" size={4} style={{ width: '100%' }}>
                                  <Space wrap>
                                    <Tag color="cyan">{formatAssetTypeLabel(asset.asset_type)}</Tag>
                                    <Text strong>{asset.display_name || asset.asset_id}</Text>
                                  </Space>
                                  <Text type="secondary">ID: {asset.asset_id}</Text>
                                  {asset.source_type && (
                                    <Text type="secondary">来源类型: {asset.source_type}</Text>
                                  )}
                                  {asset.source_path && (
                                    <Paragraph type="secondary" style={{ marginBottom: 0 }}>
                                      来源路径: {asset.source_path}
                                    </Paragraph>
                                  )}
                                </Space>
                              </Card>
                            ))}
                          </Space>
                        ) : (
                          <Text type="secondary">未配置计划使用的上下文资产。</Text>
                        )}
                        <Text strong>执行上下文</Text>
                        <Space direction="vertical" size={4} style={{ width: '100%' }}>
                          {record.project_root ? (
                            <Paragraph style={{ marginBottom: 0 }}>
                              项目路径:
                              {' '}
                              {record.project_root}
                            </Paragraph>
                          ) : (
                            <Text type="secondary">未记录 project_root。</Text>
                          )}
                          {record.remote_connection_id ? (
                            <Text type="secondary">
                              远程连接:
                              {' '}
                              {record.remote_connection_id}
                            </Text>
                          ) : (
                            <Text type="secondary">未记录 remote_connection_id。</Text>
                          )}
                        </Space>
                        {record.execution_result_contract && (
                          <>
                            <Text strong>结果要求</Text>
                            <Space wrap>
                              <Tag color={record.execution_result_contract.result_required ? 'green' : 'default'}>
                                {record.execution_result_contract.result_required ? '必须产出结果' : '结果非必填'}
                              </Tag>
                              {record.execution_result_contract.preferred_format && (
                                <Tag>{record.execution_result_contract.preferred_format}</Tag>
                              )}
                            </Space>
                          </>
                        )}
                        {record.planning_snapshot && (
                          <>
                            <Text strong>规划快照</Text>
                            <Space direction="vertical" size={6} style={{ width: '100%' }}>
                              {record.planning_snapshot.selected_model_config_id && (
                                <Text type="secondary">
                                  规划时模型配置:
                                  {' '}
                                  {record.planning_snapshot.selected_model_config_id}
                                </Text>
                              )}
                              {record.planning_snapshot.planned_at && (
                                <Text type="secondary">
                                  规划时间:
                                  {' '}
                                  {new Date(record.planning_snapshot.planned_at).toLocaleString()}
                                </Text>
                              )}
                              {record.planning_snapshot.source_user_goal_summary && (
                                <>
                                  <Text type="secondary">来源用户目标摘要:</Text>
                                  <Paragraph style={{ marginBottom: 0 }}>
                                    {record.planning_snapshot.source_user_goal_summary}
                                  </Paragraph>
                                </>
                              )}
                              {record.planning_snapshot.source_constraints_summary && (
                                <>
                                  <Text type="secondary">来源约束摘要:</Text>
                                  <Paragraph style={{ marginBottom: 0 }}>
                                    {record.planning_snapshot.source_constraints_summary}
                                  </Paragraph>
                                </>
                              )}
                              <Text type="secondary">当时联系人已授权的内置 MCP:</Text>
                              {(record.planning_snapshot.contact_authorized_builtin_mcp_ids?.length ?? 0) > 0 ? (
                                <Space wrap>
                                  {record.planning_snapshot.contact_authorized_builtin_mcp_ids.map((mcpId) => (
                                    <Tag key={`authorized-${mcpId}`}>
                                      {formatBuiltinMcpLabel(mcpId)}
                                      {' '}
                                      ({mcpId})
                                    </Tag>
                                  ))}
                                </Space>
                              ) : (
                                <Text type="secondary">当时没有可用的联系人授权内置 MCP。</Text>
                              )}
                            </Space>
                          </>
                        )}
                        {record.result_summary && (
                          <>
                            <Text strong>执行结果摘要</Text>
                            <Paragraph style={{ marginBottom: 0 }}>{record.result_summary}</Paragraph>
                          </>
                        )}
                        <Text strong>任务结果桥接摘要</Text>
                        <Space>
                          <Button
                            size="small"
                            icon={<ReloadOutlined />}
                            loading={resultBriefLoadingByTaskId[record.id]}
                            onClick={() => { void loadTaskResultBrief(record.id, true); }}
                          >
                            刷新结果桥接
                          </Button>
                        </Space>
                        {resultBriefErrorByTaskId[record.id] && (
                          <Alert
                            type="error"
                            showIcon
                            message={resultBriefErrorByTaskId[record.id] || '加载任务结果桥接失败'}
                          />
                        )}
                        {resultBriefLoadingByTaskId[record.id] && (
                          <Space>
                            <Spin size="small" />
                            <Text type="secondary">任务结果桥接加载中...</Text>
                          </Space>
                        )}
                        {!resultBriefLoadingByTaskId[record.id]
                          && !resultBriefErrorByTaskId[record.id]
                          && resultBriefByTaskId[record.id] === null && (
                          <Text type="secondary">当前任务还没有生成结果桥接摘要，通常会在任务进入终态后出现。</Text>
                        )}
                        {resultBriefByTaskId[record.id] && (
                          <Card size="small" bodyStyle={{ padding: 12 }} style={{ width: '100%' }}>
                            <Space direction="vertical" size={6} style={{ width: '100%' }}>
                              <Space wrap>
                                <Tag color={resultBriefByTaskId[record.id]?.task_status === 'completed' ? 'green' : resultBriefByTaskId[record.id]?.task_status === 'failed' ? 'red' : 'default'}>
                                  {resultBriefByTaskId[record.id]?.task_status}
                                </Tag>
                                {resultBriefByTaskId[record.id]?.result_format && (
                                  <Tag>{resultBriefByTaskId[record.id]?.result_format}</Tag>
                                )}
                                {resultBriefByTaskId[record.id]?.finished_at && (
                                  <Text type="secondary">
                                    完成时间:
                                    {' '}
                                    {new Date(resultBriefByTaskId[record.id]!.finished_at as string).toLocaleString()}
                                  </Text>
                                )}
                              </Space>
                              <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                {resultBriefByTaskId[record.id]?.result_summary}
                              </Paragraph>
                              {resultBriefByTaskId[record.id]?.result_message_id && (
                                <Text type="secondary">
                                  结果消息 ID:
                                  {' '}
                                  {resultBriefByTaskId[record.id]?.result_message_id}
                                </Text>
                              )}
                              {resultBriefByTaskId[record.id]?.source_session_id && (
                                <Text type="secondary">
                                  来源会话:
                                  {' '}
                                  {resultBriefByTaskId[record.id]?.source_session_id}
                                </Text>
                              )}
                            </Space>
                          </Card>
                        )}
                        {record.last_error && (
                          <>
                            <Text strong>最后错误</Text>
                            <Alert type="error" showIcon message={record.last_error} />
                          </>
                        )}
                        <Collapse
                          ghost
                          activeKey={executionSectionExpandedByTaskId[record.id] ? ['execution'] : []}
                          onChange={(keys) => {
                            const nextKeys = Array.isArray(keys) ? keys : [keys];
                            setExecutionSectionExpandedByTaskId((prev) => ({
                              ...prev,
                              [record.id]: nextKeys.includes('execution'),
                            }));
                          }}
                          items={[
                            {
                              key: 'execution',
                              label: (
                                <Space wrap>
                                  <Text strong>执行过程</Text>
                                  <Tag>{executionMessages.length} 条</Tag>
                                  {executionLoadingByTaskId[record.id] && <Tag color="processing">加载中</Tag>}
                                </Space>
                              ),
                              children: (
                                <Space direction="vertical" size={10} style={{ width: '100%' }}>
                                  <Space>
                                    <Button
                                      size="small"
                                      icon={<ReloadOutlined />}
                                      loading={executionLoadingByTaskId[record.id]}
                                      onClick={(event) => {
                                        event.stopPropagation();
                                        void loadExecutionMessages(record.id, true);
                                      }}
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
                                    && executionMessages.length === 0 && (
                                    <Text type="secondary">暂无执行记录。</Text>
                                  )}
                                  {pagedExecutionMessages.map((message) => {
                                    const toolEntries = extractToolEntries(message.tool_calls);
                                    const hasToolDetails =
                                      message.role === 'tool'
                                      || toolEntries.length > 0
                                      || Boolean(message.tool_call_id);

                                    return (
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
                                            {message.summary_status && <Tag>{message.summary_status}</Tag>}
                                            <Text type="secondary">{new Date(message.created_at).toLocaleString()}</Text>
                                          </Space>
                                          {hasToolDetails ? (
                                            <>
                                              <Text type="secondary">
                                                {truncateText(message.content, 160)}
                                              </Text>
                                              <Collapse
                                                ghost
                                                items={[
                                                  {
                                                    key: 'tool-details',
                                                    label: (
                                                      <Space wrap>
                                                        <Text>工具调用详情</Text>
                                                        {toolEntries.length > 0 && <Tag>{toolEntries.length} 个</Tag>}
                                                      </Space>
                                                    ),
                                                    children: (
                                                      <Space direction="vertical" size={8} style={{ width: '100%' }}>
                                                        <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                                          {message.content || '-'}
                                                        </Paragraph>
                                                        {message.tool_call_id && (
                                                          <Text type="secondary">
                                                            tool_call_id:
                                                            {' '}
                                                            {message.tool_call_id}
                                                          </Text>
                                                        )}
                                                        {toolEntries.length > 0 && (
                                                          <Space direction="vertical" size={8} style={{ width: '100%' }}>
                                                            {toolEntries.map((entry, index) => (
                                                              <Card
                                                                key={`${message.id}-tool-${index}`}
                                                                size="small"
                                                                bodyStyle={{ padding: 10 }}
                                                              >
                                                                <Space direction="vertical" size={6} style={{ width: '100%' }}>
                                                                  <Text strong>{getToolEntryLabel(entry, index)}</Text>
                                                                  <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                                                                    {stringifyPretty(entry)}
                                                                  </pre>
                                                                </Space>
                                                              </Card>
                                                            ))}
                                                          </Space>
                                                        )}
                                                        {message.metadata && (
                                                          <Card size="small" bodyStyle={{ padding: 10 }}>
                                                            <Space direction="vertical" size={4} style={{ width: '100%' }}>
                                                              <Text strong>metadata</Text>
                                                              <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                                                                {stringifyPretty(message.metadata)}
                                                              </pre>
                                                            </Space>
                                                          </Card>
                                                        )}
                                                        {message.reasoning && (
                                                          <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                                            {message.reasoning}
                                                          </Paragraph>
                                                        )}
                                                      </Space>
                                                    ),
                                                  },
                                                ]}
                                              />
                                            </>
                                          ) : (
                                            <>
                                              <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                                {message.content || '-'}
                                              </Paragraph>
                                              {message.reasoning && (
                                                <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                                  {message.reasoning}
                                                </Paragraph>
                                              )}
                                              {message.metadata && (
                                                <Collapse
                                                  ghost
                                                  items={[
                                                    {
                                                      key: 'message-metadata',
                                                      label: '附加信息',
                                                      children: (
                                                        <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                                                          {stringifyPretty(message.metadata)}
                                                        </pre>
                                                      ),
                                                    },
                                                  ]}
                                                />
                                              )}
                                            </>
                                          )}
                                        </Space>
                                      </Card>
                                    );
                                  })}
                                  {executionMessages.length > EXECUTION_PAGE_SIZE && (
                                    <Pagination
                                      align="end"
                                      current={safeExecutionPage}
                                      pageSize={EXECUTION_PAGE_SIZE}
                                      total={executionMessages.length}
                                      showSizeChanger={false}
                                      onChange={(page) => {
                                        setExecutionPageByTaskId((prev) => ({ ...prev, [record.id]: page }));
                                      }}
                                    />
                                  )}
                                </Space>
                              ),
                            },
                          ]}
                        />
                      </Space>
                      );
                    },
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
