// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FormInstance } from 'antd';
import { Form, Input, Modal, Select, Space, Typography } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskRecord } from '../../types';
import type { RunTaskFormValues } from './taskPageUtils';

type SelectOption = {
  label: string;
  value: string;
  disabled?: boolean;
};

type TaskRunModalProps = {
  t: TranslateFn;
  task: TaskRecord | null;
  form: FormInstance<RunTaskFormValues>;
  modelOptions: SelectOption[];
  loading: boolean;
  onClose: () => void;
  onSubmit: (values: RunTaskFormValues) => void;
};

export function TaskRunModal({
  t,
  task,
  form,
  modelOptions,
  loading,
  onClose,
  onSubmit,
}: TaskRunModalProps) {
  return (
    <Modal
      title={task ? t('tasks.run.titleWithName', { title: task.title }) : t('tasks.run.title')}
      open={Boolean(task)}
      onCancel={onClose}
      onOk={() => form.submit()}
      confirmLoading={loading}
      destroyOnClose
    >
      {task ? (
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Space direction="vertical" size={0}>
            <Typography.Text type="secondary">{t('tasks.run.objective')}</Typography.Text>
            <Typography.Paragraph style={{ marginBottom: 0 }}>
              {task.objective}
            </Typography.Paragraph>
          </Space>

          <Form<RunTaskFormValues> layout="vertical" form={form} onFinish={onSubmit}>
            <Form.Item name="model_config_id" label={t('tasks.run.modelConfig')}>
              <Select
                allowClear
                placeholder={t('tasks.run.modelPlaceholder')}
                options={modelOptions}
              />
            </Form.Item>
            <Form.Item name="prompt_override" label="Prompt Override">
              <Input.TextArea rows={5} placeholder={t('tasks.run.promptPlaceholder')} />
            </Form.Item>
          </Form>
        </Space>
      ) : null}
    </Modal>
  );
}

type BatchTaskRunModalProps = {
  t: TranslateFn;
  taskIds: string[];
  tasks: TaskRecord[];
  form: FormInstance<RunTaskFormValues>;
  modelOptions: SelectOption[];
  loading: boolean;
  onClose: () => void;
  onSubmit: (values: RunTaskFormValues) => void;
};

export function BatchTaskRunModal({
  t,
  taskIds,
  tasks,
  form,
  modelOptions,
  loading,
  onClose,
  onSubmit,
}: BatchTaskRunModalProps) {
  return (
    <Modal
      title={
        taskIds.length
          ? t('tasks.batchRun.titleWithCount', { count: taskIds.length })
          : t('tasks.batchRun.title')
      }
      open={Boolean(taskIds.length)}
      onCancel={onClose}
      onOk={() => form.submit()}
      confirmLoading={loading}
      destroyOnClose
    >
      {taskIds.length ? (
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Space direction="vertical" size={0}>
            <Typography.Text type="secondary">{t('tasks.batchRun.tasks')}</Typography.Text>
            <Typography.Paragraph style={{ marginBottom: 0 }}>
              {tasks.length
                ? tasks.map((task) => task.title).join(' / ')
                : t('tasks.batchRun.selectedFallback', { count: taskIds.length })}
            </Typography.Paragraph>
          </Space>

          <Form<RunTaskFormValues> layout="vertical" form={form} onFinish={onSubmit}>
            <Form.Item name="model_config_id" label={t('tasks.batchRun.overrideModel')}>
              <Select
                allowClear
                placeholder={t('tasks.batchRun.overrideModelPlaceholder')}
                options={modelOptions}
              />
            </Form.Item>
            <Form.Item name="prompt_override" label={t('tasks.batchRun.overridePrompt')}>
              <Input.TextArea
                rows={6}
                placeholder={t('tasks.batchRun.overridePromptPlaceholder')}
              />
            </Form.Item>
          </Form>
        </Space>
      ) : null}
    </Modal>
  );
}
