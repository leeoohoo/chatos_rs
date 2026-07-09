// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { PlayCircleOutlined, ReloadOutlined } from '@ant-design/icons';
import { Alert, Button, Descriptions, Empty, Space, Switch, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';
import type { CSSProperties, ReactNode } from 'react';

import type {
  ProjectRuntimeEnvironmentImageRecord,
  ProjectRuntimeEnvironmentResponse,
  ProjectRuntimeEnvironmentStatus,
  RuntimeEnvironmentProvider,
} from '../../types';

interface RuntimeEnvironmentPanelProps {
  response?: ProjectRuntimeEnvironmentResponse;
  loading: boolean;
  errorMessage?: string;
  analyzing: boolean;
  settingsSaving: boolean;
  onAnalyze: () => void;
  onRefresh: () => void;
  onSandboxEnabledChange: (value: boolean) => void;
}

type JsonRecord = Record<string, unknown>;

interface ServiceRow {
  rowKey: string;
  serviceKey: string;
  raw: unknown;
  record?: JsonRecord;
}

interface EnvVarRow {
  key: string;
  name: string;
  value: unknown;
}

const runtimeStatusLabels: Record<ProjectRuntimeEnvironmentStatus, string> = {
  disabled: '已停用',
  pending_configuration: '等待配置',
  pending: '待初始化',
  analyzing: '分析中',
  ready: '已就绪',
  not_runnable: '不可运行',
  failed: '失败',
};

const providerLabels: Record<RuntimeEnvironmentProvider, string> = {
  none: '未选择',
  local_connector: '本地连接器',
  harness: 'Harness',
  cloud_sandbox_manager: '云端沙箱',
};

const sectionStyle: CSSProperties = {
  border: '1px solid #e5e7eb',
  borderRadius: 8,
  background: '#fff',
  overflow: 'hidden',
};

const sectionHeaderStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '12px 16px',
  borderBottom: '1px solid #eef0f3',
  background: '#fafafa',
};

const sectionBodyStyle: CSSProperties = {
  padding: 16,
};

const jsonPreviewStyle: CSSProperties = {
  maxHeight: 220,
  margin: 0,
  overflow: 'auto',
  whiteSpace: 'pre-wrap',
  wordBreak: 'break-word',
  fontSize: 12,
  lineHeight: 1.55,
};

const codeTextStyle: CSSProperties = {
  maxWidth: '100%',
  whiteSpace: 'normal',
  wordBreak: 'break-word',
};

