import { Input, Modal, Select, Space, Switch, Typography } from 'antd';

import type { AgentAiCreateState, AgentPageTranslate } from './types';

const { Text } = Typography;

interface AgentAiCreateModalProps {
  t: AgentPageTranslate;
  state: AgentAiCreateState;
  saving: boolean;
  modelOptions: Array<{ value: string; label: string }>;
  onCancel: () => void;
  onSubmit: () => void | Promise<void>;
  onChange: (patch: Partial<AgentAiCreateState>) => void;
}

export function AgentAiCreateModal({
  t,
  state,
  saving,
  modelOptions,
  onCancel,
  onSubmit,
  onChange,
}: AgentAiCreateModalProps) {
  return (
    <Modal
      open={state.open}
      title={t('agents.aiCreate')}
      onCancel={onCancel}
      onOk={() => {
        void onSubmit();
      }}
      confirmLoading={saving}
    >
      <Space direction="vertical" size={10} style={{ width: '100%' }}>
        <Input.TextArea
          value={state.requirement}
          onChange={(event) => onChange({ requirement: event.target.value })}
          placeholder={t('agents.aiRequirement')}
          rows={5}
        />
        <Select
          showSearch
          allowClear={state.modelConfigs.length !== 1}
          loading={state.modelsLoading}
          value={state.modelConfigId || undefined}
          onChange={(value) => onChange({ modelConfigId: value ?? '' })}
          options={modelOptions}
          placeholder={t('agents.aiModelPlaceholder')}
          optionFilterProp="label"
        />
        <Input
          value={state.name}
          onChange={(event) => onChange({ name: event.target.value })}
          placeholder={t('agents.nameOptional')}
        />
        <Input
          value={state.category}
          onChange={(event) => onChange({ category: event.target.value })}
          placeholder={t('agents.categoryOptional')}
        />
        <Space>
          <Text>{t('agents.status')}</Text>
          <Switch checked={state.enabled} onChange={(enabled) => onChange({ enabled })} />
        </Space>
      </Space>
    </Modal>
  );
}
