import React, { useEffect, useState } from 'react';
import {
  Layout,
  Tabs,
  Typography,
  Space,
  Button,
  Card,
  Collapse,
  Descriptions,
  Form,
  Input,
  Table,
  Modal,
  Select,
  Switch,
  message,
  InputNumber,
  Divider,
  Upload,
  Drawer,
  Tag
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { UploadRequestOption } from 'rc-upload/lib/interface';
import { PlusOutlined, ReloadOutlined, SettingOutlined, UploadOutlined } from '@ant-design/icons';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { apiGet, apiPost } from './api';
import type {
  JobEvent,
  JobRecord,
  MarketplaceRecord,
  MarketplaceSummary,
  McpServerConfig,
  ModelConfig,
  StatusResponse
} from './types';

const { Header, Content } = Layout;
const { Title, Text } = Typography;

const DEFAULT_JOB_POLL_MS = 5000;
const DEFAULT_JOB_DETAIL_POLL_MS = 3000;
const AUTO_REFRESH_KEY = 'subagent_auto_refresh';
const JOB_POLL_MS_KEY = 'subagent_job_poll_ms';
const JOB_DETAIL_POLL_MS_KEY = 'subagent_job_detail_poll_ms';

function readStoredNumber(key: string, fallback: number, min: number) {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) return fallback;
    return Math.max(min, Math.round(parsed));
  } catch {
    return fallback;
  }
}

function generateId(prefix: string) {
  return `${prefix}_${Math.random().toString(36).slice(2, 8)}`;
}

function parseArgsInput(raw: string): string[] {
  const trimmed = raw.trim();
  if (!trimmed) return [];
  if (trimmed.startsWith('[')) {
    try {
      const parsed = JSON.parse(trimmed);
      if (Array.isArray(parsed)) {
        return parsed.map((entry) => String(entry)).filter(Boolean);
      }
    } catch {
      // fall through
    }
  }
  return trimmed.split(/\s+/).filter(Boolean);
}

function formatJson(value?: string) {
  if (!value) return '';
  try {
    const parsed = JSON.parse(value);
    return JSON.stringify(parsed, null, 2);
  } catch {
    return value;
  }
}

function formatDate(value?: string) {
  if (!value) return '';
  return value.replace('T', ' ').replace('Z', '');
}

function markdownFromText(value?: string) {
  const trimmed = value?.trim() ?? '';
  return trimmed ? trimmed : '_No data_';
}

function markdownFromJson(value?: string) {
  if (!value) return '_No data_';
  const pretty = formatJson(value);
  const body = pretty?.trim() ? pretty : value;
  return `\`\`\`json\n${body}\n\`\`\``;
}

function markdownFromUnknown(value: unknown) {
  if (value === null || value === undefined) return '_No data_';
  if (typeof value === 'string') return markdownFromText(value);
  return markdownFromJson(JSON.stringify(value, null, 2));
}

function markdownFromResult(value?: string) {
  if (!value) return '_No data_';
  try {
    const parsed = JSON.parse(value);
    if (typeof parsed === 'string') {
      return markdownFromText(parsed);
    }
    const candidates = ['response', 'stdout', 'result', 'output', 'message'];
    for (const key of candidates) {
      const candidate = parsed?.[key];
      if (typeof candidate === 'string' && candidate.trim()) {
        return markdownFromText(candidate);
      }
    }
    return markdownFromJson(value);
  } catch {
    return markdownFromText(value);
  }
}