export function RuntimeEnvironmentPanel({
  response,
  loading,
  errorMessage,
  analyzing,
  settingsSaving,
  onAnalyze,
  onRefresh,
  onSandboxEnabledChange,
}: RuntimeEnvironmentPanelProps) {
  const environment = response?.environment;
  const images = response?.images ?? [];
  const serviceRows = servicesFromJson(environment?.required_services);
  const envVarRows = envVarsFromJson(environment?.env_vars);
  const canAnalyze = Boolean(environment?.sandbox_enabled) && !analyzing;

  if (!loading && !environment) {
    return (
      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
        {errorMessage ? <Alert type="error" showIcon message={errorMessage} /> : null}
        <Empty description="暂无运行环境记录" />
      </Space>
    );
  }

  return (
    <Space direction="vertical" size="middle" style={{ width: '100%' }}>
      <div className="toolbar" style={{ justifyContent: 'space-between' }}>
        <Space size={10} wrap>
          <Typography.Title level={4} style={{ margin: 0 }}>
            运行环境
          </Typography.Title>
          {runtimeStatusTag(environment?.status)}
        </Space>
        <Space size={10} wrap>
          <Space size={8}>
            <Typography.Text type="secondary">使用沙箱</Typography.Text>
            <Switch
              checked={Boolean(environment?.sandbox_enabled)}
              loading={settingsSaving}
              disabled={!environment || settingsSaving || analyzing}
              onChange={onSandboxEnabledChange}
            />
          </Space>
          <Button icon={<ReloadOutlined />} onClick={onRefresh} loading={loading}>
            刷新
          </Button>
          <Button
            type="primary"
            icon={<PlayCircleOutlined />}
            loading={analyzing}
            disabled={!canAnalyze}
            onClick={onAnalyze}
          >
            初始化/重新分析
          </Button>
        </Space>
      </div>

      {errorMessage ? <Alert type="error" showIcon message={errorMessage} /> : null}
      {environment?.status === 'disabled' ? (
        <Alert type="info" showIcon message="当前项目未启用沙箱初始化。" />
      ) : null}
      {environment?.status === 'pending_configuration' ? (
        <Alert type="warning" showIcon message="运行环境初始化需要补充配置后再执行。" />
      ) : null}
      {environment?.status === 'not_runnable' ? (
        <Alert
          type="warning"
          showIcon
          message="项目暂不具备运行条件"
          description={environment.not_runnable_reason || 'Agent 未返回具体原因。'}
        />
      ) : null}
      {environment?.last_error ? (
        <Alert type="error" showIcon message="最近一次初始化失败" description={environment.last_error} />
      ) : null}

      <RuntimeSection title="初始化状态">
        <Descriptions bordered size="small" column={{ xs: 1, md: 2 }}>
          <Descriptions.Item label="沙箱">{environment?.sandbox_enabled ? '启用' : '停用'}</Descriptions.Item>
          <Descriptions.Item label="状态">{runtimeStatusTag(environment?.status)}</Descriptions.Item>
          <Descriptions.Item label="文件读取">
            {providerTag(environment?.file_provider)}
          </Descriptions.Item>
          <Descriptions.Item label="沙箱镜像">
            {providerTag(environment?.sandbox_provider)}
          </Descriptions.Item>
          <Descriptions.Item label="更新时间">
            {formatDateTime(environment?.updated_at)}
          </Descriptions.Item>
          <Descriptions.Item label="Agent Run">
            {environment?.last_agent_run_id ? (
              <Typography.Text copyable code style={codeTextStyle}>
                {environment.last_agent_run_id}
              </Typography.Text>
            ) : (
              '-'
            )}
          </Descriptions.Item>
          <Descriptions.Item label="分析摘要" span={2}>
            {environment?.analysis_summary || '-'}
          </Descriptions.Item>
        </Descriptions>
      </RuntimeSection>

      <RuntimeSection title="识别技术栈">
        {isEmptyJson(environment?.detected_stack) ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无技术栈识别结果" />
        ) : (
          renderJsonBlock(environment?.detected_stack)
        )}
      </RuntimeSection>

      <RuntimeSection title="依赖服务">
        <Table<ServiceRow>
          rowKey="rowKey"
          size="small"
          loading={loading}
          dataSource={serviceRows}
          locale={{ emptyText: '暂无依赖服务' }}
          pagination={{ pageSize: 8 }}
          scroll={{ x: 980 }}
          columns={serviceColumns}
        />
      </RuntimeSection>

      <RuntimeSection title="环境变量">
        <Table<EnvVarRow>
          rowKey="key"
          size="small"
          loading={loading}
          dataSource={envVarRows}
          locale={{ emptyText: '暂无环境变量' }}
          pagination={{ pageSize: 8 }}
          scroll={{ x: 720 }}
          columns={envVarColumns}
        />
      </RuntimeSection>

      <RuntimeSection title="沙箱镜像">
        <Table<ProjectRuntimeEnvironmentImageRecord>
          rowKey="id"
          size="small"
          loading={loading}
          dataSource={images}
          locale={{ emptyText: '暂无镜像记录' }}
          pagination={{ pageSize: 8 }}
          scroll={{ x: 1180 }}
          columns={imageColumns}
        />
      </RuntimeSection>
    </Space>
  );
}

function RuntimeSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section style={sectionStyle}>
      <div style={sectionHeaderStyle}>
        <Typography.Title level={5} style={{ margin: 0 }}>
          {title}
        </Typography.Title>
      </div>
      <div style={sectionBodyStyle}>{children}</div>
    </section>
  );
}

