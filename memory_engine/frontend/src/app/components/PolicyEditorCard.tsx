import { useEffect, useRef } from 'react';

import { Button, Card, Form } from 'antd';
import { SaveOutlined } from '@ant-design/icons';

import type { PolicyFormValues } from '../types';
import { policyFormInitialValues } from '../utils';
import { PolicyFields } from './policy/PolicyFields';
import { PolicySummary } from './policy/PolicySummary';
import type { PolicyEditorCardProps } from './policy/types';

export function PolicyEditorCard(props: PolicyEditorCardProps) {
  const {
    policy,
    meta,
    viewKey,
    modelOptions,
    saving,
    generatingPrompt,
    onSave,
    onGeneratePrompt,
  } = props;
  const [form] = Form.useForm<PolicyFormValues>();
  const initialValues = policyFormInitialValues(policy);
  const previousPolicySignatureRef = useRef<string | null>(null);
  const promptFieldName =
    policy.job_type === 'subject_memory' && viewKey === 'memory_rollup'
      ? 'rollup_summary_prompt'
      : 'summary_prompt';
  const promptLanguageFieldName =
    promptFieldName === 'rollup_summary_prompt'
      ? 'rollup_summary_prompt_language'
      : 'summary_prompt_language';
  const promptZhFieldName =
    promptFieldName === 'rollup_summary_prompt'
      ? 'rollup_summary_prompt_zh'
      : 'summary_prompt_zh';
  const promptEnFieldName =
    promptFieldName === 'rollup_summary_prompt'
      ? 'rollup_summary_prompt_en'
      : 'summary_prompt_en';
  const policySignature = JSON.stringify({
    jobType: policy.job_type,
    updatedAt: policy.updated_at,
    viewKey,
    promptFieldName,
    initialValues,
  });

  useEffect(() => {
    if (previousPolicySignatureRef.current === policySignature) {
      return;
    }
    previousPolicySignatureRef.current = policySignature;
    form.setFieldsValue(initialValues);
  }, [form, initialValues, policySignature]);

  const handleSubmit = async () => {
    const values = await form.validateFields();
    if (meta.showTargetSummaryTokens === false) {
      values.target_summary_tokens = null;
    }
    if (meta.showMaxThreadsPerTick === false) {
      values.max_threads_per_tick = null;
    }
    await onSave(policy.job_type, values);
  };

  const handleGeneratePrompt = async (userInput: string) => {
    const generated = await onGeneratePrompt(policy.job_type, promptFieldName, userInput);
    form.setFieldsValue({
      [promptZhFieldName]: generated.prompt_zh,
      [promptEnFieldName]: generated.prompt_en,
    } as Partial<PolicyFormValues>);
  };

  return (
    <Card
      title={<PolicySummary meta={meta} updatedAt={policy.updated_at} />}
      extra={
        <Button
          type="primary"
          icon={<SaveOutlined />}
          loading={saving}
          onClick={() => void handleSubmit()}
        >
          保存
        </Button>
      }
    >
      <PolicyFields
        form={form}
        initialValues={initialValues}
        meta={meta}
        modelOptions={modelOptions}
        promptFieldName={promptFieldName}
        promptLanguageFieldName={promptLanguageFieldName}
        promptZhFieldName={promptZhFieldName}
        promptEnFieldName={promptEnFieldName}
        generatingPrompt={generatingPrompt}
        onGeneratePrompt={handleGeneratePrompt}
      />
    </Card>
  );
}