function parseJsonSafe(value?: string) {
  if (!value) return null;
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

function eventHeader(event: JobEvent) {
  const payload = parseJsonSafe(event.payloadJson) as Record<string, any> | null;
  const labels: Record<string, string> = {
    start: '开始',
    finish: '完成',
    finish_error: '失败',
    ai_request: 'AI 请求',
    ai_response: 'AI 响应',
    ai_error: 'AI 错误',
    ai_retry: 'AI 重试',
    tool_call: '工具调用',
    tool_result: '工具结果'
  };
  const label = labels[event.type] || event.type;
  const step = payload?.step ? ` · step ${payload.step}` : '';
  const model = payload?.model ? ` · ${payload.model}` : '';
  const tool = payload?.tool ? ` · ${payload.tool}` : '';
  return `${label}${step}${model}${tool}`;
}

function eventPayloadMarkdown(event: JobEvent) {
  const payload = parseJsonSafe(event.payloadJson) as Record<string, any> | null;
  if (!payload && !event.payloadJson) return '_No payload_';
  if (event.type === 'tool_call') {
    return markdownFromUnknown(payload?.arguments ?? payload);
  }
  if (event.type === 'tool_result') {
    const result = payload?.result ?? event.payloadJson ?? '';
    return markdownFromUnknown(result);
  }
  if (event.type === 'ai_response' && payload?.message) {
    return markdownFromUnknown(payload.message);
  }
  const payloadText = payload ? JSON.stringify(payload, null, 2) : (event.payloadJson || '');
  if (!payloadText.trim()) return '_No payload_';
  const truncated = payloadText.length > 6000 ? `${payloadText.slice(0, 6000)} ... [truncated]` : payloadText;
  return markdownFromJson(truncated);
}

function eventDotStyle(type: string) {
  const normalized = type.toLowerCase();
  if (normalized.includes('error')) {
    return { color: '#ef4444', ring: '#fee2e2' };
  }
  if (normalized.includes('finish') || normalized.includes('done')) {
    return { color: '#10b981', ring: '#dcfce7' };
  }
  if (normalized.includes('start')) {
    return { color: '#3b82f6', ring: '#dbeafe' };
  }
  return { color: '#94a3b8', ring: '#e2e8f0' };
}

export default function App() {
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [statusLoading, setStatusLoading] = useState(false);

  const [mcpServers, setMcpServers] = useState<McpServerConfig[]>([]);
  const [mcpLoading, setMcpLoading] = useState(false);

  const [marketplaceSummary, setMarketplaceSummary] = useState<MarketplaceSummary | null>(null);
  const [summaryLoading, setSummaryLoading] = useState(false);

  const [jobs, setJobs] = useState<JobRecord[]>([]);
  const [jobsLoading, setJobsLoading] = useState(false);
  const [jobStatusFilter, setJobStatusFilter] = useState<string>('');
  const [allSessions, setAllSessions] = useState(false);
  const [autoRefresh, setAutoRefresh] = useState(() => {
    try {
      const saved = localStorage.getItem(AUTO_REFRESH_KEY);
      return saved ? saved === 'true' : true;
    } catch {
      return true;
    }
  });
  const [jobPollMs, setJobPollMs] = useState(() =>
    readStoredNumber(JOB_POLL_MS_KEY, DEFAULT_JOB_POLL_MS, 1000)
  );
  const [jobDetailPollMs, setJobDetailPollMs] = useState(() =>
    readStoredNumber(JOB_DETAIL_POLL_MS_KEY, DEFAULT_JOB_DETAIL_POLL_MS, 1000)
  );
  const [selectedJob, setSelectedJob] = useState<JobRecord | null>(null);
  const [jobEvents, setJobEvents] = useState<JobEvent[]>([]);
  const [jobEventsLoading, setJobEventsLoading] = useState(false);

  const [modelConfigs, setModelConfigs] = useState<ModelConfig[]>([]);
  const [activeModelId, setActiveModelId] = useState<string>('');

  const [runtimeForm] = Form.useForm();
  const [modelForm] = Form.useForm<ModelConfig>();
  const [mcpForm] = Form.useForm();

  const [modelModalOpen, setModelModalOpen] = useState(false);
  const [modelEditingId, setModelEditingId] = useState<string | null>(null);

  const [mcpModalOpen, setMcpModalOpen] = useState(false);
  const [mcpEditingId, setMcpEditingId] = useState<string | null>(null);

  const [settingsOpen, setSettingsOpen] = useState(false);

  const refreshStatus = async () => {
    setStatusLoading(true);
    try {
      const data = await apiGet<StatusResponse>('/api/status');
      setStatus(data);
    } catch (err) {
      message.error(String(err));
    } finally {
      setStatusLoading(false);
    }
  };

  const refreshMcpServers = async () => {
    setMcpLoading(true);
    try {
      const data = await apiGet<{ servers: McpServerConfig[] }>('/api/mcp_servers');
      setMcpServers(Array.isArray(data.servers) ? data.servers : []);
    } catch (err) {
      message.error(String(err));
    } finally {
      setMcpLoading(false);
    }
  };

  const refreshMarketplaceSummary = async () => {
    setSummaryLoading(true);
    try {
      const data = await apiGet<MarketplaceSummary>('/api/marketplace/summary');
      setMarketplaceSummary(data);
    } catch (err) {
      message.error(String(err));
    } finally {
      setSummaryLoading(false);
    }
  };

  const refreshJobs = async () => {
    setJobsLoading(true);
    try {
      const params = new URLSearchParams();
      if (jobStatusFilter) params.set('status', jobStatusFilter);
      if (allSessions) params.set('all_sessions', 'true');
      const url = params.toString() ? `/api/jobs?${params.toString()}` : '/api/jobs';
      const data = await apiGet<{ jobs: JobRecord[] }>(url);
      setJobs(Array.isArray(data.jobs) ? data.jobs : []);
    } catch (err) {
      message.error(String(err));
    } finally {
      setJobsLoading(false);
    }
  };

  const refreshJobEvents = async (jobId: string) => {
    setJobEventsLoading(true);
    try {
      const data = await apiGet<{ events: JobEvent[] }>(`/api/job_events?job_id=${encodeURIComponent(jobId)}`);
      setJobEvents(Array.isArray(data.events) ? data.events : []);
    } catch (err) {
      message.error(String(err));
    } finally {
      setJobEventsLoading(false);
    }
  };

  useEffect(() => {
    refreshStatus();
    refreshMcpServers();
    refreshMarketplaceSummary();
    refreshJobs();
  }, []);

  useEffect(() => {
    if (!autoRefresh) return undefined;
    const intervalId = window.setInterval(() => {
      refreshJobs();
    }, jobPollMs);
    return () => window.clearInterval(intervalId);
  }, [jobStatusFilter, allSessions, autoRefresh, jobPollMs]);

  useEffect(() => {
    if (!selectedJob) return undefined;
    if (!autoRefresh) return undefined;
    const active = selectedJob.status === 'queued' || selectedJob.status === 'running';
    if (!active) return undefined;
    const intervalId = window.setInterval(() => {
      refreshJobEvents(selectedJob.id);
    }, jobDetailPollMs);
    return () => window.clearInterval(intervalId);
  }, [selectedJob?.id, selectedJob?.status, autoRefresh, jobDetailPollMs]);

  useEffect(() => {
    if (!autoRefresh) return;
    refreshJobs();
    if (selectedJob && (selectedJob.status === 'queued' || selectedJob.status === 'running')) {
      refreshJobEvents(selectedJob.id);
    }
  }, [autoRefresh, selectedJob?.id, selectedJob?.status]);

  useEffect(() => {
    try {
      localStorage.setItem(AUTO_REFRESH_KEY, String(autoRefresh));
    } catch {
      // ignore storage errors
    }
  }, [autoRefresh]);

  useEffect(() => {
    try {
      localStorage.setItem(JOB_POLL_MS_KEY, String(jobPollMs));
    } catch {
      // ignore storage errors
    }
  }, [jobPollMs]);

  useEffect(() => {
    try {
      localStorage.setItem(JOB_DETAIL_POLL_MS_KEY, String(jobDetailPollMs));
    } catch {
      // ignore storage errors
    }
  }, [jobDetailPollMs]);

  useEffect(() => {
    if (!selectedJob) return;
    const latest = jobs.find((job) => job.id === selectedJob.id);
    if (!latest) return;
    const changed =
      latest.status !== selectedJob.status ||
      latest.updatedAt !== selectedJob.updatedAt ||
      latest.resultJson !== selectedJob.resultJson ||
      latest.error !== selectedJob.error;
    if (changed) {
      setSelectedJob(latest);
    }
  }, [jobs, selectedJob]);

  useEffect(() => {
    if (!status) return;
    runtimeForm.setFieldsValue({
      ai_timeout_ms: status.runtime_config?.aiTimeoutMs ?? null,
      ai_max_output_bytes: status.runtime_config?.aiMaxOutputBytes ?? null,
      ai_tool_max_turns: status.runtime_config?.aiToolMaxTurns ?? null,
      ai_max_retries: status.runtime_config?.aiMaxRetries ?? null,
      command_timeout_ms: status.runtime_config?.commandTimeoutMs ?? null,
      command_max_output_bytes: status.runtime_config?.commandMaxOutputBytes ?? null
    });

    const rawModels = Array.isArray(status.model_configs) && status.model_configs.length > 0
      ? status.model_configs
      : status.model_config
      ? [{ ...status.model_config, id: 'default', name: 'Default' }]
      : [];
    const normalized = rawModels.map((entry) => ({
      id: entry.id || generateId('model'),
      name: entry.name || entry.id || 'Model',
      api_key: entry.api_key || '',
      base_url: entry.base_url || '',
      model: entry.model || '',
      reasoning_enabled: entry.reasoning_enabled !== false,
      responses_enabled: entry.responses_enabled === true
    }));
    setModelConfigs(normalized);
    const active = status.active_model_id || (normalized[0] && normalized[0].id) || '';
    setActiveModelId(active);
  }, [status, runtimeForm]);

  const handleSaveRuntime = async () => {
    try {
      const values = await runtimeForm.validateFields();
      await apiPost('/api/runtime_settings', {
        ai_timeout_ms: values.ai_timeout_ms ?? null,
        ai_max_output_bytes: values.ai_max_output_bytes ?? null,
        ai_tool_max_turns: values.ai_tool_max_turns ?? null,
        ai_max_retries: values.ai_max_retries ?? null,
        command_timeout_ms: values.command_timeout_ms ?? null,
        command_max_output_bytes: values.command_max_output_bytes ?? null
      });
      message.success('Runtime settings saved');
      await refreshStatus();
    } catch (err) {
      message.error(String(err));
    }
  };

  const persistModels = async (nextModels: ModelConfig[], nextActiveId: string) => {
    await apiPost('/api/model_settings', {
      models: nextModels,
      active_model_id: nextActiveId
    });
    message.success('Model settings saved');
    await refreshStatus();
  };

  const openModelModal = (model?: ModelConfig) => {
    if (model) {
      setModelEditingId(model.id || null);
      modelForm.setFieldsValue({
        name: model.name,
        api_key: model.api_key,
        base_url: model.base_url,
        model: model.model,
        reasoning_enabled: model.reasoning_enabled,
        responses_enabled: model.responses_enabled
      });
    } else {
      setModelEditingId(null);
      modelForm.resetFields();
      modelForm.setFieldsValue({
        reasoning_enabled: true,
        responses_enabled: false
      });
    }
    setModelModalOpen(true);
  };

  const handleSaveModel = async () => {
    try {
      const values = await modelForm.validateFields();
      const id = modelEditingId || generateId('model');
      const existing = modelConfigs.find((entry) => entry.id === id);
      const entry: ModelConfig = {
        id,
        name: values.name || existing?.name || id,
        api_key: values.api_key || '',
        base_url: values.base_url || '',
        model: values.model || '',
        reasoning_enabled: values.reasoning_enabled !== false,
        responses_enabled: values.responses_enabled === true
      };
      let nextModels = modelConfigs.filter((item) => item.id !== id);
      nextModels = [...nextModels, entry];
      const activeId = activeModelId || entry.id || '';
      setModelModalOpen(false);
      await persistModels(nextModels, activeId);
    } catch (err) {
      message.error(String(err));
    }
  };

  const handleDeleteModel = async (id: string) => {
    try {
      const next = modelConfigs.filter((entry) => entry.id !== id);
      const nextActive = activeModelId === id ? (next[0]?.id || '') : activeModelId;
      await persistModels(next, nextActive);
    } catch (err) {
      message.error(String(err));
    }
  };

  const handleActivateModel = async (id: string) => {
    try {
      await persistModels(modelConfigs, id);
    } catch (err) {
      message.error(String(err));
    }
  };

  const openMcpModal = (server?: McpServerConfig) => {
    if (server) {
      setMcpEditingId(server.id);
      mcpForm.setFieldsValue({
        name: server.name,
        transport: server.transport,
        command: server.command,
        args: (server.args || []).join(' '),
        endpoint_url: server.endpointUrl,
        headers_json: server.headersJson,
        enabled: server.enabled
      });
    } else {
      setMcpEditingId(null);
      mcpForm.resetFields();
      mcpForm.setFieldsValue({
        transport: 'stdio',
        enabled: true
      });
    }
    setMcpModalOpen(true);
  };

  const handleSaveMcpServer = async () => {
    try {
      const values = await mcpForm.validateFields();
      const payload = {
        id: mcpEditingId || undefined,
        name: values.name,
        transport: values.transport || 'stdio',
        command: values.command || '',
        args: parseArgsInput(values.args || ''),
        endpoint_url: values.endpoint_url || '',
        headers_json: values.headers_json || '',
        enabled: values.enabled !== false
      };
      await apiPost('/api/mcp_servers/save', payload);
      message.success('MCP server saved');
      setMcpModalOpen(false);
      await refreshMcpServers();
    } catch (err) {
      message.error(String(err));
    }
  };

  const handleDeleteMcpServer = async (id: string) => {
    try {
      await apiPost('/api/mcp_servers/delete', { id });
      message.success('MCP server deleted');
      await refreshMcpServers();
    } catch (err) {
      message.error(String(err));
    }
  };

  const handleMarketplaceUpload = async (options: UploadRequestOption) => {
    const file = options.file as File;
    try {
      const text = await file.text();
      await apiPost('/api/marketplace', {
        name: file.name,
        json: text,
        activate: true
      });
      message.success('Marketplace uploaded');
      await refreshStatus();
      await refreshMarketplaceSummary();
      options.onSuccess?.({}, undefined as any);
    } catch (err) {
      message.error(String(err));
      options.onError?.(err as Error);
    }
  };

  const handleToggleMarketplace = async (id: string, active: boolean) => {
    try {
      await apiPost('/api/marketplace/activate', { id, active });
      await refreshStatus();
      await refreshMarketplaceSummary();
    } catch (err) {
      message.error(String(err));
    }
  };

  const handleDeleteMarketplace = async (id: string) => {
    try {
      await apiPost('/api/marketplace/delete', { id });
      await refreshStatus();
      await refreshMarketplaceSummary();
    } catch (err) {
      message.error(String(err));
    }
  };

  const handleInstallPlugin = async (source: string) => {
    try {
      await apiPost('/api/plugins/install', { source });
      message.success('Plugin installed');
      await refreshMarketplaceSummary();
    } catch (err) {
      message.error(String(err));
    }
  };

  const handleInstallMissing = async () => {
    try {
      await apiPost('/api/plugins/install_missing', {});
      message.success('Missing plugins installed');
      await refreshMarketplaceSummary();
    } catch (err) {
      message.error(String(err));
    }
  };

  const marketplaceList = status?.marketplaces || [];

  const modelColumns: ColumnsType<ModelConfig> = [
    { title: 'Name', dataIndex: 'name', key: 'name' },
    { title: 'Model', dataIndex: 'model', key: 'model' },
    { title: 'Base URL', dataIndex: 'base_url', key: 'base_url' },
    {
      title: 'Reasoning',
      dataIndex: 'reasoning_enabled',
      key: 'reasoning_enabled',
      render: (value) => (value ? 'On' : 'Off')
    },
    {
      title: 'Responses',
      dataIndex: 'responses_enabled',
      key: 'responses_enabled',
      render: (value) => (value ? 'On' : 'Off')
    },
    {
      title: 'Active',
      key: 'active',
      render: (_, record) =>
        record.id === activeModelId ? (
          <Tag color="blue">Active</Tag>
        ) : (
          <Button size="small" onClick={() => handleActivateModel(record.id || '')}>
            Set Active
          </Button>
        )
    },
    {
      title: 'Actions',
      key: 'actions',
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => openModelModal(record)}>
            Edit
          </Button>
          <Button size="small" danger onClick={() => handleDeleteModel(record.id || '')}>
            Delete
          </Button>
        </Space>
      )
    }
  ];

  const mcpColumns: ColumnsType<McpServerConfig> = [
    { title: 'Name', dataIndex: 'name', key: 'name' },
    { title: 'Transport', dataIndex: 'transport', key: 'transport' },
    { title: 'Command', dataIndex: 'command', key: 'command' },
    { title: 'Endpoint', dataIndex: 'endpointUrl', key: 'endpointUrl' },
    {
      title: 'Enabled',
      dataIndex: 'enabled',
      key: 'enabled',
      render: (value) => (value ? 'Yes' : 'No')
    },
    {
      title: 'Actions',
      key: 'actions',
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => openMcpModal(record)}>
            Edit
          </Button>
          <Button size="small" danger onClick={() => handleDeleteMcpServer(record.id)}>
            Delete
          </Button>
        </Space>
      )
    }
  ];

  const jobColumns: ColumnsType<JobRecord> = [
    { title: 'ID', dataIndex: 'id', key: 'id', width: 180 },
    {
      title: 'Status',
      dataIndex: 'status',
      key: 'status',
      render: (value) => {
        const color = value === 'done' ? 'green' : value === 'error' ? 'red' : value === 'running' ? 'blue' : 'default';
        return <Tag color={color}>{value}</Tag>;
      }
    },
    { title: 'Task', dataIndex: 'task', key: 'task' },
    { title: 'Agent', dataIndex: 'agentId', key: 'agentId' },
    {
      title: 'Created',
      dataIndex: 'createdAt',
      key: 'createdAt',
      render: (value) => formatDate(value)
    }
  ];

  const pluginColumns: ColumnsType<MarketplaceSummary['plugins'][number]> = [
    { title: 'Name', dataIndex: 'name', key: 'name' },
    { title: 'Source', dataIndex: 'source', key: 'source' },
    { title: 'Category', dataIndex: 'category', key: 'category' },
    {
      title: 'Agents',
      key: 'agents',
      render: (_, record) => `${record.counts.agents.available}/${record.counts.agents.total}`
    },
    {
      title: 'Skills',
      key: 'skills',
      render: (_, record) => `${record.counts.skills.available}/${record.counts.skills.total}`
    },
    {
      title: 'Commands',
      key: 'commands',
      render: (_, record) => `${record.counts.commands.available}/${record.counts.commands.total}`
    },
    {
      title: 'Installed',
      dataIndex: 'exists',
      key: 'exists',
      render: (value) => (value ? 'Yes' : 'No')
    },
    {
      title: 'Actions',
      key: 'actions',
      render: (_, record) => (
        <Space>
          <Button size="small" disabled={record.exists} onClick={() => handleInstallPlugin(record.source)}>
            Install
          </Button>
        </Space>
      )
    }
  ];

  const marketplaceColumns: ColumnsType<MarketplaceRecord> = [
    { title: 'Name', dataIndex: 'name', key: 'name' },
    { title: 'Plugins', dataIndex: 'pluginCount', key: 'pluginCount' },
    { title: 'Created', dataIndex: 'createdAt', key: 'createdAt', render: (value) => formatDate(value) },
    {
      title: 'Active',
      dataIndex: 'active',
      key: 'active',
      render: (value, record) => (
        <Switch checked={value} onChange={(checked) => handleToggleMarketplace(record.id, checked)} />
      )
    },
    {
      title: 'Actions',
      key: 'actions',
      render: (_, record) => (
        <Button size="small" danger onClick={() => handleDeleteMarketplace(record.id)}>
          Delete
        </Button>
      )
    }
  ];

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Header style={{ background: '#0f172a', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <Title level={4} style={{ color: 'white', margin: 0 }}>Sub-agent Router Admin</Title>
        <Space>
          <Button icon={<ReloadOutlined />} onClick={refreshStatus} loading={statusLoading}>Refresh Status</Button>
          <Button icon={<SettingOutlined />} onClick={() => setSettingsOpen(true)}>Settings</Button>
        </Space>
      </Header>
      <Content style={{ padding: 24 }}>
        <Tabs
          items={[
            {
              key: 'jobs',
              label: 'Jobs',
              children: (
                <Space direction="vertical" size="large" style={{ width: '100%' }}>
                  <Space>
                    <Select
                      value={jobStatusFilter}
                      onChange={(value) => setJobStatusFilter(value)}
                      style={{ width: 180 }}
                      options={[
                        { value: '', label: 'All Statuses' },
                        { value: 'queued', label: 'Queued' },
                        { value: 'running', label: 'Running' },
                        { value: 'done', label: 'Done' },
                        { value: 'error', label: 'Error' },
                        { value: 'cancelled', label: 'Cancelled' }
                      ]}
                    />
                    <Space>
                      <Text>All Sessions</Text>
                      <Switch checked={allSessions} onChange={(checked) => setAllSessions(checked)} />
                    </Space>
                    <Button onClick={refreshJobs} loading={jobsLoading}>Refresh Jobs</Button>
                  </Space>
                  <Table
                    rowKey="id"
                    dataSource={jobs}
                    columns={jobColumns}
                    loading={jobsLoading}
                    onRow={(record) => ({
                      onClick: () => {
                        setSelectedJob(record);
                        refreshJobEvents(record.id);
                      }
                    })}
                  />
                  <Drawer
                    title="Job Details"
                    width={980}
                    open={!!selectedJob}
                    onClose={() => {
                      setSelectedJob(null);
                      setJobEvents([]);
                    }}
                  >
                    {selectedJob ? (
                      <Space direction="vertical" size="large" style={{ width: '100%' }}>
                        <Card size="small" title="Summary">
                          <Descriptions
                            size="small"
                            bordered
                            column={2}
                            items={[
                              {
                                key: 'status',
                                label: 'Status',
                                children: (
                                  <Tag color={selectedJob.status === 'done' ? 'green' : selectedJob.status === 'error' ? 'red' : selectedJob.status === 'running' ? 'blue' : 'default'}>
                                    {selectedJob.status}
                                  </Tag>
                                )
                              },
                              { key: 'id', label: 'Job ID', children: selectedJob.id },
                              { key: 'agent', label: 'Agent', children: selectedJob.agentId || '-' },
                              { key: 'command', label: 'Command', children: selectedJob.commandId || '-' },
                              { key: 'session', label: 'Session', children: selectedJob.sessionId || '-' },
                              { key: 'run', label: 'Run', children: selectedJob.runId || '-' },
                              { key: 'created', label: 'Created', children: formatDate(selectedJob.createdAt) },
                              { key: 'updated', label: 'Updated', children: formatDate(selectedJob.updatedAt) }
                            ]}
                          />
                        </Card>

                        <Card size="small" title="Content">
                          <Collapse
                            defaultActiveKey={['task', 'result']}
                            items={[
                              {
                                key: 'task',
                                label: 'Task',
                                children: (
                                  <div style={{ background: '#0f172a', color: '#e2e8f0', padding: 12, borderRadius: 6 }}>
                                    <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                      {markdownFromText(selectedJob.task)}
                                    </ReactMarkdown>
                                  </div>
                                )
                              },
                              {
                                key: 'result',
                                label: 'Result',
                                children: (
                                  <div style={{ background: '#0f172a', color: '#e2e8f0', padding: 12, borderRadius: 6 }}>
                                    <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                      {markdownFromResult(selectedJob.resultJson)}
                                    </ReactMarkdown>
                                  </div>
                                )
                              },
                              {
                                key: 'payload',
                                label: 'Payload (raw)',
                                children: (
                                  <div style={{ background: '#0f172a', color: '#e2e8f0', padding: 12, borderRadius: 6 }}>
                                    <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                      {markdownFromJson(selectedJob.payloadJson)}
                                    </ReactMarkdown>
                                  </div>
                                )
                              },
                              selectedJob.error
                                ? {
                                    key: 'error',
                                    label: 'Error',
                                    children: (
                                      <div style={{ background: '#2a0f0f', color: '#fca5a5', padding: 12, borderRadius: 6 }}>
                                        <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                          {markdownFromText(selectedJob.error)}
                                        </ReactMarkdown>
                                      </div>
                                    )
                                  }
                                : null
                            ].filter(Boolean) as any}
                          />
                        </Card>

                        <Card size="small" title="Timeline">
                          {jobEvents.length === 0 ? (
                            <Text type="secondary">No events yet.</Text>
                          ) : (
                            <div style={{ position: 'relative', paddingLeft: 22 }}>
                              <div
                                style={{
                                  position: 'absolute',
                                  left: 6,
                                  top: 4,
                                  bottom: 4,
                                  width: 2,
                                  background: '#d6defa'
                                }}
                              />
                              <Space direction="vertical" size="small" style={{ width: '100%' }}>
                                {jobEvents.map((event) => {
                                  const dot = eventDotStyle(event.type);
                                  return (
                                    <details key={event.id} style={{ position: 'relative', paddingLeft: 12 }}>
                                      <summary
                                        style={{
                                          listStyle: 'none',
                                          cursor: 'pointer',
                                          display: 'flex',
                                          alignItems: 'center',
                                          gap: 8
                                        }}
                                      >
                                        <span
                                          style={{
                                            position: 'absolute',
                                            left: -20,
                                            top: 6,
                                            width: 12,
                                            height: 12,
                                            borderRadius: '50%',
                                            background: dot.color,
                                            boxShadow: `0 0 0 3px ${dot.ring}`
                                          }}
                                        />
                                        <Text strong>{eventHeader(event)}</Text>
                                        <Text type="secondary" style={{ fontSize: 12 }}>
                                          {formatDate(event.createdAt)}
                                        </Text>
                                      </summary>
                                      <div style={{ marginTop: 8, paddingLeft: 8 }}>
                                        <div style={{ background: '#0f172a', color: '#e2e8f0', padding: 12, borderRadius: 6 }}>
                                          <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                            {eventPayloadMarkdown(event)}
                                          </ReactMarkdown>
                                        </div>
                                      </div>
                                    </details>
                                  );
                                })}
                              </Space>
                            </div>
                          )}
                        </Card>
                      </Space>
                    ) : null}
                  </Drawer>
                </Space>
              )
            },
          ]}
        />
      </Content>
      <Drawer
        title="Settings"
        width={960}
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      >
        <Tabs
          defaultActiveKey="models"
          items={[
            {
              key: 'global',
              label: 'Global',
              children: (
                <Space direction="vertical" size="large" style={{ width: '100%' }}>
                  <Card size="small" title="UI Settings">
                    <Space direction="vertical" size="middle">
                      <Space>
                        <Text>Auto Refresh Jobs</Text>
                        <Switch checked={autoRefresh} onChange={(checked) => setAutoRefresh(checked)} />
                      </Space>
                      <Space>
                        <Text>Jobs Refresh Interval</Text>
                        <InputNumber
                          min={1}
                          step={1}
                          value={Math.max(1, Math.round(jobPollMs / 1000))}
                          onChange={(value) => {
                            const seconds = value && value > 0 ? value : DEFAULT_JOB_POLL_MS / 1000;
                            setJobPollMs(Math.max(1000, Math.round(seconds * 1000)));
                          }}
                        />
                        <Text type="secondary">seconds</Text>
                      </Space>
                      <Space>
                        <Text>Job Detail Refresh Interval</Text>
                        <InputNumber
                          min={1}
                          step={1}
                          value={Math.max(1, Math.round(jobDetailPollMs / 1000))}
                          onChange={(value) => {
                            const seconds = value && value > 0 ? value : DEFAULT_JOB_DETAIL_POLL_MS / 1000;
                            setJobDetailPollMs(Math.max(1000, Math.round(seconds * 1000)));
                          }}
                        />
                        <Text type="secondary">seconds</Text>
                      </Space>
                    </Space>
                  </Card>
                </Space>
              )
            },
            {
              key: 'models',
              label: 'Models',
              children: (
                <Space direction="vertical" size="large" style={{ width: '100%' }}>
                  <Space>
                    <Button type="primary" icon={<PlusOutlined />} onClick={() => openModelModal()}>
                      Add Model
                    </Button>
                  </Space>
                  <Table rowKey={(row) => row.id || ''} dataSource={modelConfigs} columns={modelColumns} pagination={false} />
                  <Modal
                    title={modelEditingId ? 'Edit Model' : 'Add Model'}
                    open={modelModalOpen}
                    onCancel={() => setModelModalOpen(false)}
                    onOk={handleSaveModel}
                    okText="Save"
                  >
                    <Form form={modelForm} layout="vertical">
                      <Form.Item label="Name" name="name" rules={[{ required: true, message: 'Name required' }]}>
                        <Input />
                      </Form.Item>
                      <Form.Item label="API Key" name="api_key">
                        <Input.Password />
                      </Form.Item>
                      <Form.Item label="Base URL" name="base_url">
                        <Input />
                      </Form.Item>
                      <Form.Item label="Model" name="model">
                        <Input />
                      </Form.Item>
                      <Form.Item label="Reasoning" name="reasoning_enabled" valuePropName="checked">
                        <Switch />
                      </Form.Item>
                      <Form.Item label="Responses" name="responses_enabled" valuePropName="checked">
                        <Switch />
                      </Form.Item>
                    </Form>
                  </Modal>
                </Space>
              )
            },
            {
              key: 'runtime',
              label: 'Runtime',
              children: (
                <Space direction="vertical" size="large" style={{ width: '100%' }}>
                  <Title level={5}>Runtime Settings</Title>
                  <Form form={runtimeForm} layout="vertical" style={{ maxWidth: 600 }}>
                    <Form.Item label="AI Timeout (ms)" name="ai_timeout_ms">
                      <InputNumber style={{ width: '100%' }} />
                    </Form.Item>
                    <Form.Item label="AI Max Output Bytes" name="ai_max_output_bytes">
                      <InputNumber style={{ width: '100%' }} />
                    </Form.Item>
                    <Form.Item label="AI Tool Max Turns" name="ai_tool_max_turns">
                      <InputNumber style={{ width: '100%' }} />
                    </Form.Item>
                    <Form.Item label="AI Max Retries" name="ai_max_retries">
                      <InputNumber style={{ width: '100%' }} />
                    </Form.Item>
                    <Form.Item label="Command Timeout (ms)" name="command_timeout_ms">
                      <InputNumber style={{ width: '100%' }} />
                    </Form.Item>
                    <Form.Item label="Command Max Output Bytes" name="command_max_output_bytes">
                      <InputNumber style={{ width: '100%' }} />
                    </Form.Item>
                  </Form>
                  <Button type="primary" onClick={handleSaveRuntime}>Save Runtime</Button>
                </Space>
              )
            },
            {
              key: 'mcp',
              label: 'MCP Servers',
              children: (
                <Space direction="vertical" size="large" style={{ width: '100%' }}>
                  <Space>
                    <Button type="primary" icon={<PlusOutlined />} onClick={() => openMcpModal()}>
                      Add MCP Server
                    </Button>
                    <Button onClick={refreshMcpServers} loading={mcpLoading}>Refresh</Button>
                  </Space>
                  <Table rowKey="id" dataSource={mcpServers} columns={mcpColumns} loading={mcpLoading} />
                  <Modal
                    title={mcpEditingId ? 'Edit MCP Server' : 'Add MCP Server'}
                    open={mcpModalOpen}
                    onCancel={() => setMcpModalOpen(false)}
                    onOk={handleSaveMcpServer}
                    okText="Save"
                  >
                    <Form form={mcpForm} layout="vertical">
                      <Form.Item label="Name" name="name" rules={[{ required: true, message: 'Name required' }]}>
                        <Input />
                      </Form.Item>
                      <Form.Item label="Transport" name="transport" rules={[{ required: true }]}> 
                        <Select options={[{ value: 'stdio', label: 'stdio' }, { value: 'http', label: 'http' }]} />
                      </Form.Item>
                      <Form.Item label="Command" name="command">
                        <Input placeholder="/path/to/binary" />
                      </Form.Item>
                      <Form.Item label="Args" name="args">
                        <Input placeholder="--flag value" />
                      </Form.Item>
                      <Form.Item label="Endpoint URL" name="endpoint_url">
                        <Input placeholder="https://host/mcp" />
                      </Form.Item>
                      <Form.Item label="Headers JSON" name="headers_json">
                        <Input.TextArea rows={3} placeholder='{"Authorization":"Bearer ..."}' />
                      </Form.Item>
                      <Form.Item label="Enabled" name="enabled" valuePropName="checked">
                        <Switch />
                      </Form.Item>
                    </Form>
                  </Modal>
                </Space>
              )
            },
            {
              key: 'marketplace',
              label: 'Marketplace',
              children: (
                <Space direction="vertical" size="large" style={{ width: '100%' }}>
                  <Space>
                    <Upload customRequest={handleMarketplaceUpload} showUploadList={false}>
                      <Button icon={<UploadOutlined />}>Upload marketplace.json</Button>
                    </Upload>
                    <Button onClick={refreshMarketplaceSummary} loading={summaryLoading}>Refresh Summary</Button>
                    <Button onClick={handleInstallMissing}>Install Missing</Button>
                  </Space>
                  <Title level={5}>Marketplaces</Title>
                  <Table rowKey="id" dataSource={marketplaceList} columns={marketplaceColumns} pagination={false} />
                  <Divider />
                  <Title level={5}>Plugins</Title>
                  <Table
                    rowKey="source"
                    dataSource={marketplaceSummary?.plugins || []}
                    columns={pluginColumns}
                    loading={summaryLoading}
                  />
                </Space>
              )
            }
          ]}
        />
      </Drawer>
    </Layout>
  );
}
