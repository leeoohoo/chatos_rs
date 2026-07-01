// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import type { MessageInstance } from 'antd/es/message/interface';

import { api } from '../../api/client';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type { CreateModelConfigPayload, ModelConfigTestResponse } from '../../types';

type UseModelMutationsParams = {
  t: TranslateFn;
  messageApi: MessageInstance;
  onModelSaved: () => void;
  onTestResult: (result: ModelConfigTestResponse) => void;
};

export function useModelMutations({
  t,
  messageApi,
  onModelSaved,
  onTestResult,
}: UseModelMutationsParams) {
  const queryClient = useQueryClient();

  const invalidateModelQueries = useCallback(async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['model-configs'] }),
      queryClient.invalidateQueries({ queryKey: ['model-config-usage'] }),
    ]);
  }, [queryClient]);

  const createModelMutation = useMutation({
    mutationFn: api.createModelConfig,
    onSuccess: async () => {
      await invalidateModelQueries();
      messageApi.success(t('models.created'));
      onModelSaved();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const updateModelMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: Partial<CreateModelConfigPayload> }) =>
      api.updateModelConfig(id, payload),
    onSuccess: async () => {
      await invalidateModelQueries();
      messageApi.success(t('models.updated'));
      onModelSaved();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const deleteModelMutation = useMutation({
    mutationFn: api.deleteModelConfig,
    onSuccess: async () => {
      await Promise.all([
        invalidateModelQueries(),
        queryClient.invalidateQueries({ queryKey: ['tasks'] }),
        queryClient.invalidateQueries({ queryKey: ['task-index'] }),
      ]);
      messageApi.success(t('models.deleted'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const testModelMutation = useMutation({
    mutationFn: (id: string) => api.testModelConfig(id, {}),
    onSuccess: (result) => {
      onTestResult(result);
      if (result.ok) {
        messageApi.success(t('models.testSuccess'));
      } else {
        messageApi.warning(t('models.testFailed'));
      }
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  return {
    createModelMutation,
    updateModelMutation,
    deleteModelMutation,
    testModelMutation,
  };
}
