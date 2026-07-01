// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FormInstance } from 'antd';

import type { EngineJobPolicy } from '../../../types';
import type {
  ModelOptions,
  PolicyFormValues,
  PolicyPromptGenerator,
  PolicyMeta,
  PolicySaveHandler,
  PolicyViewKey,
} from '../../types';

export type PolicyEditorCardProps = {
  policy: EngineJobPolicy;
  meta: PolicyMeta;
  viewKey?: PolicyViewKey;
  modelOptions: ModelOptions;
  saving: boolean;
  generatingPrompt: boolean;
  onSave: PolicySaveHandler;
  onGeneratePrompt: PolicyPromptGenerator;
};

export type PolicySummaryProps = Pick<PolicyEditorCardProps, 'meta'> & {
  updatedAt: string;
};

export type PolicyFieldsProps = {
  form: FormInstance<PolicyFormValues>;
  initialValues: PolicyFormValues;
  meta: PolicyMeta;
  modelOptions: ModelOptions;
  promptFieldName: 'summary_prompt' | 'rollup_summary_prompt';
  promptLanguageFieldName:
    | 'summary_prompt_language'
    | 'rollup_summary_prompt_language';
  promptZhFieldName: 'summary_prompt_zh' | 'rollup_summary_prompt_zh';
  promptEnFieldName: 'summary_prompt_en' | 'rollup_summary_prompt_en';
  generatingPrompt: boolean;
  onGeneratePrompt: (userInput: string) => Promise<void>;
};
