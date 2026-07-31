// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo } from 'react';
import type { FormInstance } from 'antd';
import {
  Alert,
  Button,
  Checkbox,
  Drawer,
  Form,
  Input,
  InputNumber,
  Select,
  Space,
  Switch,
  Tag,
  Typography,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  ExternalMcpConfigRecord,
  McpCatalogEntry,
  RemoteServerRecord,
  SelectableTaskPlugin,
  TaskPluginConnectorsResponse,
  TaskProjectRuntimeEnvironmentResponse,
  TaskRecord,
  TaskScheduleMode,
} from '../../types';
import {
  CODE_MAINTAINER_READ_KIND,
  CODE_MAINTAINER_WRITE_KIND,
  PROJECT_MANAGEMENT_KIND,
  completeEnabledBuiltinKindDependencies,
  scheduleModeDescriptionKeys,
  scheduleModeLabelKeys,
  taskProfileLabel,
  taskPluginAgentKey,
  taskPluginCommandKey,
  taskProfileValues,
  taskStatusValues,
  type TaskFormValues,
} from './taskPageUtils';

type SelectOption = {
  label: string;
  value: string;
  disabled?: boolean;
};

type TaskEditorDrawerProps = {
  t: TranslateFn;
  open: boolean;
  editingTask: TaskRecord | null;
  form: FormInstance<TaskFormValues>;
  saving: boolean;
  modelOptions: SelectOption[];
  projectOptions: SelectOption[];
  prerequisiteTaskOptions: SelectOption[];
  mcpCatalogEntries?: McpCatalogEntry[];
  remoteServers?: RemoteServerRecord[];
  externalMcpConfigs?: ExternalMcpConfigRecord[];
  selectablePlugins?: SelectableTaskPlugin[];
  pluginConnectors?: TaskPluginConnectorsResponse;
  pluginConnectorsLoading?: boolean;
  pluginConnectorsUnavailable?: boolean;
  pluginCatalogLoading?: boolean;
  runtimeEnvironment?: TaskProjectRuntimeEnvironmentResponse;
  runtimeEnvironmentLoading?: boolean;
  runtimeEnvironmentUnavailable?: boolean;
  onClose: () => void;
  onSubmit: (values: TaskFormValues) => void;
  onPreviewPrompt: () => void;
  onManageServers: () => void;
  onViewMcpCatalog: () => void;
};

