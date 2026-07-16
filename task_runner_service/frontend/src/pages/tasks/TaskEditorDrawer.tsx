// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo } from 'react';
import type { FormInstance } from 'antd';
import {
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
  SelectableTaskSkill,
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
  selectableSkills?: SelectableTaskSkill[];
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
  selectableSkills = [],
  onClose,
  onSubmit,
  onPreviewPrompt,
  onManageServers,
  onViewMcpCatalog,
}: TaskEditorDrawerProps) {
  const mcpEnabled = Form.useWatch('mcpEnabled', form);
  const selectedProjectId = Form.useWatch('projectId', form);
  const enabledBuiltinKinds = Form.useWatch('enabledBuiltinKinds', form) || [];
  const defaultRemoteServerId = Form.useWatch('defaultRemoteServerId', form);
  const scheduleMode = Form.useWatch('scheduleMode', form);
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
  const skillOptions = useMemo(
    () =>
      selectableSkills.map((skill) => ({
        label: `${skill.display_name} (${skill.entrypoint_kind || 'local'})`,
        value: skill.id,
        description: skill.description,
        platform: skill.platform,
        version: skill.version,
      })),
    [selectableSkills],
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
          {t('tasks.form.skills')}
        </Typography.Title>

        <Form.Item
          name="selectedSkillIds"
          label={t('tasks.form.skills')}
          extra={
            skillOptions.length
              ? t('tasks.form.skillsHelp')
              : t('tasks.form.skillsEmpty')
          }
        >
          <Select
            mode="multiple"
            allowClear
            showSearch
            optionFilterProp="label"
            maxTagCount="responsive"
            options={skillOptions}
            placeholder={t('tasks.form.skillsPlaceholder')}
          />
        </Form.Item>

        {selectableSkills.length ? (
          <Space direction="vertical" size={4} style={{ width: '100%', marginBottom: 16 }}>
            {selectableSkills.map((skill) => (
              <Typography.Text key={skill.id} type="secondary">
                {skill.display_name}: {skill.description || skill.name}
                {skill.platform ? ` / ${skill.platform}` : ''}
                {skill.version ? ` / v${skill.version}` : ''}
              </Typography.Text>
            ))}
          </Space>
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
