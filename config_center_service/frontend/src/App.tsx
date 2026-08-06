import { useEffect, useMemo, useState } from 'react';
import {
  App as AntdApp,
  Button,
  Card,
  Col,
  Descriptions,
  Empty,
  Form,
  Input,
  InputNumber,
  Layout,
  List,
  Menu,
  Modal,
  Row,
  Select,
  Space,
  Spin,
  Statistic,
  Switch,
  Table,
  Tabs,
  Tag,
  Typography,
} from 'antd';
import {
  AuditOutlined,
  ClusterOutlined,
  CloudServerOutlined,
  DashboardOutlined,
  HistoryOutlined,
  LogoutOutlined,
  PlusOutlined,
  SaveOutlined,
  SettingOutlined,
} from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import dayjs from 'dayjs';
import { api, clearToken, getToken, setToken } from './api';
import type {
  ConfigDefinition,
  ConfigRelease,
  ConfigValue,
  CurrentUser,
  QueueOperationsStream,
} from './types';

type PageKey = 'dashboard' | 'config' | 'releases' | 'queues' | 'instances' | 'audit';

const CONFIG_AREA_META: Record<string, { label: string; order: number }> = {
  'chatos-backend': { label: 'Chat OS', order: 10 },
  'task-runner': { label: 'Task Runner', order: 20 },
  'mcp-management-service': { label: 'MCP 管理', order: 30 },
  'memory-engine': { label: 'Memory Engine', order: 40 },
  'project-service': { label: '项目服务', order: 50 },
  'sandbox-manager': { label: '沙箱管理', order: 60 },
  'user-service': { label: '用户服务', order: 70 },
  'plugin-management-service': { label: '插件管理', order: 80 },
  'local-connector-service': { label: '本地连接器', order: 90 },
  'configuration-center': { label: '配置中心', order: 100 },
  'official-website': { label: '官方网站', order: 110 },
  'platform-shared': { label: '平台与共享', order: 120 },
  developer: { label: '开发参数', order: 900 },
};

function configAreaKey(definition: ConfigDefinition) {
  const serviceName = definition.service_name?.trim();
  if (serviceName) {
    return serviceName;
  }
  const categoryRoot = definition.category.split('/')[0]?.trim().toLowerCase();
  if (categoryRoot === 'chat os') return 'chatos-backend';
  if (categoryRoot === 'task runner') return 'task-runner';
  if (categoryRoot === 'mcp management') return 'mcp-management-service';
  if (categoryRoot === 'memory engine') return 'memory-engine';
  if (categoryRoot === 'project service') return 'project-service';
  if (categoryRoot === 'sandbox manager') return 'sandbox-manager';
  if (categoryRoot === 'user service') return 'user-service';
  if (categoryRoot === 'plugin management') return 'plugin-management-service';
  if (categoryRoot === 'local connector') return 'local-connector-service';
  if (categoryRoot === 'configuration center') return 'configuration-center';
  if (categoryRoot === 'developer') return 'developer';
  return 'platform-shared';
}

function configAreaLabel(key: string) {
  return CONFIG_AREA_META[key]?.label || key;
}

function configCategoryLabel(category: string) {
  const [, ...rest] = category.split('/').map((part) => part.trim());
  return rest.length > 0 ? rest.join(' / ') : category;
}

export default function App() {
  const [authenticated, setAuthenticated] = useState(Boolean(getToken()));
  if (!authenticated) {
    return <LoginPage onAuthenticated={() => setAuthenticated(true)} />;
  }
  return <ConsoleApp onLogout={() => setAuthenticated(false)} />;
}

function LoginPage({ onAuthenticated }: { onAuthenticated: () => void }) {
  const { message } = AntdApp.useApp();
  const [form] = Form.useForm<{ username: string; password: string }>();
  const login = useMutation({
    mutationFn: ({ username, password }: { username: string; password: string }) =>
      api.login(username, password),
    onSuccess: (response) => {
      setToken(response.token);
      onAuthenticated();
    },
    onError: (error: Error) => message.error(error.message),
  });
  return (
    <div className="login-shell">
      <Card className="login-card">
        <Space direction="vertical" size={4} style={{ marginBottom: 24 }}>
          <Typography.Title level={2} style={{ margin: 0 }}>Chat OS 配置中心</Typography.Title>
          <Typography.Text type="secondary">使用平台超级管理员账号登录</Typography.Text>
        </Space>
        <Form form={form} layout="vertical" onFinish={(values) => login.mutate(values)}>
          <Form.Item name="username" label="用户名" rules={[{ required: true }]}>
            <Input autoFocus />
          </Form.Item>
          <Form.Item name="password" label="密码" rules={[{ required: true }]}>
            <Input.Password />
          </Form.Item>
          <Button type="primary" htmlType="submit" block loading={login.isPending}>
            登录
          </Button>
        </Form>
      </Card>
    </div>
  );
}

