// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  DeleteOutlined,
  EditOutlined,
  EyeOutlined,
  FileTextOutlined,
  PlayCircleOutlined,
  PlusOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import {
  Alert,
  Button,
  Descriptions,
  Empty,
  Input,
  Modal,
  Space,
  Switch,
  Table,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';
import { useEffect, useMemo, useState, type CSSProperties, type ReactNode } from 'react';

import type {
  ProjectRuntimeEnvironmentConfigFileRecord,
  ProjectRuntimeEnvironmentImageRecord,
  ProjectRuntimeEnvironmentResponse,
  ProjectRuntimeEnvironmentStatus,
  ProjectRuntimeEnvironmentVariableRecord,
  RuntimeEnvironmentProvider,
  RuntimeEnvironmentVariableSource,
  UpdateProjectRuntimeEnvironmentVariablesPayload,
} from '../../types';

interface RuntimeEnvironmentPanelProps {
  response?: ProjectRuntimeEnvironmentResponse;
  loading: boolean;
  errorMessage?: string;
  analyzing: boolean;
  settingsSaving: boolean;
  variablesSaving: boolean;
  environmentStarting: boolean;
  onAnalyze: () => void;
  onRefresh: () => void;
  onSandboxEnabledChange: (value: boolean) => void;
  onSaveEnvironmentVariables: (
    payload: UpdateProjectRuntimeEnvironmentVariablesPayload,
  ) => Promise<void>;
  onStartEnvironment: () => void;
}

type JsonRecord = Record<string, unknown>;

interface ServiceRow {
  rowKey: string;
  serviceKey: string;
  raw: unknown;
  record?: JsonRecord;
}

interface EnvVarDraft extends ProjectRuntimeEnvironmentVariableRecord {
  rowKey: string;
  originalValue: string;
  draftValue: string;
  custom: boolean;
}

interface LegacyEnvVarRow {
  key: string;
  name: string;
  value: unknown;
}

const runtimeStatusLabels: Record<ProjectRuntimeEnvironmentStatus, string> = {
  disabled: '已停用',
  pending_configuration: '等待配置',
  pending_image_build: '等待启动环境',
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
  variablesSaving,
  environmentStarting,
  onAnalyze,
  onRefresh,
  onSandboxEnabledChange,
  onSaveEnvironmentVariables,
  onStartEnvironment,
}: RuntimeEnvironmentPanelProps) {
  const environment = response?.environment;
  const images = response?.images ?? [];
  const composeFile = (environment?.generated_config_files ?? []).find(
    (file) => file.path === '.chatos/runtime-environment/docker-compose.chatos.yml',
  );
  const generatedConfigFiles = (environment?.generated_config_files ?? []).filter(
    (file) => file.path !== '.chatos/runtime-environment/docker-compose.chatos.yml',
  );
  const serviceRows = servicesFromJson(environment?.required_services);
  const envVarRows = useMemo(
    () => environmentVariablesFromResponse(response),
    [environment?.environment_variables, environment?.env_vars],
  );
  const canAnalyze = Boolean(environment?.sandbox_enabled) && !analyzing;
  const [variableEditorOpen, setVariableEditorOpen] = useState(false);
  const [variableDrafts, setVariableDrafts] = useState<EnvVarDraft[]>([]);
  const [previewConfigFile, setPreviewConfigFile] =
    useState<ProjectRuntimeEnvironmentConfigFileRecord>();
  const [previewDockerfileImage, setPreviewDockerfileImage] =
    useState<ProjectRuntimeEnvironmentImageRecord>();

  useEffect(() => {
    if (!variableEditorOpen) {
      setVariableDrafts(environmentVariableDrafts(envVarRows));
    }
  }, [envVarRows, variableEditorOpen]);

  const variableNamesValid = useMemo(() => {
    const names = variableDrafts.map((item) => item.name.trim().toUpperCase());
    return (
      names.every((name) => /^[A-Z_][A-Z0-9_]*$/.test(name)) &&
      new Set(names).size === names.length
    );
  }, [variableDrafts]);

  const openVariableEditor = () => {
    setVariableDrafts(environmentVariableDrafts(envVarRows));
    setVariableEditorOpen(true);
  };

  const saveVariableOverrides = async () => {
    if (!variableNamesValid) {
      return;
    }
    await onSaveEnvironmentVariables({
      variables: variableDrafts
        .filter(
          (item) =>
            item.custom ||
            item.effective_source === 'user' ||
            item.draftValue !== item.originalValue,
        )
        .map((item) => ({
          name: item.name.trim().toUpperCase(),
          value: item.draftValue,
        })),
    });
    setVariableEditorOpen(false);
  };

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
      {environment?.status === 'pending_image_build' ? (
        <Alert
          type="info"
          showIcon
          message="项目级 Docker Compose 编排已生成，请在下方确认后一次性启动整个环境。"
        />
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

      <RuntimeSection
        title="项目级环境编排"
        extra={
          <Space>
            <Button
              icon={<EyeOutlined />}
              disabled={!composeFile}
              onClick={() => composeFile && setPreviewConfigFile(composeFile)}
            >
              查看 Compose
            </Button>
            <Button
              type="primary"
              icon={<PlayCircleOutlined />}
              loading={environmentStarting}
              disabled={!composeFile || !environment?.sandbox_enabled || analyzing}
              onClick={onStartEnvironment}
            >
              {environment?.status === 'ready' ? '重新生成并启动' : '生成并启动整个环境'}
            </Button>
          </Space>
        }
      >
        <Alert
          type="info"
          showIcon
          message="应用、数据库、缓存和配置中心会被放在同一个 Docker Compose 项目下，在 Docker Desktop 中作为一个整体展开和管理。"
        />
        <Descriptions bordered size="small" column={{ xs: 1, md: 2 }} style={{ marginTop: 12 }}>
          <Descriptions.Item label="编排文件">
            {composeFile ? <Typography.Text code>{composeFile.path}</Typography.Text> : '-'}
          </Descriptions.Item>
          <Descriptions.Item label="包含服务">
            {images.length > 0 ? `${images.length} 个（应用 + 依赖）` : '-'}
          </Descriptions.Item>
        </Descriptions>
      </RuntimeSection>

      <RuntimeSection
        title="环境变量"
        extra={
          <Button icon={<EditOutlined />} onClick={openVariableEditor} disabled={!environment}>
            编辑变量
          </Button>
        }
      >
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 12 }}
          message="每个变量只有一个当前值。系统优先采用适合当前环境的项目值，否则由 AI 生成可运行值；用户可以直接修改当前值。"
        />
        <Table<ProjectRuntimeEnvironmentVariableRecord>
          rowKey="name"
          size="small"
          loading={loading}
          dataSource={envVarRows}
          locale={{ emptyText: '暂无环境变量' }}
          pagination={{ pageSize: 8 }}
          scroll={{ x: 1320 }}
          columns={environmentVariableColumns}
        />
      </RuntimeSection>

      <RuntimeSection title="生成的环境配置文件">
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 12 }}
          message="这些文件由环境初始化 Agent 根据项目代码、原始配置和当前环境变量生成，不会覆盖项目已有配置文件。"
        />
        <Table<ProjectRuntimeEnvironmentConfigFileRecord>
          rowKey="path"
          size="small"
          loading={loading}
          dataSource={generatedConfigFiles}
          locale={{ emptyText: '当前项目无需生成额外环境配置文件' }}
          pagination={{ pageSize: 8 }}
          scroll={{ x: 1080 }}
          columns={generatedConfigFileColumns(setPreviewConfigFile)}
        />
      </RuntimeSection>

      <Modal
        title="编辑运行环境变量"
        open={variableEditorOpen}
        width={1180}
        confirmLoading={variablesSaving}
        okButtonProps={{ disabled: !variableNamesValid }}
        okText="保存并应用"
        cancelText="取消"
        onCancel={() => setVariableEditorOpen(false)}
        onOk={() => void saveVariableOverrides()}
      >
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 16 }}
          message="直接编辑当前值即可。保存后该变量来源会标记为“用户修改”，重新分析项目时会保留用户已经修改的值。"
        />
        <Table<EnvVarDraft>
          rowKey="rowKey"
          size="small"
          pagination={false}
          scroll={{ x: 1080, y: 520 }}
          dataSource={variableDrafts}
          columns={variableEditorColumns(variableDrafts, setVariableDrafts)}
        />
        <Button
          type="dashed"
          icon={<PlusOutlined />}
          style={{ marginTop: 12, width: '100%' }}
          onClick={() =>
            setVariableDrafts((current) => [
              ...current,
              newEnvironmentVariableDraft(current.length),
            ])
          }
        >
          添加自定义变量
        </Button>
        {!variableNamesValid ? (
          <Typography.Text type="danger" style={{ display: 'block', marginTop: 8 }}>
            变量名必须以字母或下划线开头，只能包含大写字母、数字和下划线，并且不能重复。
          </Typography.Text>
        ) : null}
      </Modal>

      <Modal
        title={
          <Space>
            <FileTextOutlined />
            <span>{previewConfigFile?.path || '环境配置文件'}</span>
          </Space>
        }
        open={Boolean(previewConfigFile)}
        width={1100}
        footer={null}
        onCancel={() => setPreviewConfigFile(undefined)}
      >
        {previewConfigFile ? (
          <Space direction="vertical" size="middle" style={{ width: '100%' }}>
            <Descriptions bordered size="small" column={{ xs: 1, md: 2 }}>
              <Descriptions.Item label="文件路径">
                <Typography.Text code copyable>
                  {previewConfigFile.path}
                </Typography.Text>
              </Descriptions.Item>
              <Descriptions.Item label="格式">
                <Tag>{previewConfigFile.format || 'text'}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label="用途" span={2}>
                {previewConfigFile.description || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="推断来源" span={2}>
                {previewConfigFile.source_files.length > 0 ? (
                  <Space size={[4, 4]} wrap>
                    {previewConfigFile.source_files.map((path) => (
                      <Typography.Text key={path} code>
                        {path}
                      </Typography.Text>
                    ))}
                  </Space>
                ) : (
                  '-'
                )}
              </Descriptions.Item>
            </Descriptions>
            <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <Typography.Text copyable={{ text: previewConfigFile.content }}>
                复制文件内容
              </Typography.Text>
            </div>
            <pre
              style={{
                maxHeight: 620,
                margin: 0,
                padding: 16,
                overflow: 'auto',
                borderRadius: 8,
                background: '#0d1117',
                color: '#e6edf3',
                fontSize: 13,
                lineHeight: 1.6,
                whiteSpace: 'pre',
              }}
            >
              {previewConfigFile.content}
            </pre>
          </Space>
        ) : null}
      </Modal>

      <RuntimeSection title="Compose 服务计划">
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 12 }}
          message="这里展示总编排中的各个服务。应用使用 Agent 生成的 Dockerfile，数据库、缓存和配置中心使用平台维护的标准镜像。"
        />
        <Table<ProjectRuntimeEnvironmentImageRecord>
          rowKey="id"
          size="small"
          loading={loading}
          dataSource={images}
          locale={{ emptyText: '暂无镜像记录' }}
          pagination={{ pageSize: 8 }}
          scroll={{ x: 1260 }}
          columns={imageColumns({
            onPreviewDockerfile: setPreviewDockerfileImage,
          })}
        />
      </RuntimeSection>

      <Modal
        title={`Dockerfile · ${previewDockerfileImage?.display_name || previewDockerfileImage?.environment_key || ''}`}
        open={Boolean(previewDockerfileImage)}
        width={1100}
        footer={null}
        onCancel={() => setPreviewDockerfileImage(undefined)}
      >
        {previewDockerfileImage?.dockerfile ? (
          <Space direction="vertical" size="middle" style={{ width: '100%' }}>
            <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <Typography.Text copyable={{ text: previewDockerfileImage.dockerfile }}>
                复制 Dockerfile
              </Typography.Text>
            </div>
            <pre
              style={{
                maxHeight: 620,
                margin: 0,
                padding: 16,
                overflow: 'auto',
                borderRadius: 8,
                background: '#0d1117',
                color: '#e6edf3',
                fontSize: 13,
                lineHeight: 1.6,
                whiteSpace: 'pre',
              }}
            >
              {previewDockerfileImage.dockerfile}
            </pre>
          </Space>
        ) : (
          <Empty description="暂无 Dockerfile" />
        )}
      </Modal>
    </Space>
  );
}