export function TaskEditorDrawer({
  t,
  open,
  editingTask,
  form,
  saving,
  modelOptions,
  projectOptions,
  prerequisiteTaskOptions,
  mcpCatalogEntries = [],
  remoteServers = [],
  externalMcpConfigs = [],
  selectablePlugins = [],
  pluginConnectors,
  pluginConnectorsLoading = false,
  pluginConnectorsUnavailable = false,
  pluginCatalogLoading = false,
  runtimeEnvironment,
  runtimeEnvironmentLoading = false,
  runtimeEnvironmentUnavailable = false,
  onClose,
  onSubmit,
  onPreviewPrompt,
  onManageServers,
  onViewMcpCatalog,
}: TaskEditorDrawerProps) {
  const mcpEnabled = Form.useWatch('mcpEnabled', form);
  const taskProfile = Form.useWatch('taskProfile', form);
  const selectedProjectId = Form.useWatch('projectId', form);
  const requiresExecution = Form.useWatch('requiresExecution', form);
  const enabledBuiltinKinds = Form.useWatch('enabledBuiltinKinds', form) || [];
  const defaultRemoteServerId = Form.useWatch('defaultRemoteServerId', form);
  const scheduleMode = Form.useWatch('scheduleMode', form);
  const pluginDeviceId = Form.useWatch('pluginDeviceId', form);
  const pluginWorkspaceId = Form.useWatch('pluginWorkspaceId', form);
  const selectedPluginIds = Form.useWatch('selectedPluginIds', form) || [];
  const pluginCommandSelections =
    Form.useWatch('pluginCommandSelections', form) || {};
  const effectiveScheduleMode = scheduleMode ?? 'manual';
  const scheduleModeLabels = useMemo(
    () =>
      Object.fromEntries(
        (['manual', 'once', 'interval', 'contact_async'] as TaskScheduleMode[]).map(
          (value) => [value, t(scheduleModeLabelKeys[value])],
        ),
      ) as Record<TaskScheduleMode, string>,
    [t],
  );
  const scheduleModeDescriptions = useMemo(
    () =>
      Object.fromEntries(
        (['manual', 'once', 'interval', 'contact_async'] as TaskScheduleMode[]).map(
          (value) => [value, t(scheduleModeDescriptionKeys[value])],
        ),
      ) as Record<TaskScheduleMode, string>,
    [t],
  );
  const scheduleModeOptions = useMemo(
    () =>
      (['manual', 'once', 'interval', 'contact_async'] as TaskScheduleMode[]).map(
        (value) => ({
          label: scheduleModeLabels[value],
          value,
          disabled: value === 'contact_async',
        }),
      ),
    [scheduleModeLabels],
  );
  const taskStatusOptions = useMemo(
    () =>
      taskStatusValues.map((value) => ({
        label: t(`tasks.status.${value}`),
        value,
      })),
    [t],
  );
  const taskProfileOptions = useMemo(
    () =>
      taskProfileValues.map((value) => ({
        label: taskProfileLabel(value, t),
        value,
      })),
    [t],
  );
  const mcpOptions = useMemo(
    () =>
      mcpCatalogEntries
        .filter((entry) => entry.kind !== PROJECT_MANAGEMENT_KIND)
        .map((entry) => ({
          label: entry.kind,
          value: entry.kind,
          disabled: !entry.implemented,
          description: entry.description,
          useCases: entry.use_cases,
          capabilities: entry.capabilities,
          message: entry.message || undefined,
        })),
    [mcpCatalogEntries],
  );
  useEffect(() => {
    if (!mcpCatalogEntries.length || !enabledBuiltinKinds.length) {
      return;
    }
    const selectable = new Set(mcpOptions.map((option) => option.value));
    const filtered = enabledBuiltinKinds.filter((kind) => selectable.has(kind));
    if (filtered.length !== enabledBuiltinKinds.length) {
      form.setFieldsValue({ enabledBuiltinKinds: filtered });
    }
  }, [enabledBuiltinKinds, form, mcpCatalogEntries.length, mcpOptions]);
  const remoteControllerEntry = useMemo(
    () =>
      mcpCatalogEntries.find((entry) => entry.kind === 'RemoteConnectionController') ||
      null,
    [mcpCatalogEntries],
  );
  const enabledRemoteServerCount = useMemo(
    () => remoteServers.filter((item) => item.enabled).length,
    [remoteServers],
  );
  const remoteServerTotalCount = remoteServers.length;
  const remoteControllerEffectiveSelected = Boolean(
    mcpEnabled &&
      (enabledBuiltinKinds.length === 0
        ? remoteControllerEntry
        : enabledBuiltinKinds.includes('RemoteConnectionController')),
  );
  const codeMaintainerWriteSelected = enabledBuiltinKinds.includes(
    CODE_MAINTAINER_WRITE_KIND,
  );
  useEffect(() => {
    const completed = completeEnabledBuiltinKindDependencies(enabledBuiltinKinds);
    if (
      completed.length !== enabledBuiltinKinds.length ||
      completed.some((value, index) => value !== enabledBuiltinKinds[index])
    ) {
      form.setFieldsValue({ enabledBuiltinKinds: completed });
    }
  }, [enabledBuiltinKinds, form]);
  const remoteServerMap = useMemo(() => {
    const map = new Map<string, RemoteServerRecord>();
    remoteServers.forEach((server) => {
      map.set(server.id, server);
    });
    return map;
  }, [remoteServers]);
  const remoteServerOptions = useMemo(
    () =>
      remoteServers.map((server) => ({
        label: `${server.name} (${server.host}:${server.port})${server.enabled ? '' : ' / disabled'}`,
        value: server.id,
        disabled: !server.enabled,
      })),
    [remoteServers],
  );
  const externalMcpConfigOptions = useMemo(
    () =>
      externalMcpConfigs
        .filter((config) => config.enabled)
        .map((config) => ({
          label: `${config.name} (${config.transport})`,
          value: config.id,
        })),
    [externalMcpConfigs],
  );
  const onlinePluginDevices = useMemo(
    () => (pluginConnectors?.devices || []).filter((device) => device.status === 'online'),
    [pluginConnectors?.devices],
  );
  const pluginDeviceOptions = useMemo(
    () =>
      (pluginConnectors?.devices || []).map((device) => ({
        label: `${device.display_name} (${device.os || 'unknown'}) / ${device.status}`,
        value: device.id,
        disabled: device.status !== 'online',
      })),
    [pluginConnectors?.devices],
  );
  const activePluginWorkspaces = useMemo(
    () =>
      (pluginConnectors?.workspaces || []).filter(
        (workspace) =>
          workspace.device_id === pluginDeviceId && workspace.status === 'active',
      ),
    [pluginConnectors?.workspaces, pluginDeviceId],
  );
  const pluginWorkspaceOptions = useMemo(
    () =>
      (pluginConnectors?.workspaces || [])
        .filter((workspace) => workspace.device_id === pluginDeviceId)
        .map((workspace) => ({
          label: `${workspace.display_name} (${workspace.local_path_alias}) / ${workspace.status}`,
          value: workspace.id,
          disabled: workspace.status !== 'active',
        })),
    [pluginConnectors?.workspaces, pluginDeviceId],
  );
  const pluginOptions = useMemo(
    () =>
      selectablePlugins.map((plugin) => ({
        label: `${plugin.display_name} / v${plugin.version}`,
        value: plugin.id,
      })),
    [selectablePlugins],
  );
  const selectedPluginDetails = useMemo(
    () =>
      selectedPluginIds.map((pluginId) => ({
        pluginId,
        plugin: selectablePlugins.find((candidate) => candidate.id === pluginId),
      })),
    [selectablePlugins, selectedPluginIds],
  );
  const browserPluginSelected = selectedPluginDetails.some(
    ({ plugin }) => plugin?.plugin_key === 'browser',
  );
  const pluginAgentOptions = useMemo(
    () =>
      selectedPluginDetails.flatMap(({ pluginId, plugin }) =>
        (plugin?.agents || []).map((agent) => ({
          label: `${plugin?.display_name || pluginId} / ${agent.display_name} / ${agent.base_agent} / ${agent.max_iterations}`,
          value: taskPluginAgentKey(pluginId, agent.agent_id),
        })),
      ),
    [selectedPluginDetails],
  );
  useEffect(() => {
    if (pluginDeviceId || onlinePluginDevices.length !== 1) {
      return;
    }
    form.setFieldValue('pluginDeviceId', onlinePluginDevices[0].id);
  }, [form, onlinePluginDevices, pluginDeviceId]);
  useEffect(() => {
    if (!pluginDeviceId || pluginWorkspaceId || activePluginWorkspaces.length !== 1) {
      return;
    }
    form.setFieldValue('pluginWorkspaceId', activePluginWorkspaces[0].id);
  }, [activePluginWorkspaces, form, pluginDeviceId, pluginWorkspaceId]);
  return (
    <Drawer
      title={editingTask ? t('tasks.drawer.edit') : t('tasks.drawer.create')}
      open={open}
      width={820}
      destroyOnClose
      onClose={onClose}
      extra={
        <Space>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button type="primary" loading={saving} onClick={() => form.submit()}>
            {t('common.save')}
          </Button>
        </Space>
      }
    >
      <Form<TaskFormValues> layout="vertical" form={form} onFinish={onSubmit}>
        <Form.Item
          name="title"
          label={t('tasks.form.title')}
          rules={[{ required: true, message: t('tasks.form.titleRequired') }]}
        >
          <Input />
        </Form.Item>
        <Form.Item
          name="objective"
          label={t('tasks.form.objective')}
          rules={[{ required: true, message: t('tasks.form.objectiveRequired') }]}
        >
          <Input.TextArea rows={4} />
        </Form.Item>
        <Form.Item name="description" label={t('tasks.form.description')}>
          <Input.TextArea rows={3} />
        </Form.Item>

        <Space size="middle" style={{ width: '100%' }} align="start">
          <Form.Item
            name="status"
            label={t('common.status')}
            style={{ flex: '0 0 220px', minWidth: 220 }}
          >
            <Select style={{ width: '100%' }} options={taskStatusOptions} />
          </Form.Item>
          <Form.Item
            name="taskProfile"
            label={t('tasks.form.taskProfile')}
            style={{ flex: '0 0 220px', minWidth: 220 }}
          >
            <Select style={{ width: '100%' }} options={taskProfileOptions} />
          </Form.Item>
          <Form.Item name="priority" label={t('tasks.column.priority')} style={{ width: 140 }}>
            <InputNumber style={{ width: '100%' }} />
          </Form.Item>
        </Space>
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 16 }}
          message={
            taskProfile === 'chatos_plan' && !requiresExecution
              ? t('tasks.form.agentPlanning')
              : t('tasks.form.agentExecution')
          }
          description={
            taskProfile === 'chatos_plan' && !requiresExecution
              ? t('tasks.form.agentPlanningHelp')
              : t('tasks.form.agentExecutionHelp')
          }
        />

        <Form.Item name="default_model_config_id" label={t('tasks.form.defaultModel')}>
          <Select
            allowClear
            options={modelOptions}
            placeholder={t('tasks.form.modelPlaceholder')}
          />
        </Form.Item>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'minmax(0, 1fr) 220px',
            columnGap: 16,
            alignItems: 'start',
          }}
        >
          <Form.Item
            name="projectId"
            label={t('tasks.form.project')}
            rules={[{ required: true, message: t('tasks.form.projectRequired') }]}
          >
            <Select
              showSearch
              optionFilterProp="label"
              options={projectOptions}
              placeholder={t('tasks.form.projectPlaceholder')}
              onChange={(value) => {
                if (value !== selectedProjectId) {
                  form.setFieldValue('prerequisite_task_ids', []);
                }
              }}
            />
          </Form.Item>
          <Form.Item
            name="requiresExecution"
            label={t('tasks.form.requiresExecution')}
            valuePropName="checked"
            extra={t('tasks.form.requiresExecutionHelp')}
          >
            <Switch />
          </Form.Item>
        </div>
        {selectedProjectId && selectedProjectId !== '-1' ? (
          <Space direction="vertical" size={8} style={{ width: '100%', marginBottom: 16 }}>
            <Typography.Text strong>{t('tasks.form.runtimeTopology')}</Typography.Text>
            {runtimeEnvironmentLoading ? (
              <Typography.Text type="secondary">
                {t('tasks.form.runtimeTopologyLoading')}
              </Typography.Text>
            ) : runtimeEnvironmentUnavailable ? (
              <Alert
                type="warning"
                showIcon
                message={t('tasks.form.runtimeTopologyUnavailable')}
              />
            ) : runtimeEnvironment ? (
              <div
                style={{
                  border: '1px solid #e5e7eb',
                  borderRadius: 8,
                  padding: 12,
                  background: '#fafafa',
                }}
              >
                <Space direction="vertical" size={8} style={{ width: '100%' }}>
                  <Space wrap>
                    <Tag color="blue">
                      Compose / {runtimeEnvironment.environment.status || 'unknown'}
                    </Tag>
                    <Typography.Text type="secondary">
                      {t('tasks.form.runtimeTopologyParent')}
                    </Typography.Text>
                  </Space>
                  <Space wrap>
                    {runtimeEnvironment.images.map((image) => {
                      const isWorkspace = image.service_role === 'workspace';
                      const isApplication = image.service_role === 'application';
                      const isArtifact = image.service_role === 'artifact';
                      return (
                        <Tag
                          key={image.service_id || image.environment_key}
                          color={isWorkspace ? 'green' : isApplication ? 'blue' : 'default'}
                        >
                          {image.display_name || image.service_id} / {image.service_id} /{' '}
                          {isWorkspace
                            ? t('tasks.form.runtimeWorkspaceTarget')
                            : isApplication
                              ? t('tasks.form.runtimePeerApplication')
                              : isArtifact
                                ? t('tasks.form.runtimeArtifact')
                                : t('tasks.form.runtimeDependencyNoMcp')}
                        </Tag>
                      );
                    })}
                  </Space>
                </Space>
              </div>
            ) : null}
          </Space>
        ) : null}
        <Form.Item name="prerequisite_task_ids" label="前置任务">
          <Select
            mode="multiple"
            allowClear
            showSearch
            options={prerequisiteTaskOptions}
            optionFilterProp="label"
            placeholder="选择必须先完成的任务"
          />
        </Form.Item>
        <Form.Item name="tagsText" label={t('tasks.form.tags')}>
          <Input placeholder={t('tasks.form.tagsPlaceholder')} />
        </Form.Item>

        <Typography.Title level={5} style={{ marginTop: 8 }}>
          {t('tasks.form.plugins')}
        </Typography.Title>

        {pluginConnectorsUnavailable ? (
          <Alert
            type="warning"
            showIcon
            style={{ marginBottom: 16 }}
            message={t('tasks.form.pluginConnectorsUnavailable')}
          />
        ) : !pluginConnectorsLoading && !onlinePluginDevices.length ? (
          <Alert
            type="info"
            showIcon
            style={{ marginBottom: 16 }}
            message={t('tasks.form.pluginConnectorsOffline')}
          />
        ) : null}

        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'minmax(0, 1fr) minmax(0, 1fr)',
            columnGap: 16,
            alignItems: 'start',
          }}
        >
          <Form.Item
            name="pluginDeviceId"
            label={t('tasks.form.pluginDevice')}
            extra={t('tasks.form.pluginDeviceHelp')}
            rules={[
              {
                validator: async (_, value) => {
                  if (!selectedPluginIds.length || value) {
                    return;
                  }
                  throw new Error(t('tasks.form.pluginDeviceRequired'));
                },
              },
            ]}
          >
            <Select
              allowClear
              showSearch
              loading={pluginConnectorsLoading}
              optionFilterProp="label"
              options={pluginDeviceOptions}
              placeholder={t('tasks.form.pluginDevicePlaceholder')}
              onChange={(value) => {
                if (value !== pluginDeviceId) {
                  form.setFieldsValue({
                    pluginWorkspaceId: undefined,
                    selectedPluginIds: [],
                    pluginCommandSelections: {},
                    pluginCommandArguments: {},
                    pluginAgentSelection: undefined,
                  });
                }
              }}
            />
          </Form.Item>
          <Form.Item
            name="pluginWorkspaceId"
            label={t('tasks.form.pluginWorkspace')}
            extra={t('tasks.form.pluginWorkspaceHelp')}
            rules={[
              {
                validator: async (_, value) => {
                  if (!browserPluginSelected || value) {
                    return;
                  }
                  throw new Error(t('tasks.form.pluginWorkspaceRequired'));
                },
              },
            ]}
          >
            <Select
              allowClear
              showSearch
              disabled={!pluginDeviceId}
              optionFilterProp="label"
              options={pluginWorkspaceOptions}
              placeholder={t('tasks.form.pluginWorkspacePlaceholder')}
            />
          </Form.Item>
        </div>

        <Form.Item
          name="selectedPluginIds"
          label={t('tasks.form.plugins')}
          extra={
            pluginDeviceId
              ? selectablePlugins.length
                ? t('tasks.form.pluginsHelp')
                : t('tasks.form.pluginsEmpty')
              : t('tasks.form.pluginsChooseDevice')
          }
        >
          <Select
            mode="multiple"
            allowClear
            showSearch
            disabled={!pluginDeviceId}
            loading={pluginCatalogLoading}
            optionFilterProp="label"
            maxTagCount="responsive"
            options={pluginOptions}
            placeholder={t('tasks.form.pluginsPlaceholder')}
          />
        </Form.Item>

        <Form.Item
          name="pluginAgentSelection"
          label={t('tasks.form.pluginAgents')}
          extra={
            pluginAgentOptions.length
              ? t('tasks.form.pluginAgentsHelp')
              : t('tasks.form.pluginAgentsEmpty')
          }
        >
          <Select
            allowClear
            showSearch
            disabled={!pluginAgentOptions.length}
            optionFilterProp="label"
            options={pluginAgentOptions}
            placeholder={t('tasks.form.pluginAgentsPlaceholder')}
          />
        </Form.Item>

        {selectedPluginDetails.length ? (
          <Space direction="vertical" size={8} style={{ width: '100%', marginBottom: 16 }}>
            {selectedPluginDetails.map(({ pluginId, plugin }) => (
              <div
                key={pluginId}
                style={{
                  border: '1px solid #e5e7eb',
                  borderRadius: 8,
                  padding: 12,
                  background: '#fafafa',
                }}
              >
                <Space direction="vertical" size={4} style={{ width: '100%' }}>
                  <Space wrap>
                    <Typography.Text strong>
                      {plugin?.display_name || pluginId}
                    </Typography.Text>
                    {plugin ? <Tag color="blue">v{plugin.version}</Tag> : <Tag color="warning">unavailable</Tag>}
                    {(plugin?.component_keys || []).map((componentKey) => (
                      <Tag key={`${pluginId}:${componentKey}`} color="purple">
                        {componentKey}
                      </Tag>
                    ))}
                  </Space>
                  <Typography.Text type="secondary">
                    {plugin?.description || t('tasks.form.pluginUnavailable')}
                  </Typography.Text>
                  {(plugin?.agents || []).length ? (
                    <Space direction="vertical" size={4} style={{ width: '100%' }}>
                      <Typography.Text strong>
                        {t('tasks.form.pluginAgents')}
                      </Typography.Text>
                      {(plugin?.agents || []).map((agent) => (
                        <Space key={`${pluginId}:${agent.agent_id}`} wrap>
                          <Tag color="magenta">@{agent.agent_id}</Tag>
                          <Tag>{agent.base_agent}</Tag>
                          <Tag color="cyan">
                            {t('tasks.form.pluginAgentMaxIterations', {
                              count: agent.max_iterations,
                            })}
                          </Tag>
                          {agent.allowed_tools?.length ? (
                            <Typography.Text type="secondary">
                              {agent.allowed_tools.join(', ')}
                            </Typography.Text>
                          ) : null}
                        </Space>
                      ))}
                    </Space>
                  ) : null}
                  {(plugin?.commands || []).length ? (
                    <Space direction="vertical" size={8} style={{ width: '100%' }}>
                      <Typography.Text strong>
                        {t('tasks.form.pluginCommands')}
                      </Typography.Text>
                      {(plugin?.commands || []).map((command) => {
                        const commandKey = taskPluginCommandKey(
                          pluginId,
                          command.command_id,
                        );
                        const selected = Boolean(pluginCommandSelections[commandKey]);
                        return (
                          <div
                            key={commandKey}
                            style={{
                              border: '1px solid #d8dee9',
                              borderRadius: 6,
                              padding: 10,
                              background: '#fff',
                            }}
                          >
                            <Space direction="vertical" size={6} style={{ width: '100%' }}>
                              <Space wrap>
                                <Form.Item
                                  name={['pluginCommandSelections', commandKey]}
                                  valuePropName="checked"
                                  noStyle
                                >
                                  <Checkbox>{command.display_name}</Checkbox>
                                </Form.Item>
                                <Tag color="purple">/{command.command_id}</Tag>
                                {command.requires_confirmation ? (
                                  <Tag color="orange">
                                    {t('tasks.form.pluginCommandConfirmationRequired')}
                                  </Tag>
                                ) : null}
                              </Space>
                              {command.description ? (
                                <Typography.Text type="secondary">
                                  {command.description}
                                </Typography.Text>
                              ) : null}
                              {selected ? (
                                <Form.Item
                                  name={['pluginCommandArguments', commandKey]}
                                  label={t('tasks.form.pluginCommandArguments')}
                                  extra={
                                    command.requires_confirmation
                                      ? t('tasks.form.pluginCommandApprovalHelp')
                                      : t('tasks.form.pluginCommandArgumentsHelp')
                                  }
                                  rules={[
                                    {
                                      validator: async (_, value) => {
                                        if (
                                          !value ||
                                          new TextEncoder().encode(value.trim()).length <= 16384
                                        ) {
                                          return;
                                        }
                                        throw new Error(
                                          t('tasks.form.pluginCommandArgumentsTooLarge'),
                                        );
                                      },
                                    },
                                  ]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <Input.TextArea
                                    rows={2}
                                    showCount
                                    placeholder={
                                      command.argument_hint ||
                                      t('tasks.form.pluginCommandArgumentsPlaceholder')
                                    }
                                  />
                                </Form.Item>
                              ) : null}
                            </Space>
                          </div>
                        );
                      })}
                    </Space>
                  ) : null}
                </Space>
              </div>
            ))}
          </Space>
        ) : null}

        {browserPluginSelected ? (
          <Alert
            type="success"
            showIcon
            style={{ marginBottom: 16 }}
            message={t('tasks.form.browserPluginReady')}
            description={t('tasks.form.browserPluginHelp')}
          />
        ) : null}

        <Typography.Title level={5} style={{ marginTop: 8 }}>
          {t('tasks.form.schedule')}
        </Typography.Title>

        <Form.Item
          name="scheduleMode"
          label={t('tasks.form.scheduleMode')}
          extra={scheduleModeDescriptions[effectiveScheduleMode]}
        >
          <Select options={scheduleModeOptions} />
        </Form.Item>

        {effectiveScheduleMode !== 'manual' ? (
          <Form.Item
            name="scheduleRunAt"
            label={
              effectiveScheduleMode === 'once' || effectiveScheduleMode === 'contact_async'
                ? t('tasks.form.runAt')
                : t('tasks.form.firstRunAt')
            }
            rules={[
              {
                required: true,
                message:
                  effectiveScheduleMode === 'once' ||
                  effectiveScheduleMode === 'contact_async'
                    ? t('tasks.form.runAtRequired')
                    : t('tasks.form.firstRunAtRequired'),
              },
            ]}
          >
            <Input type="datetime-local" />
          </Form.Item>
        ) : null}

        {effectiveScheduleMode === 'interval' ? (
          <Form.Item
            name="scheduleIntervalSeconds"
            label={t('tasks.form.intervalSeconds')}
            rules={[
              { required: true, message: t('tasks.form.intervalRequired') },
              {
                validator: async (_, value) => {
                  if (value === undefined || value === null || value >= 60) {
                    return;
                  }
                  throw new Error(t('tasks.form.intervalMin'));
                },
              },
            ]}
          >
            <InputNumber style={{ width: '100%' }} min={60} step={60} />
          </Form.Item>
        ) : null}

        <Typography.Title level={5} style={{ marginTop: 8 }}>
          {t('tasks.form.builtinMcp')}
        </Typography.Title>

        <Space style={{ marginBottom: 12 }}>
          <Button onClick={onPreviewPrompt}>{t('tasks.form.previewPrompt')}</Button>
        </Space>

        <Space size="middle" style={{ marginBottom: 16, width: '100%' }} align="start">
          <Form.Item
            name="mcpEnabled"
            label={t('tasks.form.enable')}
            valuePropName="checked"
            style={{ marginBottom: 0 }}
          >
            <Switch />
          </Form.Item>
        </Space>

        <Space size="middle" style={{ width: '100%' }} align="start">
          <Form.Item name="builtinPromptMode" label={t('tasks.form.promptMode')} style={{ flex: 1 }}>
            <Select
              disabled={!mcpEnabled}
              options={[
                { label: 'effective', value: 'effective' },
                { label: 'configured', value: 'configured' },
              ]}
            />
          </Form.Item>
          <Form.Item name="builtinPromptLocale" label={t('mcp.promptLanguage.label')} style={{ width: 180 }}>
            <Select
              disabled={!mcpEnabled}
              options={[
                { label: t('mcp.promptLanguage.zhCN'), value: 'zh-CN' },
                { label: t('mcp.promptLanguage.enUS'), value: 'en-US' },
              ]}
            />
          </Form.Item>
        </Space>

        <Form.Item name="enabledBuiltinKinds" label={t('tasks.form.enabledKinds')}>
          <Checkbox.Group style={{ width: '100%' }}>
            <Space direction="vertical" style={{ width: '100%' }}>
              {mcpOptions.map((option) => (
                <Checkbox
                  key={String(option.value)}
                  value={String(option.value)}
                  disabled={
                    option.disabled ||
                    !mcpEnabled ||
                    (option.value === CODE_MAINTAINER_READ_KIND &&
                      codeMaintainerWriteSelected)
                  }
                >
                  <Space direction="vertical" size={2}>
                    <Typography.Text>{option.label}</Typography.Text>
                    {option.description ? (
                      <Typography.Text type="secondary">{option.description}</Typography.Text>
                    ) : null}
                    {option.useCases.length || option.capabilities.length || option.message ? (
                      <Typography.Text type="secondary">
                        {[...option.useCases, ...option.capabilities].join(' / ')}
                        {option.message ? ` / ${option.message}` : ''}
                      </Typography.Text>
                    ) : null}
                  </Space>
                </Checkbox>
              ))}
            </Space>
          </Checkbox.Group>
        </Form.Item>

        {remoteControllerEffectiveSelected ? (
          <Form.Item name="defaultRemoteServerId" label={t('tasks.form.defaultRemoteServer')}>
            <Select
              allowClear
              disabled={!mcpEnabled}
              options={remoteServerOptions}
              placeholder={t('tasks.form.defaultRemoteServerPlaceholder')}
            />
          </Form.Item>
        ) : null}

        <Form.Item name="externalMcpConfigIds" label={t('tasks.form.externalMcpConfigs')}>
          <Select
            mode="multiple"
            allowClear
            disabled={!mcpEnabled}
            options={externalMcpConfigOptions}
            placeholder={t('tasks.form.externalMcpConfigsPlaceholder')}
          />
        </Form.Item>

        <Typography.Text type="secondary">
          {t('tasks.form.externalMcpConfigsHelp')}
        </Typography.Text>

        {mcpCatalogEntries.length ? (
          <Space direction="vertical" size={4} style={{ width: '100%' }}>
            {mcpCatalogEntries.map((entry) => (
              <Typography.Text
                key={entry.kind}
                type={entry.implemented ? 'secondary' : 'warning'}
              >
                {entry.kind}: {t('tasks.mcpTools', { count: entry.available_tool_names.length })}
                {entry.message ? `, ${entry.message}` : ''}
              </Typography.Text>
            ))}
          </Space>
        ) : null}

        {remoteControllerEffectiveSelected ? (
          <Space
            direction="vertical"
            size={4}
            style={{
              width: '100%',
              padding: 12,
              border: '1px solid #f0f0f0',
              borderRadius: 6,
              background: '#fafafa',
            }}
          >
            <Space wrap>
              <Tag color={enabledRemoteServerCount > 0 ? 'success' : 'warning'}>
                RemoteConnectionController
              </Tag>
              <Typography.Text type="secondary">
                {t('tasks.form.remoteServerCount', {
                  enabled: enabledRemoteServerCount,
                  total: remoteServerTotalCount,
                })}
              </Typography.Text>
            </Space>
            <Typography.Text type="secondary">
              {defaultRemoteServerId
                ? t('tasks.form.defaultRemoteServerBound', {
                    server:
                      remoteServerMap.get(defaultRemoteServerId)?.name ||
                      defaultRemoteServerId,
                  })
                : enabledRemoteServerCount > 0
                  ? t('tasks.form.defaultRemoteServerUnbound')
                  : t('tasks.form.noRemoteServers')}
            </Typography.Text>
            <Space>
              <Button size="small" onClick={onManageServers}>
                {t('tasks.form.manageServers')}
              </Button>
              <Button size="small" onClick={onViewMcpCatalog}>
                {t('tasks.form.viewMcpCatalog')}
              </Button>
            </Space>
          </Space>
        ) : null}
      </Form>
    </Drawer>
  );
}
