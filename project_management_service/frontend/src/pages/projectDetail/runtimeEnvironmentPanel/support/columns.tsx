// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { DeleteOutlined, EyeOutlined, FileTextOutlined } from '@ant-design/icons';
import { formatFileSize } from '@chatos/frontend-runtime';
import { Button, Input, Space, Tag, Tooltip, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';

import type {
  ProjectRuntimeEnvironmentConfigFileRecord,
  ProjectRuntimeEnvironmentImageRecord,
  ProjectRuntimeEnvironmentVariableRecord,
} from '../../../../types';

import { codeTextStyle } from './layout';
import {
  fieldText,
  imageStatusTag,
  isSecretName,
  providerTag,
  renderCompactJson,
  renderEnvVarSummary,
  renderJsonBlock,
  renderVariableValue,
  variableSourceDescription,
  variableSourceTag,
} from './rendering';
import type { EnvVarDraft, ServiceRow } from './types';

export const serviceColumns: ColumnsType<ServiceRow> = [
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

export const environmentVariableColumns: ColumnsType<ProjectRuntimeEnvironmentVariableRecord> = [
  {
    title: '变量名',
    dataIndex: 'name',
    width: 230,
    fixed: 'left',
    render: (value: string, row) => (
      <Space direction="vertical" size={2}>
        <Space size={4}>
          <Typography.Text code copyable style={codeTextStyle}>
            {value}
          </Typography.Text>
          {row.required ? <Tag color="red">必填</Tag> : null}
          {row.secret ? <Tag color="gold">敏感</Tag> : null}
        </Space>
        {row.description ? (
          <Typography.Text type="secondary">{row.description}</Typography.Text>
        ) : null}
      </Space>
    ),
  },
  {
    title: '当前值',
    width: 360,
    render: (_, row) => (
      <>{renderVariableValue(row.effective_value, row.secret)}</>
    ),
  },
  {
    title: '来源',
    width: 150,
    render: (_, row) => variableSourceTag(row.effective_source),
  },
  {
    title: '来源说明',
    render: (_, row) => variableSourceDescription(row),
  },
];

export function generatedConfigFileColumns(
  onPreview: (file: ProjectRuntimeEnvironmentConfigFileRecord) => void,
): ColumnsType<ProjectRuntimeEnvironmentConfigFileRecord> {
  return [
    {
      title: '文件',
      dataIndex: 'path',
      width: 340,
      render: (value: string) => (
        <Space size={6}>
          <FileTextOutlined />
          <Typography.Text code copyable style={codeTextStyle}>
            {value}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: '格式',
      dataIndex: 'format',
      width: 120,
      render: (value: string) => <Tag>{value || 'text'}</Tag>,
    },
    {
      title: '用途',
      dataIndex: 'description',
      render: (value?: string | null) => value || '-',
    },
    {
      title: '推断来源',
      width: 240,
      render: (_, row) =>
        row.source_files.length > 0 ? (
          <Tooltip title={row.source_files.join('\n')}>
            <Typography.Text type="secondary">
              {row.source_files.length} 个项目文件
            </Typography.Text>
          </Tooltip>
        ) : (
          '-'
        ),
    },
    {
      title: '大小',
      width: 100,
      render: (_, row) => formatFileSize(new Blob([row.content]).size),
    },
    {
      title: '操作',
      width: 100,
      fixed: 'right',
      render: (_, row) => (
        <Button type="link" icon={<EyeOutlined />} onClick={() => onPreview(row)}>
          查看
        </Button>
      ),
    },
  ];
}

export function variableEditorColumns(
  drafts: EnvVarDraft[],
  setDrafts: (value: EnvVarDraft[] | ((current: EnvVarDraft[]) => EnvVarDraft[])) => void,
): ColumnsType<EnvVarDraft> {
  const update = (rowKey: string, patch: Partial<EnvVarDraft>) => {
    setDrafts((current) =>
      current.map((item) => (item.rowKey === rowKey ? { ...item, ...patch } : item)),
    );
  };
  return [
    {
      title: '变量名',
      width: 220,
      render: (_, row) =>
        row.custom ? (
          <Input
            value={row.name}
            placeholder="例如 APP_PORT"
            onChange={(event) => {
              const name = event.target.value.toUpperCase();
              update(row.rowKey, { name, secret: isSecretName(name) });
            }}
          />
        ) : (
          <Typography.Text code>{row.name}</Typography.Text>
        ),
    },
    {
      title: '来源',
      width: 140,
      render: (_, row) => variableSourceTag(row.effective_source),
    },
    {
      title: '当前值',
      width: 480,
      render: (_, row) => (
        <>
          {row.secret ? (
            <Input.Password
              value={row.draftValue}
              placeholder="输入当前值"
              onChange={(event) => update(row.rowKey, { draftValue: event.target.value })}
            />
          ) : (
            <Input
              value={row.draftValue}
              placeholder="输入当前值"
              onChange={(event) => update(row.rowKey, { draftValue: event.target.value })}
            />
          )}
        </>
      ),
    },
    {
      title: '操作',
      width: 70,
      fixed: 'right',
      render: (_, row) =>
        row.custom ? (
          <Button
            type="text"
            danger
            icon={<DeleteOutlined />}
            onClick={() => setDrafts(drafts.filter((item) => item.rowKey !== row.rowKey))}
          />
        ) : null,
    },
  ];
}

export function imageColumns({
  onPreviewDockerfile,
}: {
  onPreviewDockerfile: (image: ProjectRuntimeEnvironmentImageRecord) => void;
}): ColumnsType<ProjectRuntimeEnvironmentImageRecord> {
  return [
    {
    title: '环境',
    width: 240,
    render: (_, row) => (
      <Space direction="vertical" size={2}>
        <Typography.Text strong>{row.display_name || row.environment_key}</Typography.Text>
        <Space size={4} wrap>
          <Tag>{row.environment_type || 'service'}</Tag>
          <Tag color={row.service_role === 'application' ? 'blue' : undefined}>
            {row.service_role || 'unknown'}
          </Tag>
          <Typography.Text type="secondary" code style={codeTextStyle}>
            {row.service_id || row.environment_key}
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
    title: 'MCP 策略',
    width: 150,
    render: (_, row) =>
      row.mcp_policy?.attachment === 'project_gateway_target' ? (
        <Space direction="vertical" size={2}>
          <Tag color="green">系统管理目标</Tag>
          <Typography.Text type="secondary">
            {row.mcp_policy.filesystem ? '文件' : ''}
            {row.mcp_policy.filesystem && row.mcp_policy.terminal ? ' / ' : ''}
            {row.mcp_policy.terminal ? '终端' : ''}
          </Typography.Text>
        </Space>
      ) : (
        <Tag>无 MCP</Tag>
      ),
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
    {
      title: 'Dockerfile',
      width: 120,
      render: (_, row) =>
        row.dockerfile ? (
          <Button type="link" icon={<EyeOutlined />} onClick={() => onPreviewDockerfile(row)}>
            查看
          </Button>
        ) : (
          '-'
        ),
    },
  ];
}
