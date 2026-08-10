// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
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
  SelectableTaskPlugin,
  TaskProjectRuntimeEnvironmentResponse,
  TaskRecord,
  TaskScheduleMode,
} from '../../types';
import {
  scheduleModeDescriptionKeys,
  scheduleModeLabelKeys,
  taskProfileLabel,
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
  selectablePlugins?: SelectableTaskPlugin[];
  pluginCatalogLoading?: boolean;
  runtimeEnvironment?: TaskProjectRuntimeEnvironmentResponse;
  runtimeEnvironmentLoading?: boolean;
  runtimeEnvironmentUnavailable?: boolean;
  onClose: () => void;
  onSubmit: (values: TaskFormValues) => void;
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
  selectablePlugins = [],
  pluginCatalogLoading = false,
  runtimeEnvironment,
  runtimeEnvironmentLoading = false,
  runtimeEnvironmentUnavailable = false,
  onClose,
  onSubmit,
}: TaskEditorDrawerProps) {
  const taskProfile = Form.useWatch('taskProfile', form);
  const selectedProjectId = Form.useWatch('projectId', form);
  const requiresExecution = Form.useWatch('requiresExecution', form);
  const scheduleMode = Form.useWatch('scheduleMode', form);
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
  const pluginOptions = useMemo(
    () =>
      selectablePlugins.map((plugin) => ({
        label: `${plugin.display_name} / v${plugin.version} / ${plugin.execution_type}`,
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
  const browserPluginSelected = selectedPluginDetails.some(({ plugin }) =>
    Boolean(
      plugin &&
        (plugin.plugin_key.toLowerCase().includes('browser') ||
          plugin.component_keys.includes('browser-tools')),
    ),
  );
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

        <Form.Item
          name="selectedPluginIds"
          label={t('tasks.form.plugins')}
          extra={selectablePlugins.length
            ? t('tasks.form.pluginsHelp')
            : t('tasks.form.pluginsEmpty')}
        >
          <Select
            mode="multiple"
            allowClear
            showSearch
            loading={pluginCatalogLoading}
            optionFilterProp="label"
            maxTagCount="responsive"
            options={pluginOptions}
            placeholder={t('tasks.form.pluginsPlaceholder')}
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
        <Alert
          type="info"
          showIcon
          message={t('tasks.form.mcpProgramManaged')}
          description={t('tasks.form.mcpProgramManagedHelp')}
        />
      </Form>
    </Drawer>
  );
}
