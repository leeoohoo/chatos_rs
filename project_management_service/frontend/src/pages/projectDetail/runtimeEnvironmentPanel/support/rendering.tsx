// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Space, Tag, Tooltip, Typography } from 'antd';

import type {
  ProjectRuntimeEnvironmentResponse,
  ProjectRuntimeEnvironmentStatus,
  ProjectRuntimeEnvironmentVariableRecord,
  RuntimeEnvironmentProvider,
  RuntimeEnvironmentVariableSource,
} from '../../../../types';

import { codeTextStyle, jsonPreviewStyle } from './layout';
import type { EnvVarDraft, JsonRecord, LegacyEnvVarRow, ServiceRow } from './types';

export const runtimeStatusLabels: Record<ProjectRuntimeEnvironmentStatus, string> = {
  disabled: '已停用',
  pending_configuration: '等待配置',
  pending_image_build: '等待启动环境',
  pending: '待初始化',
  analyzing: '分析中',
  ready: '已就绪',
  not_runnable: '不可运行',
  failed: '失败',
};

export const providerLabels: Record<RuntimeEnvironmentProvider, string> = {
  none: '未选择',
  local_connector: '本地连接器',
  harness: 'Harness',
  cloud_sandbox_manager: '云端沙箱',
};

export function runtimeStatusTag(status?: ProjectRuntimeEnvironmentStatus) {
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

export function providerTag(provider?: RuntimeEnvironmentProvider) {
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

export function imageStatusTag(status: string) {
  const normalized = status.trim().toLowerCase();
  const color =
    normalized === 'ready' || normalized === 'available' || normalized === 'running'
      ? 'success'
      : normalized === 'failed' || normalized === 'error'
        ? 'error'
        : normalized === 'creating' || normalized === 'pending' || normalized === 'building' || normalized === 'starting'
          ? 'processing'
          : normalized === 'planned'
            ? 'warning'
          : 'default';
  const label =
    normalized === 'planned'
      ? '待生成'
      : normalized === 'building' || normalized === 'starting'
        ? '启动中'
        : normalized === 'running'
          ? '运行中'
        : normalized === 'ready'
          ? '已生成'
          : status || '-';
  return <Tag color={color}>{label}</Tag>;
}

export function servicesFromJson(value: unknown): ServiceRow[] {
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

export function envVarsFromJson(value: unknown): LegacyEnvVarRow[] {
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

export function environmentVariablesFromResponse(
  response?: ProjectRuntimeEnvironmentResponse,
): ProjectRuntimeEnvironmentVariableRecord[] {
  const records = response?.environment.environment_variables;
  if (records?.length) {
    return records;
  }
  return envVarsFromJson(response?.environment.env_vars).map((row) => ({
    name: row.name,
    project_value: null,
    project_value_suitable: false,
    recommended_value: row.value == null ? null : String(row.value),
    user_value: null,
    effective_value: row.value == null ? null : String(row.value),
    effective_source: 'ai_recommended',
    description: '历史运行环境变量',
    recommendation_reason: '历史记录未包含来源信息',
    required: false,
    secret: isSecretName(row.name),
  }));
}

export function environmentVariableDrafts(
  records: ProjectRuntimeEnvironmentVariableRecord[],
): EnvVarDraft[] {
  return records.map((record, index) => {
    const currentValue = record.effective_value ?? '';
    return {
      ...record,
      rowKey: `${record.name}-${index}`,
      originalValue: currentValue,
      draftValue: currentValue,
      custom: record.project_value == null && record.recommended_value == null,
    };
  });
}

export function newEnvironmentVariableDraft(index: number): EnvVarDraft {
  return {
    rowKey: `new-${Date.now()}-${index}`,
    name: '',
    project_value: null,
    project_value_suitable: false,
    recommended_value: null,
    user_value: null,
    effective_value: null,
    effective_source: 'user',
    description: '用户自定义环境变量',
    recommendation_reason: null,
    required: false,
    secret: false,
    originalValue: '',
    draftValue: '',
    custom: true,
  };
}

export function variableSourceTag(source: RuntimeEnvironmentVariableSource) {
  const labels: Record<RuntimeEnvironmentVariableSource, string> = {
    project: '项目原值',
    ai_recommended: 'AI 生成',
    user: '用户修改',
    none: '未配置',
  };
  const colors: Record<RuntimeEnvironmentVariableSource, string> = {
    project: 'blue',
    ai_recommended: 'cyan',
    user: 'purple',
    none: 'default',
  };
  return <Tag color={colors[source]}>{labels[source]}</Tag>;
}

export function variableSourceDescription(record: ProjectRuntimeEnvironmentVariableRecord) {
  const description =
    record.effective_source === 'project'
      ? '从项目配置中读取，且可以直接用于当前运行环境。'
      : record.effective_source === 'ai_recommended'
        ? record.recommendation_reason || '项目中缺少可用值，由 AI 根据当前运行环境生成。'
        : record.effective_source === 'user'
          ? '用户已直接修改当前值。'
          : '尚未生成可用值。';
  return (
    <Tooltip title={description}>
      <Typography.Text type="secondary" ellipsis style={{ maxWidth: 420 }}>
        {description}
      </Typography.Text>
    </Tooltip>
  );
}

export function renderVariableValue(value: string | null | undefined, secret: boolean) {
  if (value === null || value === undefined) {
    return <Typography.Text type="secondary">-</Typography.Text>;
  }
  return (
    <Typography.Text code copyable={{ text: value }} style={codeTextStyle}>
      {secret ? '********' : value || '(空字符串)'}
    </Typography.Text>
  );
}

export function asRecord(value: unknown): JsonRecord | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  return value as JsonRecord;
}

export function fieldText(record: JsonRecord | undefined, fields: string[]) {
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

export function renderCompactJson(value: unknown) {
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

export function renderEnvVarSummary(value: unknown) {
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

export function renderJsonBlock(value: unknown) {
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

export function renderScalar(value: unknown, secret: boolean) {
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

export function isScalar(value: unknown) {
  return (
    value === null ||
    value === undefined ||
    typeof value === 'string' ||
    typeof value === 'number' ||
    typeof value === 'boolean'
  );
}

export function isEmptyJson(value: unknown) {
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

export function stringifyJson(value: unknown) {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function isSecretName(name: string) {
  return /(password|passwd|secret|token|credential|private|access_key|apikey|api_key)/i.test(name);
}