const serviceColumns: ColumnsType<ServiceRow> = [
  {
    title: '服务',
    width: 220,
    render: (_, row) => {
      const name =
        fieldText(row.record, ['display_name', 'name', 'service_name', 'service']) ||
        row.serviceKey;
      return (
        <Space direction="vertical" size={2}>
          <Typography.Text strong>{name}</Typography.Text>
          <Typography.Text type="secondary" code style={codeTextStyle}>
            {row.serviceKey}
          </Typography.Text>
        </Space>
      );
    },
  },
  {
    title: '类型',
    width: 160,
    render: (_, row) => {
      const type = fieldText(row.record, ['type', 'service_type', 'environment_type', 'kind']);
      return type ? <Tag>{type}</Tag> : '-';
    },
  },
  {
    title: '说明',
    render: (_, row) =>
      fieldText(row.record, ['reason', 'description', 'summary', 'notes']) || '-',
  },
  {
    title: '配置',
    width: 360,
    render: (_, row) => renderJsonBlock(row.record ?? row.raw),
  },
];

const envVarColumns: ColumnsType<EnvVarRow> = [
  {
    title: '变量名',
    dataIndex: 'name',
    width: 260,
    render: (value: string) => (
      <Typography.Text code copyable style={codeTextStyle}>
        {value}
      </Typography.Text>
    ),
  },
  {
    title: '变量值',
    render: (_, row) => renderScalar(row.value, isSecretName(row.name)),
  },
];

const imageColumns: ColumnsType<ProjectRuntimeEnvironmentImageRecord> = [
  {
    title: '环境',
    width: 240,
    render: (_, row) => (
      <Space direction="vertical" size={2}>
        <Typography.Text strong>{row.display_name || row.environment_key}</Typography.Text>
        <Space size={4} wrap>
          <Tag>{row.environment_type || 'service'}</Tag>
          <Typography.Text type="secondary" code style={codeTextStyle}>
            {row.environment_key}
          </Typography.Text>
        </Space>
      </Space>
    ),
  },
  {
    title: '镜像',
    width: 260,
    render: (_, row) => (
      <Space direction="vertical" size={2}>
        {row.image_id ? (
          <Typography.Text code copyable style={codeTextStyle}>
            {row.image_id}
          </Typography.Text>
        ) : (
          <Typography.Text type="secondary">-</Typography.Text>
        )}
        {row.image_ref ? (
          <Typography.Text type="secondary" copyable style={codeTextStyle}>
            {row.image_ref}
          </Typography.Text>
        ) : null}
      </Space>
    ),
  },
  {
    title: '提供方',
    width: 140,
    render: (_, row) => providerTag(row.image_provider),
  },
  {
    title: '状态',
    width: 110,
    render: (_, row) => imageStatusTag(row.status),
  },
  {
    title: '端口',
    width: 160,
    render: (_, row) => renderCompactJson(row.ports),
  },
  {
    title: '环境变量',
    width: 260,
    render: (_, row) => renderEnvVarSummary(row.env_vars),
  },
  {
    title: '错误',
    width: 240,
    render: (_, row) => row.error || '-',
  },
];

function runtimeStatusTag(status?: ProjectRuntimeEnvironmentStatus) {
  if (!status) {
    return <Tag>未知</Tag>;
  }
  const color =
    status === 'ready'
      ? 'success'
      : status === 'failed'
        ? 'error'
        : status === 'not_runnable'
          ? 'warning'
          : status === 'disabled'
            ? 'default'
            : 'processing';
  return <Tag color={color}>{runtimeStatusLabels[status] || status}</Tag>;
}

function providerTag(provider?: RuntimeEnvironmentProvider) {
  const value = provider || 'none';
  const color =
    value === 'local_connector'
      ? 'cyan'
      : value === 'harness'
        ? 'geekblue'
        : value === 'cloud_sandbox_manager'
          ? 'blue'
          : 'default';
  return <Tag color={color}>{providerLabels[value]}</Tag>;
}

function imageStatusTag(status: string) {
  const normalized = status.trim().toLowerCase();
  const color =
    normalized === 'ready' || normalized === 'available'
      ? 'success'
      : normalized === 'failed' || normalized === 'error'
        ? 'error'
        : normalized === 'creating' || normalized === 'pending'
          ? 'processing'
          : 'default';
  return <Tag color={color}>{status || '-'}</Tag>;
}

