import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Button,
  Checkbox,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  Radio,
  Select,
  Segmented,
  Space,
  Table,
  Tag,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import type {
  TaskSummaryRecord,
  RunSummaryRecord,
  UiPromptRecord,
  UiPromptStatus,
} from '../types';

interface PromptField {
  key: string;
  label: string;
  description?: string;
  placeholder?: string;
  default?: string;
  required?: boolean;
  multiline?: boolean;
  secret?: boolean;
}

interface PromptChoiceOption {
  value: string;
  label?: string;
  description?: string;
}

interface PromptChoice {
  multiple?: boolean;
  options: PromptChoiceOption[];
  default?: unknown;
  min_selections?: number;
  max_selections?: number;
}

const promptColorMap: Record<UiPromptStatus, string> = {
  pending: 'processing',
  submitted: 'success',
  cancelled: 'default',
  timed_out: 'warning',
  failed: 'error',
};

const promptStatusFilterValues: Array<UiPromptStatus | 'all'> = [
  'all',
  'pending',
  'submitted',
  'cancelled',
  'timed_out',
];

export function PromptsPage() {
  const { t } = useI18n();
  const DEFAULT_PAGE_SIZE = 10;
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [messageApi, contextHolder] = message.useMessage();
  const [form] = Form.useForm<Record<string, unknown>>();
  const [promptPage, setPromptPage] = useState(1);
  const [promptPageSize, setPromptPageSize] = useState(DEFAULT_PAGE_SIZE);
  const [taskSearchTerm, setTaskSearchTerm] = useState('');
  const [runSearchTerm, setRunSearchTerm] = useState('');
  const routePromptId = searchParams.get('prompt_id') || undefined;
  const taskFilterId = searchParams.get('task_id') || undefined;
  const runFilterId = searchParams.get('run_id') || undefined;
  const statusFilter = (searchParams.get('status') as UiPromptStatus | 'all' | null) || 'all';
  const promptStatusOptions = useMemo(
    () => promptStatusFilterValues.map((value) => ({
      label: t(`prompts.status.${value}`),
      value,
    })),
    [t],
  );
  const promptStatusLabel = (status: UiPromptStatus) => t(`prompts.status.${status}`);

  const promptsQuery = useQuery({
    queryKey: ['prompts', taskFilterId, runFilterId, statusFilter, promptPage, promptPageSize],
    queryFn: () =>
      api.listPromptsPage({
        taskId: taskFilterId,
        runId: runFilterId,
        status: statusFilter === 'all' ? undefined : statusFilter,
        limit: promptPageSize,
        offset: (promptPage - 1) * promptPageSize,
      }),
  });
  const modelsQuery = useQuery({
    queryKey: ['model-configs'],
    queryFn: api.listModelConfigs,
  });
  const selectedPromptQuery = useQuery({
    queryKey: ['prompt', routePromptId],
    queryFn: () => api.getPrompt(routePromptId!),
    enabled: Boolean(routePromptId),
  });
  const selectedPrompt = useMemo(() => {
    if (!routePromptId) {
      return null;
    }
    return (
      selectedPromptQuery.data ||
      (promptsQuery.data?.items || []).find((prompt) => prompt.id === routePromptId) ||
      null
    );
  }, [promptsQuery.data, routePromptId, selectedPromptQuery.data]);

  useEffect(() => {
    setPromptPage(1);
  }, [taskFilterId, runFilterId, statusFilter]);

  const displayRunIds = useMemo(() => {
    const ids = new Set<string>();
    if (runFilterId) {
      ids.add(runFilterId);
    }
    if (selectedPromptQuery.data?.run_id) {
      ids.add(selectedPromptQuery.data.run_id);
    }
    (promptsQuery.data?.items || []).forEach((prompt) => {
      if (prompt.run_id) {
        ids.add(prompt.run_id);
      }
    });
    return Array.from(ids).sort();
  }, [promptsQuery.data?.items, runFilterId, selectedPromptQuery.data?.run_id]);

  const runSummariesQuery = useQuery({
    queryKey: ['run-summaries', displayRunIds.join(',')],
    queryFn: () => api.listRunSummaries({ ids: displayRunIds }),
    enabled: displayRunIds.length > 0,
  });

  const runSearchQuery = useQuery({
    queryKey: ['run-summary-search', taskFilterId, runSearchTerm],
    queryFn: () =>
      api.listRunSummaries({
        task_id: taskFilterId,
        keyword: runSearchTerm.trim() || undefined,
        limit: 20,
      }),
    enabled: Boolean(taskFilterId || runSearchTerm.trim()),
  });

  const displayTaskIds = useMemo(() => {
    const ids = new Set<string>();
    if (taskFilterId) {
      ids.add(taskFilterId);
    }
    if (selectedPromptQuery.data?.task_id) {
      ids.add(selectedPromptQuery.data.task_id);
    }
    (promptsQuery.data?.items || []).forEach((prompt) => {
      if (prompt.task_id) {
        ids.add(prompt.task_id);
      }
    });
    (runSearchQuery.data || []).forEach((run) => ids.add(run.task_id));
    (runSummariesQuery.data || []).forEach((run) => ids.add(run.task_id));
    return Array.from(ids).sort();
  }, [
    promptsQuery.data?.items,
    runSearchQuery.data,
    runSummariesQuery.data,
    selectedPromptQuery.data?.task_id,
    taskFilterId,
  ]);

  const taskSummariesQuery = useQuery({
    queryKey: ['task-summaries', displayTaskIds.join(',')],
    queryFn: () => api.listTaskSummaries({ ids: displayTaskIds }),
    enabled: displayTaskIds.length > 0,
  });

  const taskSearchQuery = useQuery({
    queryKey: ['task-summary-search', taskSearchTerm],
    queryFn: () =>
      api.listTaskSummaries({
        keyword: taskSearchTerm.trim() || undefined,
        limit: 20,
      }),
    enabled: taskSearchTerm.trim().length > 0,
  });

  const submitPromptMutation = useMutation({
    mutationFn: ({ id, values }: { id: string; values: Record<string, unknown> }) => {
      const fields = selectedPrompt ? extractFields(selectedPrompt) : [];
      const choice = selectedPrompt ? extractChoice(selectedPrompt) : null;
      const payloadValues =
        fields.length > 0
          ? Object.fromEntries(fields.map((field) => [field.key, values[field.key] ?? '']))
          : undefined;
      const selection = choice ? values.selection : undefined;
      return api.submitPrompt(id, {
        values: payloadValues,
        selection,
      });
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['prompts'] }),
        queryClient.invalidateQueries({ queryKey: ['prompt-task-counts'] }),
        queryClient.invalidateQueries({ queryKey: ['runs'] }),
        queryClient.invalidateQueries({ queryKey: ['run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['run-prompts'] }),
        queryClient.invalidateQueries({ queryKey: ['run-events'] }),
        queryClient.invalidateQueries({ queryKey: ['prompt'] }),
      ]);
      messageApi.success(t('prompts.submitted'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const cancelPromptMutation = useMutation({
    mutationFn: (id: string) => api.cancelPrompt(id, {}),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['prompts'] }),
        queryClient.invalidateQueries({ queryKey: ['prompt-task-counts'] }),
        queryClient.invalidateQueries({ queryKey: ['runs'] }),
        queryClient.invalidateQueries({ queryKey: ['run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['run-prompts'] }),
        queryClient.invalidateQueries({ queryKey: ['run-events'] }),
        queryClient.invalidateQueries({ queryKey: ['prompt'] }),
      ]);
      messageApi.success(t('prompts.cancelled'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const taskMap = useMemo(() => {
    const map = new Map<string, TaskSummaryRecord>();
    (taskSummariesQuery.data || []).forEach((task) => map.set(task.id, task));
    return map;
  }, [taskSummariesQuery.data]);

  const runMap = useMemo(() => {
    const map = new Map<string, RunSummaryRecord>();
    (runSummariesQuery.data || []).forEach((run) => map.set(run.id, run));
    return map;
  }, [runSummariesQuery.data]);
  const modelMap = useMemo(() => {
    const map = new Map<string, string>();
    (modelsQuery.data || []).forEach((model) => map.set(model.id, model.name));
    return map;
  }, [modelsQuery.data]);

  useEffect(() => {
    if (!selectedPrompt) {
      form.resetFields();
      return;
    }
    form.setFieldsValue(buildInitialValues(selectedPrompt) as never);
  }, [selectedPrompt, form]);

  const columns: ColumnsType<UiPromptRecord> = [
    {
      title: t('prompts.column.promptId'),
      dataIndex: 'id',
      width: 180,
      render: (value: string) => <Typography.Text code>{value.slice(0, 12)}</Typography.Text>,
    },
    {
      title: t('prompts.column.title'),
      dataIndex: 'title',
      render: (_, record) => record.title || record.message || record.kind,
    },
    {
      title: t('prompts.column.task'),
      dataIndex: 'task_id',
      render: (value?: string | null) =>
        value ? (
          <Button
            type="link"
            size="small"
            onClick={() => navigate(`/tasks?task_id=${encodeURIComponent(value)}`)}
          >
            {taskMap.get(value)?.title || value}
          </Button>
        ) : (
          '-'
        ),
    },
    {
      title: t('prompts.column.run'),
      dataIndex: 'run_id',
      width: 180,
      render: (value?: string | null) =>
        value ? (
          <Button
            type="link"
            size="small"
            onClick={() => navigate(`/runs?run_id=${encodeURIComponent(value)}`)}
          >
            <Typography.Text code>{value.slice(0, 12)}</Typography.Text>
          </Button>
        ) : (
          '-'
        ),
    },
    {
      title: t('common.status'),
      dataIndex: 'status',
      width: 120,
      render: (status: UiPromptStatus) => (
        <Tag color={promptColorMap[status]}>{promptStatusLabel(status)}</Tag>
      ),
    },
    {
      title: t('common.updatedAt'),
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 120,
      render: (_, record) => (
        <Button size="small" onClick={() => openPromptDrawer(record.id)}>
          {record.status === 'pending' ? t('prompts.action.handle') : t('common.view')}
        </Button>
      ),
    },
  ];

  const selectedTask = selectedPrompt?.task_id ? taskMap.get(selectedPrompt.task_id) : null;
  const selectedRun = selectedPrompt?.run_id ? runMap.get(selectedPrompt.run_id) : null;
  const selectedFields = selectedPrompt ? extractFields(selectedPrompt) : [];
  const selectedChoice = selectedPrompt ? extractChoice(selectedPrompt) : null;

  function updatePromptSearchParam(key: string, value?: string) {
    const next = new URLSearchParams(searchParams);
    if (value) {
      next.set(key, value);
    } else {
      next.delete(key);
    }
    setSearchParams(next);
  }

  function openPromptDrawer(promptId: string) {
    updatePromptSearchParam('prompt_id', promptId);
  }

  function closePromptDrawer() {
    updatePromptSearchParam('prompt_id', undefined);
  }

  return (
    <>
      {contextHolder}
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }}>
          <Space direction="vertical" size={0}>
            <Typography.Title level={3} style={{ margin: 0 }}>
              {t('prompts.title')}
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('prompts.subtitle')}
            </Typography.Text>
          </Space>
          <Space>
            <Select
              allowClear
              showSearch
              filterOption={false}
              placeholder={t('prompts.taskFilter')}
              style={{ width: 220 }}
              value={taskFilterId}
              options={Array.from(
                new Map(
                  [...(taskSummariesQuery.data || []), ...(taskSearchQuery.data || [])].map((task) => [
                    task.id,
                    {
                      label: task.title,
                      value: task.id,
                    },
                  ]),
                ).values(),
              )}
              onSearch={setTaskSearchTerm}
              onChange={(value: string | undefined) => updatePromptSearchParam('task_id', value)}
            />
            <Select
              allowClear
              showSearch
              filterOption={false}
              placeholder={t('prompts.runFilter')}
              style={{ width: 220 }}
              value={runFilterId}
              options={Array.from(
                new Map(
                  [...(runSummariesQuery.data || []), ...(runSearchQuery.data || [])].map((run) => [
                    run.id,
                    {
                      label: `${run.id.slice(0, 12)} / ${
                        taskMap.get(run.task_id)?.title || run.task_id
                      }`,
                      value: run.id,
                    },
                  ]),
                ).values(),
              )}
              onSearch={setRunSearchTerm}
              onChange={(value: string | undefined) => updatePromptSearchParam('run_id', value)}
            />
            <Segmented
              value={statusFilter}
              onChange={(value) =>
                updatePromptSearchParam(
                  'status',
                  value === 'all' ? undefined : (value as UiPromptStatus),
                )
              }
              options={[
                ...promptStatusOptions,
              ]}
            />
            <Button
              onClick={() => {
                const next = new URLSearchParams(searchParams);
                next.delete('task_id');
                next.delete('run_id');
                next.delete('status');
                setSearchParams(next);
              }}
            >
              {t('common.clearFilters')}
            </Button>
            <Button onClick={() => promptsQuery.refetch()}>{t('common.refresh')}</Button>
          </Space>
        </Space>

        <Table<UiPromptRecord>
          rowKey="id"
          loading={promptsQuery.isLoading}
          columns={columns}
          dataSource={promptsQuery.data?.items || []}
          pagination={{
            current: promptPage,
            pageSize: promptPageSize,
            total: promptsQuery.data?.total || 0,
            showSizeChanger: true,
            onChange: (page, pageSize) => {
              setPromptPage(page);
              setPromptPageSize(pageSize);
            },
          }}
          locale={{
            emptyText: (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={t('prompts.empty')}
              />
            ),
          }}
        />
      </Space>

      <Drawer
        title={t('prompts.detail.title')}
        open={Boolean(routePromptId)}
        width={760}
        onClose={closePromptDrawer}
      >
        {selectedPrompt ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Space wrap>
              {selectedPrompt.task_id ? (
                <Button
                  onClick={() =>
                    navigate(`/tasks?task_id=${encodeURIComponent(selectedPrompt.task_id!)}`)
                  }
                >
                  {t('prompts.detail.openTask')}
                </Button>
              ) : null}
              {selectedPrompt.run_id ? (
                <Button
                  onClick={() =>
                    navigate(`/runs?run_id=${encodeURIComponent(selectedPrompt.run_id!)}`)
                  }
                >
                  {t('prompts.detail.openRun')}
                </Button>
              ) : null}
            </Space>

            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label={t('prompts.column.promptId')}>{selectedPrompt.id}</Descriptions.Item>
              <Descriptions.Item label={t('prompts.column.task')}>
                {selectedTask?.title || selectedPrompt.task_id || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('prompts.column.run')}>
                {selectedRun?.id || selectedPrompt.run_id || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('prompts.detail.modelConfig')}>
                {selectedRun?.model_config_id ? (
                  <Button
                    type="link"
                    size="small"
                    style={{ paddingInline: 0 }}
                    onClick={() =>
                      navigate(
                        `/models?model_id=${encodeURIComponent(selectedRun.model_config_id)}`,
                      )
                    }
                  >
                    {modelMap.get(selectedRun.model_config_id) || selectedRun.model_config_id}
                  </Button>
                ) : (
                  '-'
                )}
              </Descriptions.Item>
              <Descriptions.Item label={t('common.status')}>
                <Tag color={promptColorMap[selectedPrompt.status]}>
                  {promptStatusLabel(selectedPrompt.status)}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label={t('prompts.detail.kind')}>{selectedPrompt.kind}</Descriptions.Item>
              <Descriptions.Item label={t('prompts.column.title')}>
                {selectedPrompt.title || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('prompts.detail.message')}>
                {selectedPrompt.message || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('prompts.detail.expiresAt')}>
                {selectedPrompt.expires_at
                  ? dayjs(selectedPrompt.expires_at).format('YYYY-MM-DD HH:mm:ss')
                  : '-'}
              </Descriptions.Item>
            </Descriptions>

            {selectedPrompt.status === 'pending' ? (
              <Form
                form={form}
                layout="vertical"
                onFinish={(values) =>
                  submitPromptMutation.mutate({
                    id: selectedPrompt.id,
                    values,
                  })
                }
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
                                return Promise.reject(new Error(t('prompts.detail.minSelections', { min })));
                              }
                              if (items.length > max) {
                                return Promise.reject(new Error(t('prompts.detail.maxSelections', { max })));
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
                  <Button
                    type="primary"
                    htmlType="submit"
                    loading={submitPromptMutation.isPending}
                  >
                    {t('common.submit')}
                  </Button>
                  <Button
                    disabled={!selectedPrompt.allow_cancel}
                    loading={cancelPromptMutation.isPending}
                    onClick={() => cancelPromptMutation.mutate(selectedPrompt.id)}
                  >
                    {t('prompts.detail.cancelPrompt')}
                  </Button>
                </Space>
              </Form>
            ) : (
              <>
                <Typography.Title level={5}>{t('prompts.detail.response')}</Typography.Title>
                {selectedPrompt.response ? (
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
                    {JSON.stringify(selectedPrompt.response, null, 2)}
                  </Typography.Paragraph>
                ) : (
                  <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
                )}
              </>
            )}

            <div>
              <Typography.Title level={5}>{t('prompts.detail.rawPayload')}</Typography.Title>
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
                {JSON.stringify(selectedPrompt.payload, null, 2)}
              </Typography.Paragraph>
            </div>
          </Space>
        ) : null}
      </Drawer>
    </>
  );
}

function buildInitialValues(prompt: UiPromptRecord): Record<string, unknown> {
  const values: Record<string, unknown> = {};
  extractFields(prompt).forEach((field) => {
    values[field.key] = field.default ?? '';
  });

  const choice = extractChoice(prompt);
  if (choice) {
    values.selection =
      prompt.response?.selection ??
      choice.default ??
      (choice.multiple ? [] : '');
  }

  const responseValues = asRecord(prompt.response?.values);
  if (responseValues) {
    Object.assign(values, responseValues);
  }

  return values;
}

function extractFields(prompt: UiPromptRecord): PromptField[] {
  const payload = asRecord(prompt.payload);
  const rawFields = Array.isArray(payload?.fields) ? payload.fields : [];
  return rawFields
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => Boolean(item))
    .map((field) => ({
      key: asString(field.key) || 'field',
      label: asString(field.label) || asString(field.key) || 'field',
      description: asOptionalString(field.description),
      placeholder: asOptionalString(field.placeholder),
      default: asOptionalString(field.default) ?? '',
      required: Boolean(field.required),
      multiline: Boolean(field.multiline),
      secret: Boolean(field.secret),
    }));
}

function extractChoice(prompt: UiPromptRecord): PromptChoice | null {
  const payload = asRecord(prompt.payload);
  const choice = asRecord(payload?.choice);
  if (!choice || !Array.isArray(choice.options) || choice.options.length === 0) {
    return null;
  }

  return {
    multiple: Boolean(choice.multiple),
    default: choice.default,
    min_selections: asNumber(choice.min_selections),
    max_selections: asNumber(choice.max_selections),
    options: choice.options
      .map((item) => asRecord(item))
      .filter((item): item is Record<string, unknown> => Boolean(item))
      .map((option) => ({
        value: asString(option.value),
        label: asOptionalString(option.label),
        description: asOptionalString(option.description),
      }))
      .filter((option) => option.value),
  };
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function asString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function asOptionalString(value: unknown): string | undefined {
  const text = asString(value).trim();
  return text ? text : undefined;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}
