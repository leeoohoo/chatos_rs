// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Alert, Form, Input, Modal, Select, message } from 'antd';
import { useEffect } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';

import { api } from '../../api/client';
import { useI18n } from '../../i18n/I18nProvider';
import type { AgentPromptVendor } from '../../types';

type GenerateValues = {
  model_config_id: string;
  requirement: string;
};

export function AgentPromptGenerateModal({
  open,
  agentKey,
  vendor,
  currentContent,
  onClose,
  onGenerated,
}: {
  open: boolean;
  agentKey: string;
  vendor: AgentPromptVendor;
  currentContent: string;
  onClose: () => void;
  onGenerated: (content: string) => void;
}) {
  const { t } = useI18n();
  const [form] = Form.useForm<GenerateValues>();
  const modelsQuery = useQuery({
    queryKey: ['admin-ai-models'],
    queryFn: api.listAdminAiModels,
    enabled: open,
  });
  const generateMutation = useMutation({
    mutationFn: (values: GenerateValues) => api.generateAgentProviderPrompt(
      agentKey,
      vendor,
      { ...values, current_content: currentContent },
    ),
    onSuccess: (result) => {
      onGenerated(result.content);
      message.success(t('agent.promptGenerated'));
      onClose();
    },
    onError: (error) => message.error((error as Error).message),
  });

  useEffect(() => {
    if (!open) {
      form.resetFields();
      return;
    }
    const firstModel = modelsQuery.data?.[0];
    if (firstModel && !form.getFieldValue('model_config_id')) {
      form.setFieldValue('model_config_id', firstModel.id);
    }
  }, [form, modelsQuery.data, open]);

  return (
    <Modal
      title={t('agent.promptGenerateTitle')}
      open={open}
      okText={t('agent.promptGenerate')}
      cancelText={t('common.cancel')}
      confirmLoading={generateMutation.isPending}
      onCancel={onClose}
      onOk={() => form.submit()}
      destroyOnClose
    >
      <Alert type="info" showIcon message={t('agent.promptGenerateNotice')} />
      <Form form={form} layout="vertical" onFinish={(values) => generateMutation.mutate(values)}>
        <Form.Item
          name="model_config_id"
          label={t('agent.promptGenerateModel')}
          rules={[{ required: true }]}
        >
          <Select
            loading={modelsQuery.isLoading}
            options={(modelsQuery.data || []).map((model) => ({
              value: model.id,
              label: `${model.name} · ${model.model || model.model_name || model.provider}`,
            }))}
          />
        </Form.Item>
        <Form.Item
          name="requirement"
          label={t('agent.promptGenerateRequirement')}
          rules={[{ required: true }]}
        >
          <Input.TextArea rows={5} maxLength={4_000} showCount />
        </Form.Item>
      </Form>
    </Modal>
  );
}