function ConsoleApp({ onLogout }: { onLogout: () => void }) {
  const queryClient = useQueryClient();
  const { message } = AntdApp.useApp();
  const [page, setPage] = useState<PageKey>('dashboard');
  const [environment, setEnvironment] = useState(
    localStorage.getItem('chatos.configuration-center.environment') || 'local',
  );
  const me = useQuery({ queryKey: ['me'], queryFn: api.me });

  useEffect(() => {
    if (me.error) {
      clearToken();
      onLogout();
    }
  }, [me.error, onLogout]);

  const updateEnvironment = (value: string) => {
    const normalized = value.trim() || 'local';
    localStorage.setItem('chatos.configuration-center.environment', normalized);
    setEnvironment(normalized);
    void queryClient.invalidateQueries();
  };

  const logout = () => {
    clearToken();
    message.success('已退出');
    onLogout();
  };

  return (
    <Layout className="app-shell">
      <Layout.Sider width={244} theme="light" className="sidebar">
        <div className="brand">
          <SettingOutlined />
          <div>
            <strong>配置中心</strong>
            <span>Configuration Center</span>
          </div>
        </div>
        <Menu
          mode="inline"
          selectedKeys={[page]}
          onClick={({ key }) => setPage(key as PageKey)}
          items={[
            { key: 'dashboard', icon: <DashboardOutlined />, label: '总览' },
            { key: 'config', icon: <SettingOutlined />, label: '配置管理' },
            { key: 'releases', icon: <HistoryOutlined />, label: '发布历史' },
            { key: 'queues', icon: <ClusterOutlined />, label: '队列运维' },
            { key: 'instances', icon: <CloudServerOutlined />, label: '服务实例' },
            { key: 'audit', icon: <AuditOutlined />, label: '审计日志' },
          ]}
        />
        <div className="sidebar-footer">
          <Typography.Text type="secondary">{me.data?.display_name || me.data?.username}</Typography.Text>
          <Button type="text" icon={<LogoutOutlined />} onClick={logout}>退出</Button>
        </div>
      </Layout.Sider>
      <Layout>
        <Layout.Header className="topbar">
          <Typography.Title level={4} style={{ margin: 0 }}>
            {pageTitle(page)}
          </Typography.Title>
          <Space>
            <Typography.Text type="secondary">环境</Typography.Text>
            <Select
              value={environment}
              onChange={updateEnvironment}
              style={{ width: 150 }}
              options={[
                { value: 'local', label: 'local' },
                { value: 'development', label: 'development' },
                { value: 'staging', label: 'staging' },
                { value: 'production', label: 'production' },
              ]}
            />
          </Space>
        </Layout.Header>
        <Layout.Content className="content">
          {page === 'dashboard' && <Dashboard environment={environment} />}
          {page === 'config' && <ConfigEditor environment={environment} />}
          {page === 'releases' && <ReleaseHistory environment={environment} />}
          {page === 'queues' && <QueueOperations environment={environment} />}
          {page === 'instances' && <Instances />}
          {page === 'audit' && <AuditLog />}
        </Layout.Content>
      </Layout>
    </Layout>
  );
}

function Dashboard({ environment }: { environment: string }) {
  const effective = useQuery({
    queryKey: ['effective', environment],
    queryFn: () => api.effective(environment),
  });
  const releases = useQuery({
    queryKey: ['releases', environment],
    queryFn: () => api.releases(environment),
  });
  const instances = useQuery({ queryKey: ['instances'], queryFn: api.instances });
  const matching = instances.data?.filter((item) => item.environment === environment) || [];
  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Row gutter={16}>
        <Col span={6}><Card><Statistic title="当前 Revision" value={effective.data?.revision || 0} /></Card></Col>
        <Col span={6}><Card><Statistic title="已发布版本" value={releases.data?.length || 0} /></Card></Col>
        <Col span={6}><Card><Statistic title="在线实例记录" value={matching.length} /></Card></Col>
        <Col span={6}><Card><Statistic title="待重启实例" value={matching.filter((item) => item.pending_restart_keys.length > 0).length} /></Card></Col>
      </Row>
      <Card title="当前配置">
        <Descriptions column={2}>
          <Descriptions.Item label="环境">{environment}</Descriptions.Item>
          <Descriptions.Item label="Release ID">{effective.data?.release_id || '-'}</Descriptions.Item>
          <Descriptions.Item label="配置数量">{Object.keys(effective.data?.values || {}).length}</Descriptions.Item>
          <Descriptions.Item label="状态"><Tag color="green">已发布</Tag></Descriptions.Item>
        </Descriptions>
      </Card>
      <Card title="最近发布">
        <List
          dataSource={(releases.data || []).slice(0, 6)}
          locale={{ emptyText: <Empty description="暂无发布记录" /> }}
          renderItem={(release) => (
            <List.Item>
              <List.Item.Meta
                title={<Space><Tag color="blue">r{release.revision}</Tag>{release.publish_message}</Space>}
                description={dayjs(release.published_at || release.created_at).format('YYYY-MM-DD HH:mm:ss')}
              />
              <Tag color={release.status === 'published' ? 'green' : 'red'}>{release.status}</Tag>
            </List.Item>
          )}
        />
      </Card>
    </Space>
  );
}

