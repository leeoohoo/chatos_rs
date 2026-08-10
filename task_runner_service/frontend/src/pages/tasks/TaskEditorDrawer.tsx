// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import type { FormInstance } from 'antd';
import {
  Alert,
  Button,
  Descriptions,
  Drawer,
  Form,
  Input,
  InputNumber,
  Select,
  Space,
  Tag,
  Typography,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskRecord, TaskScheduleMode } from '../../types';
import {
  scheduleModeDescriptionKeys,
  scheduleModeLabelKeys,
  taskProfileColorMap,
  taskProfileLabel,
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
  onClose,
  onSubmit,
}: TaskEditorDrawerProps) {
  const scheduleMode = Form.useWatch('scheduleMode', form);
  const effectiveScheduleMode = scheduleMode ?? 'manual';
  const scheduleModeLabels = useMemo(
    () => Object.fromEntries(
      (['manual', 'once', 'interval', 'contact_async'] as TaskScheduleMode[]).map(
        (value) => [value, t(scheduleModeLabelKeys[value])],
      ),
    ) as Record<TaskScheduleMode, string>,
    [t],
  );
  const scheduleModeDescriptions = useMemo(
    () => Object.fromEntries(
      (['manual', 'once', 'interval', 'contact_async'] as TaskScheduleMode[]).map(
        (value) => [value, t(scheduleModeDescriptionKeys[value])],
      ),
    ) as Record<TaskScheduleMode, string>,
    [t],
  );
  const scheduleModeOptions = useMemo(
    () => (['manual', 'once', 'interval', 'contact_async'] as TaskScheduleMode[]).map(
      (value) => ({
        label: scheduleModeLabels[value],
        value,
        disabled: value === 'contact_async',
      }),
    ),
    [scheduleModeLabels],
  );
  const taskStatusOptions = useMemo(
    () => taskStatusValues.map((value) => ({
      label: t(`tasks.status.${value}`),
      value,
    })),
    [t],
  );
  const projectLabel = editingTask
    ? projectOptions.find((option) => option.value === editingTask.project_id)?.label
      || editingTask.project_id
    : '-';

  return (
    <Drawer
      className="task-editor-drawer"
      title={t('tasks.drawer.edit')}
      open={open}
      width="min(1120px, calc(100vw - 32px))"
      destroyOnClose
      onClose={onClose}
      extra={(
        <Space>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            type="primary"
            loading={saving}
            disabled={!editingTask}
            onClick={() => form.submit()}
          >
            {t('common.save')}
          </Button>
        </Space>
      )}
    >
      {editingTask ? (
        <Form<TaskFormValues> layout="vertical" form={form} onFinish={onSubmit}>
          <Alert
            type="info"
            showIcon
            style={{ marginBottom: 16 }}
            message={t('tasks.form.mcpProgramManaged')}
            description={t('tasks.form.mcpProgramManagedHelp')}
          />

          <Descriptions bordered column={1} size="small" style={{ marginBottom: 20 }}>
            <Descriptions.Item label={t('tasks.form.project')}>
              {projectLabel}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.taskProfile')}>
              <Tag color={taskProfileColorMap[editingTask.task_profile] || 'default'}>
                {taskProfileLabel(
                  editingTask.task_profile,
                  t,
                  editingTask.mcp_config.requires_execution,
                )}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.form.mcpSelectedCapabilities')}>
              <Space wrap>
                {editingTask.mcp_config.enabled_builtin_kinds.length
                  ? editingTask.mcp_config.enabled_builtin_kinds.map((kind) => (
                    <Tag key={kind} color="blue">{kind}</Tag>
                  ))
                  : t('common.noData')}
                {editingTask.mcp_config.external_mcp_config_ids.map((id) => (
                  <Tag key={id} color="cyan">{id}</Tag>
                ))}
              </Space>
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.plugins')}>
              {editingTask.plugin_config.selected_plugins.length ? (
                <Space direction="vertical" size="small" style={{ width: '100%' }}>
                  {editingTask.plugin_config.selected_plugins.map((plugin) => (
                    <Space
                      key={plugin.plugin_id}
                      direction="vertical"
                      size={0}
                      style={{ width: '100%' }}
                    >
                      <Tag color="purple">{plugin.plugin_id}</Tag>
                      {plugin.selected_command_ids.length ? (
                        <Typography.Text type="secondary">
                          {t('tasks.detail.pluginCommandList', {
                            commands: plugin.selected_command_ids.map((commandId) => (
                              `/${commandId}`
                            )).join(', '),
                          })}
                        </Typography.Text>
                      ) : null}
                    </Space>
                  ))}
                </Space>
              ) : t('tasks.detail.pluginsPendingRun')}
            </Descriptions.Item>
          </Descriptions>

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
              name="priority"
              label={t('tasks.column.priority')}
              style={{ width: 140 }}
            >
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
              rules={[{
                required: true,
                message:
                  effectiveScheduleMode === 'once'
                  || effectiveScheduleMode === 'contact_async'
                    ? t('tasks.form.runAtRequired')
                    : t('tasks.form.firstRunAtRequired'),
              }]}
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
        </Form>
      ) : null}
    </Drawer>
  );
}
