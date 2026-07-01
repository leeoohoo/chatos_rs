// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Form,
  Space,
  message,
} from 'antd';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import type { AskUserPromptStatus } from '../types';
import { PromptDetailDrawer } from './prompts/PromptDetailDrawer';
import {
  buildInitialValues,
  extractChoice,
  extractFields,
} from './prompts/promptDetailUtils';
import { PromptListTable } from './prompts/PromptListTable';
import { PromptListToolbar } from './prompts/PromptListToolbar';
import { type PromptStatusFilter } from './prompts/promptPageUtils';
import { usePromptsPageData } from './prompts/usePromptsPageData';

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
  const statusFilter = (searchParams.get('status') as PromptStatusFilter | null) || 'all';
  const promptStatusLabel = (status: AskUserPromptStatus) => t(`prompts.status.${status}`);
  const {
    promptStatusOptions,
    promptsQuery,
    selectedPrompt,
    taskMap,
    runMap,
    modelMap,
    taskOptions,
    runOptions,
  } = usePromptsPageData({
    t,
    routePromptId,
    taskFilterId,
    runFilterId,
    statusFilter,
    promptPage,
    promptPageSize,
    taskSearchTerm,
    runSearchTerm,
  });

  useEffect(() => {
    setPromptPage(1);
  }, [taskFilterId, runFilterId, statusFilter]);

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

  useEffect(() => {
    if (!selectedPrompt) {
      form.resetFields();
      return;
    }
    form.setFieldsValue(buildInitialValues(selectedPrompt) as never);
  }, [selectedPrompt, form]);

  const selectedTask = selectedPrompt?.task_id
    ? taskMap.get(selectedPrompt.task_id) ?? null
    : null;
  const selectedRun = selectedPrompt?.run_id
    ? runMap.get(selectedPrompt.run_id) ?? null
    : null;

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
        <PromptListToolbar
          t={t}
          taskFilterId={taskFilterId}
          runFilterId={runFilterId}
          statusFilter={statusFilter}
          taskOptions={taskOptions}
          runOptions={runOptions}
          promptStatusOptions={promptStatusOptions}
          onTaskSearch={setTaskSearchTerm}
          onRunSearch={setRunSearchTerm}
          onFilterChange={updatePromptSearchParam}
          onClearFilters={() => {
            const next = new URLSearchParams(searchParams);
            next.delete('task_id');
            next.delete('run_id');
            next.delete('status');
            setSearchParams(next);
          }}
          onRefresh={() => promptsQuery.refetch()}
        />

        <PromptListTable
          t={t}
          prompts={promptsQuery.data?.items || []}
          loading={promptsQuery.isLoading}
          currentPage={promptPage}
          pageSize={promptPageSize}
          total={promptsQuery.data?.total || 0}
          taskMap={taskMap}
          promptStatusLabel={promptStatusLabel}
          onOpenTask={(taskId) => navigate(`/tasks?task_id=${encodeURIComponent(taskId)}`)}
          onOpenRun={(runId) => navigate(`/runs?run_id=${encodeURIComponent(runId)}`)}
          onOpenPrompt={openPromptDrawer}
          onPageChange={(page, pageSize) => {
            setPromptPage(page);
            setPromptPageSize(pageSize);
          }}
        />
      </Space>

      <PromptDetailDrawer
        t={t}
        open={Boolean(routePromptId)}
        prompt={selectedPrompt}
        selectedTask={selectedTask}
        selectedRun={selectedRun}
        modelMap={modelMap}
        form={form}
        submitting={submitPromptMutation.isPending}
        canceling={cancelPromptMutation.isPending}
        promptStatusLabel={promptStatusLabel}
        onClose={closePromptDrawer}
        onOpenTask={(taskId) => navigate(`/tasks?task_id=${encodeURIComponent(taskId)}`)}
        onOpenRun={(runId) => navigate(`/runs?run_id=${encodeURIComponent(runId)}`)}
        onOpenModel={(modelId) => navigate(`/models?model_id=${encodeURIComponent(modelId)}`)}
        onSubmit={(id, values) => submitPromptMutation.mutate({ id, values })}
        onCancelPrompt={(id) => cancelPromptMutation.mutate(id)}
      />
    </>
  );
}