function ConfigEditor({ environment }: { environment: string }) {
  const { message, modal } = AntdApp.useApp();
  const queryClient = useQueryClient();
  const catalog = useQuery({ queryKey: ['catalog'], queryFn: api.catalog });
  const effective = useQuery({
    queryKey: ['effective', environment],
    queryFn: () => api.effective(environment),
  });
  const draft = useQuery({
    queryKey: ['draft', environment],
    queryFn: () => api.draft(environment),
  });
  const [changes, setChanges] = useState<Record<string, ConfigValue>>({});
  const [publishMessage, setPublishMessage] = useState('');
  const [customOpen, setCustomOpen] = useState(false);
  const [activeArea, setActiveArea] = useState(
    localStorage.getItem('chatos.configuration-center.config-area') || 'chatos-backend',
  );
  const [configSearch, setConfigSearch] = useState('');
  const [customForm] = Form.useForm<{
    key: string;
    display_name: string;
    service_name: string;
    value_type: string;
    default_value: string;
    reload_mode: string;
    env_alias: string;
  }>();

  useEffect(() => {
    setChanges(draft.data?.draft?.changes || {});
  }, [draft.data?.draft?.id, draft.data?.draft?.updated_at]);

  const save = useMutation({
    mutationFn: () => api.saveDraft(environment, changes),
    onSuccess: async () => {
      message.success('草稿已保存');
      await queryClient.invalidateQueries({ queryKey: ['draft', environment] });
    },
    onError: (error: Error) => message.error(error.message),
  });
  const publish = useMutation({
    mutationFn: async () => {
      await api.saveDraft(environment, changes);
      const validation = await api.validateDraft(environment);
      if (!validation.valid) {
        throw new Error(validation.errors.join('；'));
      }
      return api.publishDraft(environment, publishMessage || '更新平台配置');
    },
    onSuccess: async (release) => {
      message.success(`Revision ${release.revision} 发布成功`);
      setPublishMessage('');
      setChanges({});
      await queryClient.invalidateQueries();
    },
    onError: (error: Error) => message.error(error.message),
  });
  const createCustom = useMutation({
    mutationFn: async (values: {
      key: string;
      display_name: string;
      service_name: string;
      value_type: string;
      default_value: string;
      reload_mode: string;
      env_alias: string;
    }) => {
      const defaultValue: ConfigValue = values.value_type === 'boolean'
        ? values.default_value.trim().toLowerCase() === 'true'
        : ['integer', 'duration_ms', 'bytes'].includes(values.value_type)
          ? Number(values.default_value)
          : values.default_value;
      return api.createCustomDefinition({
        environment,
        key: values.key,
        display_name: values.display_name,
        category: 'Developer',
        scope: 'service',
        service_name: values.service_name,
        value_type: values.value_type,
        default_value: defaultValue,
        reload_mode: values.reload_mode,
        env_aliases: values.env_alias.trim() ? [values.env_alias.trim()] : [],
      });
    },
    onSuccess: async () => {
      message.success('开发参数已加入草稿');
      setCustomOpen(false);
      customForm.resetFields();
      await queryClient.invalidateQueries();
    },
    onError: (error: Error) => message.error(error.message),
  });

  const definitions = catalog.data || [];
  const areas = useMemo(() => {
    const next = new Map<string, ConfigDefinition[]>();
    definitions.forEach((definition) => {
      const key = configAreaKey(definition);
      const items = next.get(key) || [];
      items.push(definition);
      next.set(key, items);
    });
    return [...next.entries()]
      .map(([key, items]) => ({
        key,
        label: configAreaLabel(key),
        items: [...items].sort((left, right) => left.ui_order - right.ui_order),
      }))
      .sort((left, right) => {
        const order = (CONFIG_AREA_META[left.key]?.order ?? 500)
          - (CONFIG_AREA_META[right.key]?.order ?? 500);
        return order || left.label.localeCompare(right.label, 'zh-CN');
      });
  }, [definitions]);
  const selectedArea = areas.find((area) => area.key === activeArea) || areas[0];
  const normalizedSearch = configSearch.trim().toLocaleLowerCase('zh-CN');
  const visibleDefinitions = (selectedArea?.items || []).filter((definition) => {
    if (!normalizedSearch) return true;
    return [
      definition.display_name,
      definition.description,
      definition.key,
      definition.category,
      definition.service_name || '',
    ].some((value) => value.toLocaleLowerCase('zh-CN').includes(normalizedSearch));
  });
  const groups = useMemo(() => {
    const next = new Map<string, ConfigDefinition[]>();
    visibleDefinitions.forEach((definition) => {
      const items = next.get(definition.category) || [];
      items.push(definition);
      next.set(definition.category, items);
    });
    return [...next.entries()];
  }, [visibleDefinitions]);

  if (catalog.isLoading || effective.isLoading || draft.isLoading) {
    return <div className="centered"><Spin size="large" /></div>;
  }

  const currentValue = (definition: ConfigDefinition): ConfigValue =>
    Object.prototype.hasOwnProperty.call(changes, definition.key)
      ? changes[definition.key]
      : effective.data?.values[definition.key] ?? definition.default_value;

  const update = (definition: ConfigDefinition, value: ConfigValue) => {
    const baseline = effective.data?.values[definition.key] ?? definition.default_value;
    setChanges((previous) => {
      const next = { ...previous };
      if (JSON.stringify(value) === JSON.stringify(baseline)) {
        delete next[definition.key];
      } else {
        next[definition.key] = value;
      }
      return next;
    });
  };

  const selectArea = (key: string) => {
    localStorage.setItem('chatos.configuration-center.config-area', key);
    setActiveArea(key);
    setConfigSearch('');
  };

  const confirmPublish = () => {
    modal.confirm({
      title: '发布配置',
      width: 560,
      content: (
        <Space direction="vertical" style={{ width: '100%', marginTop: 12 }}>
          <Typography.Text>将发布 {Object.keys(changes).length} 项变更到 {environment}。</Typography.Text>
          <Input.TextArea
            placeholder="发布说明"
            value={publishMessage}
            onChange={(event) => setPublishMessage(event.target.value)}
          />
        </Space>
      ),
      okText: '校验并发布',
      onOk: async () => {
        await publish.mutateAsync();
      },
    });
  };

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Card>
        <Space wrap>
          <Tag color="blue">Revision {effective.data?.revision || 0}</Tag>
          <Typography.Text>草稿变更 {Object.keys(changes).length} 项</Typography.Text>
          <Button icon={<SaveOutlined />} onClick={() => save.mutate()} loading={save.isPending}>
            保存草稿
          </Button>
          <Button icon={<PlusOutlined />} onClick={() => setCustomOpen(true)}>
            新增开发参数
          </Button>
          <Button
            type="primary"
            disabled={Object.keys(changes).length === 0}
            onClick={confirmPublish}
            loading={publish.isPending}
          >
            发布
          </Button>
        </Space>
      </Card>
      <Card className="config-navigation-card">
        <div className="config-navigation-header">
          <div>
            <Typography.Title level={5} style={{ margin: 0 }}>配置分类</Typography.Title>
            <Typography.Text type="secondary">
              按服务域查看配置，当前共 {definitions.length} 项
            </Typography.Text>
          </div>
          <Input.Search
            allowClear
            value={configSearch}
            onChange={(event) => setConfigSearch(event.target.value)}
            placeholder={`搜索${selectedArea?.label || ''}配置`}
            className="config-search"
          />
        </div>
        <Tabs
          activeKey={selectedArea?.key}
          onChange={selectArea}
          className="config-area-tabs"
          items={areas.map((area) => ({
            key: area.key,
            label: (
              <span className="config-tab-label">
                {area.label}
                <span className="config-tab-count">{area.items.length}</span>
              </span>
            ),
          }))}
        />
      </Card>
      {groups.map(([category, items]) => (
        <Card
          key={category}
          title={(
            <Space size={8}>
              <span>{configCategoryLabel(category)}</span>
              <Tag>{items.length} 项</Tag>
            </Space>
          )}
        >
          <Row gutter={[24, 20]}>
            {items.map((definition) => (
              <Col xs={24} xl={12} key={definition.key}>
                <div className="config-field">
                  <Space size={6} wrap>
                    <Typography.Text strong>{definition.display_name}</Typography.Text>
                    <Tag>{definition.reload_mode}</Tag>
                    {Object.prototype.hasOwnProperty.call(changes, definition.key) && <Tag color="gold">已修改</Tag>}
                  </Space>
                  <Typography.Paragraph type="secondary" className="field-description">
                    {definition.description}
                  </Typography.Paragraph>
                  <ConfigInput
                    definition={definition}
                    value={currentValue(definition)}
                    onChange={(value) => update(definition, value)}
                  />
                  <Typography.Text type="secondary" className="field-key">
                    {definition.key}
                  </Typography.Text>
                </div>
              </Col>
            ))}
          </Row>
        </Card>
      ))}
      {groups.length === 0 && (
        <Card>
          <Empty description={configSearch ? '当前分类没有匹配的配置' : '当前分类暂无配置'} />
        </Card>
      )}
      <Modal
        open={customOpen}
        title="新增开发参数"
        okText="加入草稿"
        confirmLoading={createCustom.isPending}
        onCancel={() => setCustomOpen(false)}
        onOk={() => customForm.submit()}
      >
        <Form
          form={customForm}
          layout="vertical"
          initialValues={{
            service_name: 'chatos-backend',
            value_type: 'string',
            reload_mode: 'next_request',
          }}
          onFinish={(values) => createCustom.mutate(values)}
        >
          <Form.Item name="key" label="Key" rules={[{ required: true }]}>
            <Input placeholder="developer.feature.example" />
          </Form.Item>
          <Form.Item name="display_name" label="名称" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="service_name" label="目标服务" rules={[{ required: true }]}>
            <Select
              options={[
                'chatos-backend',
                'task-runner',
                'user-service',
                'project-service',
                'plugin-management-service',
                'local-connector-service',
                'sandbox-manager',
                'memory-engine',
                'official-website',
              ].map((value) => ({ value, label: value }))}
            />
          </Form.Item>
          <Form.Item name="value_type" label="类型" rules={[{ required: true }]}>
            <Select
              options={['string', 'integer', 'boolean', 'duration_ms', 'bytes', 'json']
                .map((value) => ({ value, label: value }))}
            />
          </Form.Item>
          <Form.Item name="default_value" label="默认值" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="reload_mode" label="生效方式" rules={[{ required: true }]}>
            <Select
              options={['hot_reload', 'next_request', 'next_run', 'restart_required']
                .map((value) => ({ value, label: value }))}
            />
          </Form.Item>
          <Form.Item name="env_alias" label="环境变量映射">
            <Input placeholder="DEVELOPER_FEATURE_EXAMPLE" />
          </Form.Item>
        </Form>
      </Modal>
    </Space>
  );
}

