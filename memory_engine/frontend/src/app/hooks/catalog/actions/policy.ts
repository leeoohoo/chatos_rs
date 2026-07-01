// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { PolicyFormValues } from '../../../types';
import { api } from '../../../../api';
import { buildPolicyPayload } from '../../../utils';

import type { CatalogActionsContext, CatalogPolicyActions } from './types';

export function buildCatalogPolicyActions(
  context: CatalogActionsContext,
): CatalogPolicyActions {
  const { message, controls, loaders } = context;

  const handleSavePolicy = async (jobType: string, values: PolicyFormValues) => {
    controls.setSavingPolicyJobType(jobType);
    try {
      await api.updateJobPolicy(jobType, buildPolicyPayload(values));
      message.success(`已保存任务策略：${jobType}`);
      try {
        await loaders.loadPolicies();
      } catch (error) {
        message.error(`保存任务策略成功，但刷新策略列表失败：${String(error)}`);
      }
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      message.error(`保存任务策略失败：${text}`);
    } finally {
      controls.setSavingPolicyJobType(null);
    }
  };

  const handleGeneratePolicyPrompt = async (
    jobType: string,
    promptField: 'summary_prompt' | 'rollup_summary_prompt',
    userInput: string,
  ) => {
    controls.setGeneratingPolicyJobType(jobType);
    try {
      const generated = await api.generateJobPolicyPrompt(jobType, {
        prompt_field: promptField,
        user_input: userInput,
      });
      message.success('已生成中英双语 Prompt 草稿');
      return generated;
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      message.error(`生成任务策略 Prompt 失败：${text}`);
      throw error;
    } finally {
      controls.setGeneratingPolicyJobType(null);
    }
  };

  return {
    handleSavePolicy,
    handleGeneratePolicyPrompt,
  };
}
