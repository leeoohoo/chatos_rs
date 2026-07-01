// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FormInstance } from 'antd';
import {
  Button,
  Checkbox,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  Radio,
  Space,
  Tag,
  Typography,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  RunSummaryRecord,
  TaskSummaryRecord,
  AskUserPromptRecord,
  AskUserPromptStatus,
} from '../../types';
import {
  extractChoice,
  extractFields,
} from './promptDetailUtils';
import { promptColorMap } from './promptPageUtils';

type PromptDetailDrawerProps = {
  t: TranslateFn;
  open: boolean;
  prompt: AskUserPromptRecord | null;
  selectedTask: TaskSummaryRecord | null;
  selectedRun: RunSummaryRecord | null;
  modelMap: Map<string, string>;
  form: FormInstance<Record<string, unknown>>;
  submitting: boolean;
  canceling: boolean;
  promptStatusLabel: (status: AskUserPromptStatus) => string;
  onClose: () => void;
  onOpenTask: (taskId: string) => void;
  onOpenRun: (runId: string) => void;
  onOpenModel: (modelId: string) => void;
  onSubmit: (promptId: string, values: Record<string, unknown>) => void;
  onCancelPrompt: (promptId: string) => void;
};

export function PromptDetailDrawer({
  t,
  open,
  prompt,
  selectedTask,
  selectedRun,
  modelMap,
  form,
  submitting,
  canceling,
  promptStatusLabel,
  onClose,
  onOpenTask,
  onOpenRun,
  onOpenModel,
  onSubmit,
  onCancelPrompt,
}: PromptDetailDrawerProps) {
  const selectedFields = prompt ? extractFields(prompt) : [];
  const selectedChoice = prompt ? extractChoice(prompt) : null;

  return (
    <Drawer
      title={t('prompts.detail.title')}
      open={open}
      width={760}
      onClose={onClose}
    >
      {prompt ? (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Space wrap>
            {prompt.task_id ? (
              <Button onClick={() => onOpenTask(prompt.task_id!)}>
                {t('prompts.detail.openTask')}
              </Button>
            ) : null}
            {prompt.run_id ? (
              <Button onClick={() => onOpenRun(prompt.run_id!)}>
                {t('prompts.detail.openRun')}
              </Button>
            ) : null}
          </Space>

          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label={t('prompts.column.promptId')}>{prompt.id}</Descriptions.Item>
            <Descriptions.Item label={t('prompts.column.task')}>
              {selectedTask?.title || prompt.task_id || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('prompts.column.run')}>
              {selectedRun?.id || prompt.run_id || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('prompts.detail.modelConfig')}>
              {selectedRun?.model_config_id ? (
                <Button
                  type="link"
                  size="small"
                  style={{ paddingInline: 0 }}
                  onClick={() => onOpenModel(selectedRun.model_config_id)}
                >
                  {modelMap.get(selectedRun.model_config_id) || selectedRun.model_config_id}
                </Button>
              ) : (
                '-'
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t('common.status')}>
              <Tag color={promptColorMap[prompt.status]}>
                {promptStatusLabel(prompt.status)}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label={t('prompts.detail.kind')}>{prompt.kind}</Descriptions.Item>
            <Descriptions.Item label={t('prompts.column.title')}>
              {prompt.title || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('prompts.detail.message')}>
              {prompt.message || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('prompts.detail.expiresAt')}>
              {prompt.expires_at
                ? dayjs(prompt.expires_at).format('YYYY-MM-DD HH:mm:ss')
                : '-'}
            </Descriptions.Item>
          </Descriptions>

          {prompt.status === 'pending' ? (
            <Form
              form={form}
              layout="vertical"
              onFinish={(values) => onSubmit(prompt.id, values)}
            >
              {selectedFields.length ? (
                <>
                  <Typography.Title level={5}>{t('prompts.detail.inputFields')}</Typography.Title>
                  {selectedFields.map((field) => (
                    <Form.Item
                      key={field.key}
                      name={field.key}
                      label={field.label || field.key}
                      extra={field.description || undefined}
                      rules={
                        field.required
                          ? [{
                              required: true,
                              message: t('prompts.detail.fieldRequired', {
                                field: field.label || field.key,
                              }),
                            }]
                          : undefined
                      }
                    >
                      {field.secret ? (
                        <Input.Password placeholder={field.placeholder} />
                      ) : field.multiline ? (
                        <Input.TextArea rows={4} placeholder={field.placeholder} />
                      ) : (
                        <Input placeholder={field.placeholder} />
                      )}
                    </Form.Item>
                  ))}
                </>
              ) : null}

              {selectedChoice ? (
                <>
                  <Typography.Title level={5}>{t('prompts.detail.choices')}</Typography.Title>
                  <Form.Item
                    name="selection"
                    rules={[
                      {
                        validator: (_, value) => {
                          if (selectedChoice.multiple) {
                            const items = Array.isArray(value) ? value : [];
                            const min = selectedChoice.min_selections ?? 0;
                            const max =
                              selectedChoice.max_selections ?? selectedChoice.options.length;
                            if (items.length < min) {
                              return Promise.reject(
                                new Error(t('prompts.detail.minSelections', { min })),
                              );
                            }
                            if (items.length > max) {
                              return Promise.reject(
                                new Error(t('prompts.detail.maxSelections', { max })),
                              );
                            }
                            return Promise.resolve();
                          }
                          if ((selectedChoice.min_selections ?? 0) > 0 && !value) {
                            return Promise.reject(new Error(t('prompts.detail.chooseOne')));
                          }
                          return Promise.resolve();
                        },
                      },
                    ]}
                  >
                    {selectedChoice.multiple ? (
                      <Checkbox.Group style={{ width: '100%' }}>
                        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                          {selectedChoice.options.map((option) => (
                            <Checkbox key={option.value} value={option.value}>
                              <Space direction="vertical" size={0}>
                                <Typography.Text>{option.label || option.value}</Typography.Text>
                                {option.description ? (
                                  <Typography.Text type="secondary">
                                    {option.description}
                                  </Typography.Text>
                                ) : null}
                              </Space>
                            </Checkbox>
                          ))}
                        </Space>
                      </Checkbox.Group>
                    ) : (
                      <Radio.Group style={{ width: '100%' }}>
                        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                          {selectedChoice.options.map((option) => (
                            <Radio key={option.value} value={option.value}>
                              <Space direction="vertical" size={0}>
                                <Typography.Text>{option.label || option.value}</Typography.Text>
                                {option.description ? (
                                  <Typography.Text type="secondary">
                                    {option.description}
                                  </Typography.Text>
                                ) : null}
                              </Space>
                            </Radio>
                          ))}
                        </Space>
                      </Radio.Group>
                    )}
                  </Form.Item>
                </>
              ) : null}

              <Space>
                <Button type="primary" htmlType="submit" loading={submitting}>
                  {t('common.submit')}
                </Button>
                <Button
                  disabled={!prompt.allow_cancel}
                  loading={canceling}
                  onClick={() => onCancelPrompt(prompt.id)}
                >
                  {t('prompts.detail.cancelPrompt')}
                </Button>
              </Space>
            </Form>
          ) : (
            <>
              <Typography.Title level={5}>{t('prompts.detail.response')}</Typography.Title>
              {prompt.response ? (
                <JsonParagraph value={prompt.response} />
              ) : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
              )}
            </>
          )}

          <div>
            <Typography.Title level={5}>{t('prompts.detail.rawPayload')}</Typography.Title>
            <JsonParagraph value={prompt.payload} />
          </div>
        </Space>
      ) : null}
    </Drawer>
  );
}

function JsonParagraph({ value }: { value: unknown }) {
  return (
    <Typography.Paragraph
      style={{
        background: '#fafafa',
        padding: 12,
        borderRadius: 6,
        marginBottom: 0,
        whiteSpace: 'pre-wrap',
        fontFamily: 'monospace',
      }}
    >
      {JSON.stringify(value, null, 2)}
    </Typography.Paragraph>
  );
}