function ConfigInput({
  definition,
  value,
  onChange,
}: {
  definition: ConfigDefinition;
  value: ConfigValue;
  onChange: (value: ConfigValue) => void;
}) {
  if (definition.value_type === 'boolean') {
    return <Switch checked={value === true} onChange={onChange} />;
  }
  if (definition.value_type === 'enum') {
    return (
      <Select
        value={typeof value === 'string' ? value : undefined}
        onChange={onChange}
        style={{ width: '100%' }}
        options={definition.enum_options.map((item) => ({ value: item, label: item }))}
      />
    );
  }
  if (['integer', 'duration_ms', 'bytes'].includes(definition.value_type)) {
    return (
      <InputNumber
        value={typeof value === 'number' ? value : null}
        min={definition.min ?? undefined}
        max={definition.max ?? undefined}
        onChange={(next) => onChange(next)}
        style={{ width: '100%' }}
        addonAfter={definition.value_type === 'duration_ms' ? 'ms' : definition.value_type === 'bytes' ? 'bytes' : undefined}
      />
    );
  }
  return (
    <Input
      value={typeof value === 'string' ? value : ''}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}

function ReleaseHistory({ environment }: { environment: string }) {
  const { message, modal } = AntdApp.useApp();
  const queryClient = useQueryClient();
  const releases = useQuery({
    queryKey: ['releases', environment],
    queryFn: () => api.releases(environment),
  });
  const rollback = useMutation({
    mutationFn: (release: ConfigRelease) => api.rollback(environment, release.id),
    onSuccess: async (release) => {
      message.success(`已回滚并生成 Revision ${release.revision}`);
      await queryClient.invalidateQueries();
    },
    onError: (error: Error) => message.error(error.message),
  });
  return (
    <Card>
      <Table
        rowKey="id"
        loading={releases.isLoading}
        dataSource={releases.data || []}
        columns={[
          { title: 'Revision', dataIndex: 'revision', render: (value) => <Tag color="blue">r{value}</Tag> },
          { title: '状态', dataIndex: 'status', render: (value) => <Tag color={value === 'published' ? 'green' : 'red'}>{value}</Tag> },
          { title: '说明', dataIndex: 'publish_message' },
          { title: '变更', dataIndex: 'changed_keys', render: (values: string[]) => values.length },
          { title: '发布时间', dataIndex: 'published_at', render: (value) => value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-' },
          {
            title: '操作',
            render: (_, release: ConfigRelease) => (
              <Button
                size="small"
                onClick={() => modal.confirm({
                  title: `回滚到 Revision ${release.revision}？`,
                  content: '回滚会创建一个新的发布版本，不会删除历史记录。',
                  onOk: () => rollback.mutateAsync(release),
                })}
              >
                回滚
              </Button>
            ),
          },
        ]}
      />
    </Card>
  );
}

function Instances() {
  const query = useQuery({ queryKey: ['instances'], queryFn: api.instances, refetchInterval: 10000 });
  return (
    <Card>
      <Table
        rowKey="id"
        loading={query.isLoading}
        dataSource={query.data || []}
        columns={[
          { title: '环境', dataIndex: 'environment' },
          { title: '服务', dataIndex: 'service_name' },
          { title: '实例', dataIndex: 'service_id' },
          { title: 'Revision', dataIndex: 'effective_revision' },
          { title: '状态', render: (_, item) => item.stale ? <Tag color="orange">stale</Tag> : <Tag color="green">active</Tag> },
          { title: '待重启 Key', dataIndex: 'pending_restart_keys', render: (values: string[]) => values.join(', ') || '-' },
          { title: '最后心跳', dataIndex: 'last_seen_at', render: (value) => dayjs(value).format('YYYY-MM-DD HH:mm:ss') },
        ]}
      />
    </Card>
  );
}

function QueueOperations({ environment }: { environment: string }) {
  const { message } = AntdApp.useApp();
  const queryClient = useQueryClient();
  const [replayTarget, setReplayTarget] = useState<QueueOperationsStream | null>(null);
  const [replayItemId, setReplayItemId] = useState('');
  const [replayTenantId, setReplayTenantId] = useState('');
  const [replaySourceId, setReplaySourceId] = useState('');
  const [replayVersion, setReplayVersion] = useState<number | null>(null);
  const [replayEventType, setReplayEventType] = useState<string>();
  const [replayReason, setReplayReason] = useState('');
  const query = useQuery({
    queryKey: ['queue-operations', environment],
    queryFn: () => api.queueOperations(environment),
    refetchInterval: 10000,
  });
  const streams = query.data?.streams || [];
  const unavailable = streams.filter((stream) => !stream.runtime.available).length;
  const deadLetters = streams.reduce(
    (total, stream) => total + queueRuntime(stream, 'dead_letter').messages,
    0,
  );
  const mainBacklog = streams.reduce(
    (total, stream) => total + queueRuntime(stream, 'main').messages,
    0,
  );
  const consumerGaps = streams.filter((stream) => {
    const main = queueRuntime(stream, 'main');
    return main.messages > 0 && main.consumers === 0;
  }).length;
  const replay = useMutation({
    mutationFn: () => api.replayQueueItem(environment, {
      service: replayTarget?.service || '',
      stream: replayTarget?.stream || '',
      item_id: replayItemId.trim(),
      tenant_id: replayTarget?.service === 'memory-engine' ? replayTenantId.trim() : undefined,
      source_id: replayTarget?.service === 'memory-engine' ? replaySourceId.trim() : undefined,
      version: ['memory-engine', 'plugin-management'].includes(replayTarget?.service || '')
        ? replayVersion || undefined
        : undefined,
      event_type: replayTarget?.stream === 'subject_memory' ? replayEventType : undefined,
      reason: replayReason.trim(),
    }),
    onSuccess: async (result) => {
      message.success(
        replayTarget?.service === 'mcp-management'
          ? 'MCP 终态死信已归档，工具不会重新执行'
          : result.dead_letter_archived ? '重放已入队，旧死信已归档' : '重放已入队，旧死信待归档',
      );
      setReplayTarget(null);
      setReplayItemId('');
      setReplayTenantId('');
      setReplaySourceId('');
      setReplayVersion(null);
      setReplayEventType(undefined);
      setReplayReason('');
      await queryClient.invalidateQueries({ queryKey: ['queue-operations', environment] });
      await queryClient.invalidateQueries({ queryKey: ['audit'] });
    },
    onError: (error: Error) => message.error(error.message),
  });

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Row gutter={[16, 16]}>
        <Col xs={24} md={6}><Card><Statistic title="生效 Revision" value={query.data?.revision || 0} /></Card></Col>
        <Col xs={24} md={6}><Card><Statistic title="主队列积压" value={mainBacklog} valueStyle={{ color: mainBacklog > 0 ? '#d97706' : '#15803d' }} /></Card></Col>
        <Col xs={24} md={6}><Card><Statistic title="DLQ 消息" value={deadLetters} valueStyle={{ color: deadLetters > 0 ? '#b42318' : '#15803d' }} /></Card></Col>
        <Col xs={24} md={6}><Card><Statistic title="异常流" value={unavailable + consumerGaps} valueStyle={{ color: unavailable + consumerGaps > 0 ? '#b42318' : '#15803d' }} /></Card></Col>
      </Row>
      <Card
        title="统一异步流状态"
        extra={<Typography.Text type="secondary">每 10 秒刷新，仅使用 {environment} 当前生效配置</Typography.Text>}
      >
        <Table
          rowKey={(stream) => `${stream.service}:${stream.stream}`}
          loading={query.isLoading || query.isFetching}
          dataSource={streams}
          pagination={false}
          scroll={{ x: 1050 }}
          columns={[
            { title: '服务', dataIndex: 'service', width: 170 },
            { title: '异步流', dataIndex: 'stream', width: 170, render: (value) => <Tag>{value}</Tag> },
            {
              title: '状态',
              width: 120,
              render: (_, stream) => queueHealthTag(stream),
            },
            {
              title: '主队列',
              width: 240,
              render: (_, stream) => queueCell(stream.main_queue, queueRuntime(stream, 'main')),
            },
            {
              title: '延迟重试',
              width: 220,
              render: (_, stream) => queueCell(stream.retry_queue, queueRuntime(stream, 'retry')),
            },
            {
              title: '死信队列',
              width: 240,
              render: (_, stream) => queueCell(stream.dead_letter_queue, queueRuntime(stream, 'dead_letter'), true),
            },
            {
              title: '操作',
              fixed: 'right',
              width: 110,
              render: (_, stream) => {
                const supported = (
                  stream.service === 'task-runner' && stream.stream === 'run_post_process'
                ) || (
                  stream.service === 'memory-engine'
                  && ['summary', 'rollup', 'subject_memory'].includes(stream.stream)
                ) || (
                  stream.service === 'plugin-management' && stream.stream === 'catalog_sync'
                ) || (
                  stream.service === 'mcp-management' && stream.stream === 'async_tool'
                );
                return (
                  <Button
                    danger
                    size="small"
                    disabled={!supported || queueRuntime(stream, 'dead_letter').messages === 0}
                    onClick={() => {
                      setReplayTarget(stream);
                      setReplayItemId('');
                      setReplayTenantId('');
                      setReplaySourceId('');
                      setReplayVersion(null);
                      setReplayEventType(undefined);
                      setReplayReason('');
                    }}
                  >
                    {stream.service === 'mcp-management' ? '人工归档' : '人工重放'}
                  </Button>
                );
              },
            },
          ]}
        />
      </Card>
      <Modal
        title={queueReplayTitle(replayTarget)}
        open={Boolean(replayTarget)}
        okText={replayTarget?.service === 'mcp-management' ? '确认归档' : '确认重放'}
        cancelText="取消"
        confirmLoading={replay.isPending}
        okButtonProps={{
          danger: true,
          disabled: replayItemId.trim().length === 0
            || replayReason.trim().length < 8
            || (replayTarget?.service === 'memory-engine' && (
              replayTenantId.trim().length === 0
              || replaySourceId.trim().length === 0
              || !replayVersion
              || (replayTarget.stream === 'subject_memory' && !replayEventType)
            ))
            || (replayTarget?.service === 'plugin-management' && !replayVersion),
        }}
        onCancel={() => setReplayTarget(null)}
        onOk={() => replay.mutate()}
      >
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Typography.Text type="secondary">
            {queueReplayDescription(replayTarget)}
          </Typography.Text>
          {replayTarget?.service === 'memory-engine' && (
            <>
              <Input
                value={replayTenantId}
                onChange={(event) => setReplayTenantId(event.target.value)}
                placeholder="Tenant ID"
              />
              <Input
                value={replaySourceId}
                onChange={(event) => setReplaySourceId(event.target.value)}
                placeholder="Source ID"
              />
              <InputNumber
                value={replayVersion}
                onChange={setReplayVersion}
                min={1}
                precision={0}
                placeholder="Dead-letter version"
                style={{ width: '100%' }}
              />
              {replayTarget.stream === 'subject_memory' && (
                <Select
                  value={replayEventType}
                  onChange={setReplayEventType}
                  placeholder="选择 Subject Memory 事件类型"
                  options={[
                    { value: 'source_available', label: '摘要来源事件 (source_available)' },
                    { value: 'scope_requested', label: '记忆范围事件 (scope_requested)' },
                  ]}
                />
              )}
            </>
          )}
          {replayTarget?.service === 'plugin-management' && (
            <InputNumber
              value={replayVersion}
              onChange={setReplayVersion}
              min={1}
              precision={0}
              placeholder="Dead-letter version"
              style={{ width: '100%' }}
            />
          )}
          <Input
            value={replayItemId}
            onChange={(event) => setReplayItemId(event.target.value)}
            placeholder={queueReplayItemPlaceholder(replayTarget)}
          />
          <Input.TextArea
            value={replayReason}
            onChange={(event) => setReplayReason(event.target.value)}
            placeholder={`${replayTarget?.service === 'mcp-management' ? '归档' : '重放'}原因，至少 8 个字符`}
            rows={4}
            maxLength={500}
            showCount
          />
        </Space>
      </Modal>
    </Space>
  );
}

function queueReplayItemPlaceholder(stream: QueueOperationsStream | null) {
  if (!stream || stream.service === 'task-runner') return 'Run ID';
  if (stream.service === 'mcp-management') return 'Invocation ID';
  if (stream.service === 'plugin-management') return 'Marketplace ID';
  if (stream.stream === 'summary') return 'Thread ID';
  if (stream.stream === 'rollup') return 'Summary ID';
  return 'Summary ID 或 Scope Key';
}

function queueReplayTitle(stream: QueueOperationsStream | null) {
  if (stream?.service === 'mcp-management') return '人工归档 MCP 终态死信';
  if (stream?.service === 'memory-engine') return '人工重放 Memory Engine 死信';
  if (stream?.service === 'plugin-management') return '人工重放 Plugin Catalog 死信';
  return '人工重放 Run 后处理死信';
}

function queueReplayDescription(stream: QueueOperationsStream | null) {
  if (stream?.service === 'mcp-management') {
    return '该失败结果已经返回原 AI 调用方。系统只归档与 Invocation 身份及耗尽重试次数完全匹配的旧 DLQ 消息，不会恢复 Outbox、重新投递或再次执行工具。';
  }
  if (stream?.service === 'memory-engine') {
    return '系统只会重放租户、来源、业务 ID 和旧版本完全匹配的死信；新 Outbox 经确认发布后，才归档对应旧消息。';
  }
  if (stream?.service === 'plugin-management') {
    return '系统只会恢复 Marketplace ID 与旧版本完全匹配的 Catalog Outbox；新版本确认发布后，才归档对应旧 DLQ 消息。';
  }
  return '系统将重置该 Run 的后处理 dead-letter 状态、重建 Outbox，并在确认发布后归档匹配的旧 DLQ 消息。';
}

function queueRuntime(stream: QueueOperationsStream, role: 'main' | 'retry' | 'dead_letter') {
  return stream.runtime.queues.find((queue) => queue.role === role) || {
    role,
    name: '',
    messages: 0,
    consumers: 0,
  };
}

function queueHealthTag(stream: QueueOperationsStream) {
  if (!stream.runtime.available) {
    return <Tag color="red">不可观测</Tag>;
  }
  const main = queueRuntime(stream, 'main');
  const deadLetter = queueRuntime(stream, 'dead_letter');
  if (deadLetter.messages > 0) {
    return <Tag color="red">存在死信</Tag>;
  }
  if (main.messages > 0 && main.consumers === 0) {
    return <Tag color="volcano">无消费者</Tag>;
  }
  if (main.messages > 0 || queueRuntime(stream, 'retry').messages > 0) {
    return <Tag color="gold">处理中</Tag>;
  }
  return <Tag color="green">正常</Tag>;
}

function queueCell(
  name: string,
  runtime: { messages: number; consumers: number },
  danger = false,
) {
  return (
    <Space direction="vertical" size={2}>
      <Typography.Text className="queue-name">{name}</Typography.Text>
      <Space size={6}>
        <Tag color={danger && runtime.messages > 0 ? 'red' : runtime.messages > 0 ? 'gold' : 'default'}>
          消息 {runtime.messages}
        </Tag>
        <Tag color={runtime.consumers > 0 ? 'blue' : 'default'}>消费者 {runtime.consumers}</Tag>
      </Space>
    </Space>
  );
}

function AuditLog() {
  const query = useQuery({ queryKey: ['audit'], queryFn: api.audit });
  return (
    <Card>
      <Table
        rowKey="id"
        loading={query.isLoading}
        dataSource={query.data || []}
        columns={[
          { title: '时间', dataIndex: 'created_at', render: (value) => dayjs(value).format('YYYY-MM-DD HH:mm:ss') },
          { title: '环境', dataIndex: 'environment', render: (value) => value || '-' },
          { title: '动作', dataIndex: 'action' },
          { title: '操作者', dataIndex: 'actor_display_name' },
          { title: '变更 Key', dataIndex: 'changed_keys', render: (values: string[]) => values.join(', ') || '-' },
        ]}
      />
    </Card>
  );
}

function pageTitle(page: PageKey): string {
  return {
    dashboard: '配置总览',
    config: '配置管理',
    releases: '发布历史',
    queues: '队列运维',
    instances: '服务实例',
    audit: '审计日志',
  }[page];
}
