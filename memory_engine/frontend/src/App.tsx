import {
  AppstoreOutlined,
  BarsOutlined,
  DeleteOutlined,
  EditOutlined,
  HistoryOutlined,
  PlusOutlined,
  ReloadOutlined,
  SaveOutlined,
  SettingOutlined,
} from '@ant-design/icons';
import {
  App,
  Button,
  Card,
  Col,
  ConfigProvider,
  Empty,
  Form,
  Input,
  InputNumber,
  Layout,
  Menu,
  Modal,
  Popconfirm,
  Row,
  Select,
  Space,
  Spin,
  Statistic,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { TableColumnsType } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { useEffect, useMemo, useState } from 'react';

import { api } from './api';
import type {
  EngineJobPolicy,
  EngineJobRun,
  EngineModelProfile,
  JobRunQuery,
  UpsertEngineJobPolicyPayload,
  UpsertEngineModelProfilePayload,
} from './types';

const { Header, Sider, Content } = Layout;
const { Title, Text, Paragraph } = Typography;
const { TextArea } = Input;

type TabKey = 'dashboard' | 'models' | 'policies' | 'runs';
type ModelFormValues = {
  name: string;
  provider: string;
  model: string;
  base_url?: string;
  api_key?: string;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  temperature?: number | null;
  thinking_level?: string;
  enabled: boolean;
};
type PolicyFormValues = {
  enabled: boolean;
  model_profile_id?: string;
  summary_prompt?: string;
  token_limit?: number | null;
  round_limit?: number | null;
  target_summary_tokens?: number | null;
  interval_seconds?: number | null;
  max_threads_per_tick?: number | null;
  keep_level0_count?: number | null;
  max_level?: number | null;
  max_records_per_thread?: number | null;
};

const RUN_STATUS_OPTIONS = ['running', 'done', 'failed', 'queued'];
const JOB_TYPE_OPTIONS = ['summary', 'rollup', 'subject_memory', 'thread_repair'];

function toLocal(value?: string | null): string {
  if (!value) {
    return '-';
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

function statusColor(status: string): string {
  if (status === 'done') return 'success';
  if (status === 'failed') return 'error';
  if (status === 'running') return 'processing';
  return 'default';
}

function textOrUndefined(value?: string | null): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function numberOrNull(value?: number | null): number | null {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return null;
  }
  return value;
}

function buildModelPayload(values: ModelFormValues): UpsertEngineModelProfilePayload {
  return {
    name: values.name.trim(),
    provider: values.provider.trim(),
    model: values.model.trim(),
    base_url: textOrUndefined(values.base_url) ?? null,
    api_key: textOrUndefined(values.api_key),
    supports_images: values.supports_images,
    supports_reasoning: values.supports_reasoning,
    supports_responses: values.supports_responses,
    temperature: numberOrNull(values.temperature),
    thinking_level: textOrUndefined(values.thinking_level) ?? null,
    enabled: values.enabled,
  };
}

function buildPolicyPayload(values: PolicyFormValues): UpsertEngineJobPolicyPayload {
  return {
    enabled: values.enabled,
    model_profile_id: textOrUndefined(values.model_profile_id) ?? null,
    summary_prompt: textOrUndefined(values.summary_prompt) ?? null,
    token_limit: numberOrNull(values.token_limit),
    round_limit: numberOrNull(values.round_limit),
    target_summary_tokens: numberOrNull(values.target_summary_tokens),
    interval_seconds: numberOrNull(values.interval_seconds),
    max_threads_per_tick: numberOrNull(values.max_threads_per_tick),
    keep_level0_count: numberOrNull(values.keep_level0_count),
    max_level: numberOrNull(values.max_level),
    max_records_per_thread: numberOrNull(values.max_records_per_thread),
  };
}

function modelFormInitialValues(model?: EngineModelProfile | null): ModelFormValues {
  return {
    name: model?.name ?? '',
    provider: model?.provider ?? '',
    model: model?.model ?? '',
    base_url: model?.base_url ?? '',
    api_key: '',
    supports_images: model?.supports_images ?? false,
    supports_reasoning: model?.supports_reasoning ?? false,
    supports_responses: model?.supports_responses ?? false,
    temperature: model?.temperature ?? null,
    thinking_level: model?.thinking_level ?? '',
    enabled: model?.enabled ?? true,
  };
}

function policyFormInitialValues(policy: EngineJobPolicy): PolicyFormValues {
  return {
    enabled: policy.enabled,
    model_profile_id: policy.model_profile_id ?? undefined,
    summary_prompt: policy.summary_prompt ?? '',
    token_limit: policy.token_limit ?? null,
    round_limit: policy.round_limit ?? null,
    target_summary_tokens: policy.target_summary_tokens ?? null,
    interval_seconds: policy.interval_seconds ?? null,
    max_threads_per_tick: policy.max_threads_per_tick ?? null,
    keep_level0_count: policy.keep_level0_count ?? null,
    max_level: policy.max_level ?? null,
    max_records_per_thread: policy.max_records_per_thread ?? null,
  };
}

function PolicyEditorCard(props: {
  policy: EngineJobPolicy;
  modelOptions: Array<{ label: string; value: string }>;
  saving: boolean;
  onSave: (jobType: string, values: PolicyFormValues) => Promise<void>;
}) {
  const { policy, modelOptions, saving, onSave } = props;
  const [form] = Form.useForm<PolicyFormValues>();

  useEffect(() => {
    form.setFieldsValue(policyFormInitialValues(policy));
  }, [form, policy]);

  return (
    <Card
      title={
        <Space>
          <Tag color="geekblue">{policy.job_type}</Tag>
          <Text type="secondary">更新时间 {toLocal(policy.updated_at)}</Text>
        </Space>
      }
      extra={
        <Button
          type="primary"
          icon={<SaveOutlined />}
          loading={saving}
          onClick={() =>
            void form.validateFields().then((values) => onSave(policy.job_type, values))
          }
        >
          保存
        </Button>
      }
    >
      <Form
        form={form}
        layout="vertical"
        initialValues={policyFormInitialValues(policy)}
      >
        <Row gutter={[12, 0]}>
          <Col xs={24} md={8}>
            <Form.Item label="启用" name="enabled" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Col>
          <Col xs={24} md={16}>
            <Form.Item label="模型配置" name="model_profile_id">
              <Select
                allowClear
                showSearch
                optionFilterProp="label"
                placeholder="留空表示使用全局默认模型"
                options={modelOptions}
              />
            </Form.Item>
          </Col>
          <Col span={24}>
            <Form.Item label="总结 Prompt" name="summary_prompt">
              <TextArea rows={4} placeholder="为空时使用默认总结模板" />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="Token 上限" name="token_limit">
              <InputNumber min={128} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="轮次上限" name="round_limit">
              <InputNumber min={1} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="目标总结 Token" name="target_summary_tokens">
              <InputNumber min={128} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="调度间隔(秒)" name="interval_seconds">
              <InputNumber min={3} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="每轮线程数" name="max_threads_per_tick">
              <InputNumber min={1} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="保留 L0 数量" name="keep_level0_count">
              <InputNumber min={0} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="最大层级" name="max_level">
              <InputNumber min={1} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="每线程最大记录" name="max_records_per_thread">
              <InputNumber min={1} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
        </Row>
      </Form>
    </Card>
  );
}

export default function AppShell() {
  return (
    <ConfigProvider
      locale={zhCN}
      theme={{
        token: {
          colorPrimary: '#136f63',
          borderRadius: 8,
          fontFamily: '"IBM Plex Sans","Noto Sans SC","Source Han Sans SC",sans-serif',
        },
      }}
    >
      <App>
        <AppContent />
      </App>
    </ConfigProvider>
  );
}

function AppContent() {
  const { message } = App.useApp();
  const [tab, setTab] = useState<TabKey>('dashboard');
  const [loading, setLoading] = useState(true);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [policiesLoading, setPoliciesLoading] = useState(false);
  const [runsLoading, setRunsLoading] = useState(false);
  const [modelSubmitting, setModelSubmitting] = useState(false);
  const [modelModalOpen, setModelModalOpen] = useState(false);
  const [editingModel, setEditingModel] = useState<EngineModelProfile | null>(null);
  const [savingPolicyJobType, setSavingPolicyJobType] = useState<string | null>(null);
  const [runFilters, setRunFilters] = useState<JobRunQuery>({
    job_type: undefined,
    status: undefined,
    tenant_id: '',
    source_id: '',
    limit: 200,
  });
  const [modelProfiles, setModelProfiles] = useState<EngineModelProfile[]>([]);
  const [jobPolicies, setJobPolicies] = useState<EngineJobPolicy[]>([]);
  const [jobRuns, setJobRuns] = useState<EngineJobRun[]>([]);
  const [jobStats, setJobStats] = useState<Record<string, Record<string, number>>>({});

  const [modelForm] = Form.useForm<ModelFormValues>();
  const [runFilterForm] = Form.useForm<JobRunQuery>();

  const loadModels = async () => {
    setModelsLoading(true);
    try {
      const models = await api.listModelProfiles();
      setModelProfiles(models);
    } finally {
      setModelsLoading(false);
    }
  };

  const loadPolicies = async () => {
    setPoliciesLoading(true);
    try {
      const policies = await api.listJobPolicies();
      setJobPolicies(policies);
    } finally {
      setPoliciesLoading(false);
    }
  };

  const loadRuns = async (filters?: JobRunQuery) => {
    setRunsLoading(true);
    try {
      const [runs, stats] = await Promise.all([
        api.listJobRuns(filters ?? runFilters),
        api.getJobRunStats(),
      ]);
      setJobRuns(runs);
      setJobStats(stats);
    } finally {
      setRunsLoading(false);
    }
  };

  const loadAll = async () => {
    setLoading(true);
    try {
      const [models, policies, runs, stats] = await Promise.all([
        api.listModelProfiles(),
        api.listJobPolicies(),
        api.listJobRuns(runFilters),
        api.getJobRunStats(),
      ]);
      setModelProfiles(models);
      setJobPolicies(policies);
      setJobRuns(runs);
      setJobStats(stats);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const dashboardStats = useMemo(() => {
    const running = jobRuns.filter((item) => item.status === 'running').length;
    const done = jobRuns.filter((item) => item.status === 'done').length;
    const failed = jobRuns.filter((item) => item.status === 'failed').length;
    return {
      models: modelProfiles.length,
      policies: jobPolicies.length,
      running,
      done,
      failed,
    };
  }, [jobPolicies.length, jobRuns, modelProfiles.length]);

  const menuItems = [
    { key: 'dashboard', icon: <AppstoreOutlined />, label: '概览' },
    { key: 'models', icon: <BarsOutlined />, label: '模型配置' },
    { key: 'policies', icon: <SettingOutlined />, label: '任务策略' },
    { key: 'runs', icon: <HistoryOutlined />, label: '任务运行' },
  ];

  const modelOptions = modelProfiles.map((profile) => ({
    label: `${profile.name} (${profile.model})`,
    value: profile.id,
  }));

  const openCreateModelModal = () => {
    setEditingModel(null);
    modelForm.setFieldsValue(modelFormInitialValues(null));
    setModelModalOpen(true);
  };

  const openEditModelModal = (model: EngineModelProfile) => {
    setEditingModel(model);
    modelForm.setFieldsValue(modelFormInitialValues(model));
    setModelModalOpen(true);
  };

  const closeModelModal = () => {
    setModelModalOpen(false);
    setEditingModel(null);
    modelForm.resetFields();
  };

  const handleSubmitModel = async () => {
    try {
      const values = await modelForm.validateFields();
      const payload = buildModelPayload(values);
      setModelSubmitting(true);
      if (editingModel) {
        await api.updateModelProfile(editingModel.id, payload);
        message.success(`已更新模型配置：${payload.name}`);
      } else {
        await api.createModelProfile(payload);
        message.success(`已创建模型配置：${payload.name}`);
      }
      closeModelModal();
      await Promise.all([loadModels(), loadPolicies()]);
    } finally {
      setModelSubmitting(false);
    }
  };

  const handleDeleteModel = async (model: EngineModelProfile) => {
    await api.deleteModelProfile(model.id);
    message.success(`已删除模型配置：${model.name}`);
    await Promise.all([loadModels(), loadPolicies()]);
  };

  const handleSavePolicy = async (jobType: string, values: PolicyFormValues) => {
    setSavingPolicyJobType(jobType);
    try {
      await api.updateJobPolicy(jobType, buildPolicyPayload(values));
      message.success(`已保存任务策略：${jobType}`);
      await loadPolicies();
    } finally {
      setSavingPolicyJobType(null);
    }
  };

  const handleApplyRunFilters = async () => {
    const values = await runFilterForm.validateFields();
    const nextFilters: JobRunQuery = {
      job_type: textOrUndefined(values.job_type),
      status: textOrUndefined(values.status),
      tenant_id: textOrUndefined(values.tenant_id),
      source_id: textOrUndefined(values.source_id),
      limit: values.limit ?? 200,
    };
    setRunFilters(nextFilters);
    await loadRuns(nextFilters);
  };

  const handleResetRunFilters = async () => {
    const resetValues: JobRunQuery = {
      job_type: undefined,
      status: undefined,
      tenant_id: '',
      source_id: '',
      limit: 200,
    };
    runFilterForm.setFieldsValue(resetValues);
    setRunFilters(resetValues);
    await loadRuns(resetValues);
  };

  const modelColumns: TableColumnsType<EngineModelProfile> = [
    { title: '名称', dataIndex: 'name', key: 'name', width: 180 },
    { title: 'Provider', dataIndex: 'provider', key: 'provider', width: 140 },
    { title: 'Model', dataIndex: 'model', key: 'model', width: 220 },
    {
      title: '能力',
      key: 'capabilities',
      width: 220,
      render: (_, record) => (
        <Space size={[4, 4]} wrap>
          {record.supports_images ? <Tag color="purple">images</Tag> : null}
          {record.supports_reasoning ? <Tag color="gold">reasoning</Tag> : null}
          {record.supports_responses ? <Tag color="cyan">responses</Tag> : null}
          {!record.supports_images &&
          !record.supports_reasoning &&
          !record.supports_responses ? (
            <Text type="secondary">-</Text>
          ) : null}
        </Space>
      ),
    },
    {
      title: '启用',
      dataIndex: 'enabled',
      key: 'enabled',
      width: 90,
      render: (value: boolean) => (
        <Tag color={value ? 'success' : 'default'}>{value ? '是' : '否'}</Tag>
      ),
    },
    {
      title: '温度',
      dataIndex: 'temperature',
      key: 'temperature',
      width: 90,
      render: (value?: number | null) => value ?? '-',
    },
    {
      title: '更新时间',
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 180,
      render: toLocal,
    },
    {
      title: '操作',
      key: 'actions',
      fixed: 'right',
      width: 140,
      render: (_, record) => (
        <Space>
          <Button icon={<EditOutlined />} size="small" onClick={() => openEditModelModal(record)}>
            编辑
          </Button>
          <Popconfirm
            title="删除模型配置"
            description={`确认删除 ${record.name} 吗？`}
            okText="删除"
            cancelText="取消"
            onConfirm={() => void handleDeleteModel(record)}
          >
            <Button danger icon={<DeleteOutlined />} size="small">
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const runColumns: TableColumnsType<EngineJobRun> = [
    { title: 'ID', dataIndex: 'id', key: 'id', width: 90, render: (value: string) => value.slice(0, 8) },
    { title: '任务类型', dataIndex: 'job_type', key: 'job_type', width: 130, render: (value: string) => <Tag>{value}</Tag> },
    { title: '触发来源', dataIndex: 'trigger_type', key: 'trigger_type', width: 160 },
    { title: 'Tenant', dataIndex: 'tenant_id', key: 'tenant_id', width: 140, render: (value?: string | null) => value || '-' },
    { title: 'Source', dataIndex: 'source_id', key: 'source_id', width: 140, render: (value?: string | null) => value || '-' },
    { title: 'Thread', dataIndex: 'thread_id', key: 'thread_id', width: 120, render: (value?: string | null) => (value ? value.slice(0, 8) : '-') },
    { title: 'Subject', dataIndex: 'subject_id', key: 'subject_id', width: 160, render: (value?: string | null) => value || '-' },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 100,
      render: (value: string) => <Tag color={statusColor(value)}>{value}</Tag>,
    },
    { title: '输入', dataIndex: 'input_count', key: 'input_count', width: 80 },
    { title: '输出', dataIndex: 'output_count', key: 'output_count', width: 80 },
    { title: '处理', dataIndex: 'processed_count', key: 'processed_count', width: 80 },
    { title: '成功', dataIndex: 'success_count', key: 'success_count', width: 80 },
    { title: '失败数', dataIndex: 'error_count', key: 'error_count', width: 80 },
    { title: '开始时间', dataIndex: 'started_at', key: 'started_at', width: 180, render: toLocal },
    { title: '结束时间', dataIndex: 'finished_at', key: 'finished_at', width: 180, render: toLocal },
    {
      title: '元数据',
      dataIndex: 'metadata',
      key: 'metadata',
      width: 320,
      render: (value?: Record<string, unknown> | null) =>
        value ? <pre className="engine-pre">{JSON.stringify(value, null, 2)}</pre> : '-',
    },
  ];

  if (loading) {
    return (
      <div className="engine-loading">
        <Spin size="large" />
      </div>
    );
  }

  return (
    <>
      <Layout style={{ minHeight: '100vh' }}>
        <Sider theme="light" width={240}>
          <div className="engine-brand">
            <Title level={4} style={{ margin: 0 }}>
              Memory Engine
            </Title>
            <Text type="secondary">统一记忆平台控制台</Text>
          </div>
          <Menu
            mode="inline"
            selectedKeys={[tab]}
            items={menuItems}
            onSelect={(event) => setTab(event.key as TabKey)}
          />
        </Sider>
        <Layout>
          <Header className="engine-topbar">
            <Space wrap>
              <Tag color="blue">memory_engine</Tag>
              <Text type="secondary">全局配置、任务策略、运行记录</Text>
              <Button icon={<ReloadOutlined />} onClick={() => void loadAll()}>
                刷新
              </Button>
            </Space>
          </Header>
          <Content className="engine-page">
            {tab === 'dashboard' ? (
              <Space direction="vertical" size={16} style={{ width: '100%' }}>
                <Row gutter={[12, 12]}>
                  <Col xs={12} lg={4}>
                    <Statistic title="模型配置" value={dashboardStats.models} />
                  </Col>
                  <Col xs={12} lg={4}>
                    <Statistic title="任务策略" value={dashboardStats.policies} />
                  </Col>
                  <Col xs={12} lg={4}>
                    <Statistic title="运行中" value={dashboardStats.running} />
                  </Col>
                  <Col xs={12} lg={4}>
                    <Statistic title="已完成" value={dashboardStats.done} />
                  </Col>
                  <Col xs={12} lg={4}>
                    <Statistic title="失败" value={dashboardStats.failed} />
                  </Col>
                </Row>
                <Row gutter={[12, 12]}>
                  {Object.keys(jobStats).length === 0 ? (
                    <Col span={24}>
                      <Card>
                        <Empty description="最近没有任务运行记录" />
                      </Card>
                    </Col>
                  ) : (
                    Object.entries(jobStats).map(([jobType, stats]) => (
                      <Col key={jobType} xs={24} lg={12}>
                        <Card size="small" title={<Tag>{jobType}</Tag>}>
                          <Row gutter={[12, 12]}>
                            {Object.entries(stats).map(([status, count]) => (
                              <Col key={status} xs={12}>
                                <Statistic title={status} value={count} />
                              </Col>
                            ))}
                          </Row>
                        </Card>
                      </Col>
                    ))
                  )}
                </Row>
              </Space>
            ) : tab === 'models' ? (
              <Card
                title="模型配置"
                extra={
                  <Space>
                    <Button icon={<ReloadOutlined />} loading={modelsLoading} onClick={() => void loadModels()}>
                      刷新
                    </Button>
                    <Button type="primary" icon={<PlusOutlined />} onClick={openCreateModelModal}>
                      新建模型
                    </Button>
                  </Space>
                }
              >
                <Table
                  rowKey="id"
                  dataSource={modelProfiles}
                  loading={modelsLoading}
                  pagination={{ pageSize: 10 }}
                  scroll={{ x: 1280 }}
                  columns={modelColumns}
                />
              </Card>
            ) : tab === 'policies' ? (
              <Space direction="vertical" size={16} style={{ width: '100%' }}>
                <Card
                  title="任务策略"
                  extra={
                    <Button icon={<ReloadOutlined />} loading={policiesLoading} onClick={() => void loadPolicies()}>
                      刷新
                    </Button>
                  }
                >
                  <Text type="secondary">
                    这里管理全局任务策略，不再按用户拆分。后续子系统接入时，直接复用这一套策略即可。
                  </Text>
                </Card>
                {jobPolicies.map((policy) => {
                  return (
                    <PolicyEditorCard
                      key={policy.job_type}
                      policy={policy}
                      modelOptions={modelOptions}
                      saving={savingPolicyJobType === policy.job_type}
                      onSave={handleSavePolicy}
                    />
                  );
                })}
              </Space>
            ) : (
              <Space direction="vertical" size={16} style={{ width: '100%' }}>
                <Card title="任务运行筛选">
                  <Form form={runFilterForm} layout="vertical" initialValues={runFilters}>
                    <Row gutter={[12, 0]}>
                      <Col xs={24} md={6}>
                        <Form.Item label="任务类型" name="job_type">
                          <Select
                            allowClear
                            placeholder="全部"
                            options={JOB_TYPE_OPTIONS.map((value) => ({ label: value, value }))}
                          />
                        </Form.Item>
                      </Col>
                      <Col xs={24} md={6}>
                        <Form.Item label="状态" name="status">
                          <Select
                            allowClear
                            placeholder="全部"
                            options={RUN_STATUS_OPTIONS.map((value) => ({ label: value, value }))}
                          />
                        </Form.Item>
                      </Col>
                      <Col xs={24} md={6}>
                        <Form.Item label="Tenant" name="tenant_id">
                          <Input placeholder="按租户筛选" />
                        </Form.Item>
                      </Col>
                      <Col xs={24} md={6}>
                        <Form.Item label="Source" name="source_id">
                          <Input placeholder="按来源系统筛选" />
                        </Form.Item>
                      </Col>
                      <Col xs={24} md={6}>
                        <Form.Item label="返回条数" name="limit">
                          <InputNumber min={1} max={1000} style={{ width: '100%' }} />
                        </Form.Item>
                      </Col>
                      <Col span={24}>
                        <Space>
                          <Button type="primary" loading={runsLoading} onClick={() => void handleApplyRunFilters()}>
                            应用筛选
                          </Button>
                          <Button onClick={() => void handleResetRunFilters()}>重置</Button>
                          <Button icon={<ReloadOutlined />} loading={runsLoading} onClick={() => void loadRuns()}>
                            刷新
                          </Button>
                        </Space>
                      </Col>
                    </Row>
                  </Form>
                </Card>
                <Card title="任务运行">
                  <Table
                    rowKey="id"
                    dataSource={jobRuns}
                    loading={runsLoading}
                    pagination={{ pageSize: 12 }}
                    scroll={{ x: 1900 }}
                    columns={runColumns}
                  />
                </Card>
              </Space>
            )}
          </Content>
        </Layout>
      </Layout>

      <Modal
        open={modelModalOpen}
        title={editingModel ? '编辑模型配置' : '新建模型配置'}
        onCancel={closeModelModal}
        onOk={() => void handleSubmitModel()}
        confirmLoading={modelSubmitting}
        okText={editingModel ? '保存' : '创建'}
        cancelText="取消"
        width={760}
        destroyOnClose
      >
        <Paragraph type="secondary">
          模型配置由 memory_engine 统一管理。编辑时如果不改 API Key，留空即可保留原值。
        </Paragraph>
        <Form form={modelForm} layout="vertical" initialValues={modelFormInitialValues(editingModel)}>
          <Row gutter={[12, 0]}>
            <Col xs={24} md={12}>
              <Form.Item
                label="名称"
                name="name"
                rules={[{ required: true, message: '请输入配置名称' }]}
              >
                <Input placeholder="例如：summary-default" />
              </Form.Item>
            </Col>
            <Col xs={24} md={12}>
              <Form.Item
                label="Provider"
                name="provider"
                rules={[{ required: true, message: '请输入 provider' }]}
              >
                <Input placeholder="例如：openai" />
              </Form.Item>
            </Col>
            <Col xs={24} md={12}>
              <Form.Item
                label="Model"
                name="model"
                rules={[{ required: true, message: '请输入 model' }]}
              >
                <Input placeholder="例如：gpt-4.1" />
              </Form.Item>
            </Col>
            <Col xs={24} md={12}>
              <Form.Item label="Base URL" name="base_url">
                <Input placeholder="可选" />
              </Form.Item>
            </Col>
            <Col xs={24} md={12}>
              <Form.Item label="API Key" name="api_key">
                <Input.Password placeholder={editingModel ? '留空表示保持不变' : '可选'} />
              </Form.Item>
            </Col>
            <Col xs={24} md={12}>
              <Form.Item label="Thinking Level" name="thinking_level">
                <Input placeholder="例如：medium" />
              </Form.Item>
            </Col>
            <Col xs={24} md={12}>
              <Form.Item label="Temperature" name="temperature">
                <InputNumber min={0} max={2} step={0.1} style={{ width: '100%' }} />
              </Form.Item>
            </Col>
            <Col xs={24} md={12}>
              <Form.Item label="启用" name="enabled" valuePropName="checked">
                <Switch />
              </Form.Item>
            </Col>
            <Col xs={24} md={8}>
              <Form.Item label="支持图片" name="supports_images" valuePropName="checked">
                <Switch />
              </Form.Item>
            </Col>
            <Col xs={24} md={8}>
              <Form.Item label="支持推理" name="supports_reasoning" valuePropName="checked">
                <Switch />
              </Form.Item>
            </Col>
            <Col xs={24} md={8}>
              <Form.Item label="支持 Responses" name="supports_responses" valuePropName="checked">
                <Switch />
              </Form.Item>
            </Col>
          </Row>
        </Form>
      </Modal>
    </>
  );
}