function RuntimeSection({
  title,
  extra,
  children,
}: {
  title: string;
  extra?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section style={sectionStyle}>
      <div style={sectionHeaderStyle}>
        <Typography.Title level={5} style={{ margin: 0 }}>
          {title}
        </Typography.Title>
        {extra}
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

const environmentVariableColumns: ColumnsType<ProjectRuntimeEnvironmentVariableRecord> = [
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

function generatedConfigFileColumns(
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

function variableEditorColumns(
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

function imageColumns({
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

function envVarsFromJson(value: unknown): LegacyEnvVarRow[] {
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

function environmentVariablesFromResponse(
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

function environmentVariableDrafts(
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

function newEnvironmentVariableDraft(index: number): EnvVarDraft {
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

function variableSourceTag(source: RuntimeEnvironmentVariableSource) {
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

function variableSourceDescription(record: ProjectRuntimeEnvironmentVariableRecord) {
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

function renderVariableValue(value: string | null | undefined, secret: boolean) {
  if (value === null || value === undefined) {
    return <Typography.Text type="secondary">-</Typography.Text>;
  }
  return (
    <Typography.Text code copyable={{ text: value }} style={codeTextStyle}>
      {secret ? '********' : value || '(空字符串)'}
    </Typography.Text>
  );
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

function formatFileSize(bytes: number) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
