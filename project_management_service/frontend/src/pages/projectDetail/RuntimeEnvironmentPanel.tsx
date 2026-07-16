// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
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
  Modal,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import { useEffect, useMemo, useState } from 'react';

import type {
  ProjectRuntimeEnvironmentConfigFileRecord,
  ProjectRuntimeEnvironmentImageRecord,
  ProjectRuntimeEnvironmentResponse,
  ProjectRuntimeEnvironmentVariableRecord,
  UpdateProjectRuntimeEnvironmentVariablesPayload,
} from '../../types';

import {
  RuntimeSection,
  codeTextStyle,
  environmentVariableColumns,
  environmentVariableDrafts,
  environmentVariablesFromResponse,
  formatDateTime,
  generatedConfigFileColumns,
  imageColumns,
  isEmptyJson,
  newEnvironmentVariableDraft,
  providerTag,
  renderJsonBlock,
  runtimeStatusTag,
  serviceColumns,
  servicesFromJson,
  variableEditorColumns,
  type EnvVarDraft,
  type ServiceRow,
} from './runtimeEnvironmentPanel/support';

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