function servicesFromJson(value: unknown): ServiceRow[] {
  if (Array.isArray(value)) {
    return value.map((item, index) => {
      const record = asRecord(item);
      const serviceKey =
        fieldText(record, ['key', 'environment_key', 'service_key', 'name', 'type']) ||
        `service_${index + 1}`;
      return {
        rowKey: `${serviceKey}-${index}`,
        serviceKey,
        raw: item,
        record,
      };
    });
  }
  const record = asRecord(value);
  if (!record) {
    return [];
  }
  return Object.entries(record).map(([key, item], index) => ({
    rowKey: `${key}-${index}`,
    serviceKey: key,
    raw: item,
    record: asRecord(item),
  }));
}

function envVarsFromJson(value: unknown): EnvVarRow[] {
  if (Array.isArray(value)) {
    return value.map((item, index) => {
      const record = asRecord(item);
      const name = fieldText(record, ['name', 'key', 'env', 'variable']) || `ENV_${index + 1}`;
      const envValue = record && 'value' in record ? record.value : item;
      return {
        key: `${name}-${index}`,
        name,
        value: envValue,
      };
    });
  }
  const record = asRecord(value);
  if (!record) {
    return [];
  }
  return Object.entries(record).map(([name, item]) => ({
    key: name,
    name,
    value: item,
  }));
}

function asRecord(value: unknown): JsonRecord | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  return value as JsonRecord;
}

function fieldText(record: JsonRecord | undefined, fields: string[]) {
  if (!record) {
    return undefined;
  }
  for (const field of fields) {
    const value = record[field];
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
    if (typeof value === 'number' || typeof value === 'boolean') {
      return String(value);
    }
  }
  return undefined;
}

function renderCompactJson(value: unknown) {
  if (isEmptyJson(value)) {
    return <Typography.Text type="secondary">-</Typography.Text>;
  }
  if (Array.isArray(value) && value.every((item) => isScalar(item))) {
    return (
      <Space size={[4, 4]} wrap>
        {value.map((item, index) => (
          <Tag key={`${String(item)}-${index}`}>{String(item)}</Tag>
        ))}
      </Space>
    );
  }
  return renderJsonBlock(value);
}

function renderEnvVarSummary(value: unknown) {
  const rows = envVarsFromJson(value);
  if (rows.length === 0) {
    return <Typography.Text type="secondary">-</Typography.Text>;
  }
  return (
    <Space direction="vertical" size={2}>
      {rows.slice(0, 4).map((row) => (
        <Typography.Text key={row.key} code style={codeTextStyle}>
          {row.name}
        </Typography.Text>
      ))}
      {rows.length > 4 ? <Typography.Text type="secondary">+{rows.length - 4}</Typography.Text> : null}
    </Space>
  );
}

function renderJsonBlock(value: unknown) {
  if (isScalar(value)) {
    return renderScalar(value, false);
  }
  const text = stringifyJson(value);
  return (
    <Typography.Paragraph copyable={{ text }} style={{ marginBottom: 0 }}>
      <pre style={jsonPreviewStyle}>{text}</pre>
    </Typography.Paragraph>
  );
}

function renderScalar(value: unknown, secret: boolean) {
  if (value === null || value === undefined || value === '') {
    return <Typography.Text type="secondary">-</Typography.Text>;
  }
  const text = String(value);
  return (
    <Typography.Text code copyable={{ text }} style={codeTextStyle}>
      {secret ? '********' : text}
    </Typography.Text>
  );
}

function isScalar(value: unknown) {
  return (
    value === null ||
    value === undefined ||
    typeof value === 'string' ||
    typeof value === 'number' ||
    typeof value === 'boolean'
  );
}

function isEmptyJson(value: unknown) {
  if (value === null || value === undefined) {
    return true;
  }
  if (Array.isArray(value)) {
    return value.length === 0;
  }
  if (typeof value === 'object') {
    return Object.keys(value).length === 0;
  }
  if (typeof value === 'string') {
    return value.trim().length === 0;
  }
  return false;
}

function stringifyJson(value: unknown) {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function isSecretName(name: string) {
  return /(password|passwd|secret|token|credential|private|access_key|apikey|api_key)/i.test(name);
}

function formatDateTime(value?: string | null) {
  return value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-';
}
