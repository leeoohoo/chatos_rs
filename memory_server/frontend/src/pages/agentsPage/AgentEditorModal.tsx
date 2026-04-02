import { Input, Modal, Select, Space, Switch, Typography } from 'antd';

import type { AgentEditorState, AgentPageTranslate } from './types';

const { Text } = Typography;

interface AgentEditorModalProps {
  t: AgentPageTranslate;
  open: boolean;
  saving: boolean;
  editor: AgentEditorState;
  modelOptions: Array<{ value: string; label: string }>;
  pluginOptions: Array<{ value: string; label: string }>;
  skillOptions: Array<{ value: string; label: string }>;
  onCancel: () => void;
  onSave: () => void | Promise<void>;
  onChange: (updater: (prev: AgentEditorState) => AgentEditorState) => void;
  mergePluginSourcesWithSkills: (pluginSources: string[], skillIds: string[]) => string[];
}

export function AgentEditorModal({
  t,
  open,
  saving,
  editor,
  modelOptions,
  pluginOptions,
  skillOptions,
  onCancel,
  onSave,
  onChange,
  mergePluginSourcesWithSkills,
}: AgentEditorModalProps) {
  return (
    <Modal
      open={open}
      title={editor.id ? t('agents.edit') : t('agents.create')}
      onCancel={onCancel}
      onOk={() => {
        void onSave();
      }}
      confirmLoading={saving}
      width={760}
    >
      <Space direction="vertical" size={10} style={{ width: '100%' }}>
        <Input
          value={editor.name}
          onChange={(event) => onChange((prev) => ({ ...prev, name: event.target.value }))}
          placeholder={t('agents.name')}
        />
        <Input
          value={editor.category}
          onChange={(event) => onChange((prev) => ({ ...prev, category: event.target.value }))}
          placeholder={t('agents.category')}
        />
        <Input.TextArea
          value={editor.description}
          onChange={(event) => onChange((prev) => ({ ...prev, description: event.target.value }))}
          placeholder={t('agents.description')}
          rows={3}
        />
        <Input.TextArea
          value={editor.roleDefinition}
          onChange={(event) => onChange((prev) => ({ ...prev, roleDefinition: event.target.value }))}
          placeholder={t('agents.roleDefinition')}
          rows={6}
        />
        <Select
          showSearch
          allowClear
          value={editor.modelConfigId || undefined}
          onChange={(value) => onChange((prev) => ({ ...prev, modelConfigId: value || '' }))}
          options={modelOptions}
          placeholder={t('agents.executionModel')}
          optionFilterProp="label"
          style={{ width: '100%' }}
        />
        <Select
          mode="multiple"
          showSearch
          allowClear
          value={editor.pluginSources}
          onChange={(value) => onChange((prev) => ({
            ...prev,
            pluginSources: mergePluginSourcesWithSkills(value, prev.skillIds),
          }))}
          options={pluginOptions}
          placeholder={t('agents.pluginSelectPlaceholder')}
          optionFilterProp="label"
          style={{ width: '100%' }}
        />
        <Select
          mode="multiple"
          showSearch
          allowClear
          value={editor.skillIds}
          onChange={(value) => onChange((prev) => ({
            ...prev,
            skillIds: value,
            pluginSources: mergePluginSourcesWithSkills(prev.pluginSources, value),
          }))}
          options={skillOptions}
          placeholder={editor.pluginSources.length > 0
            ? t('agents.skillSelectPlaceholder')
            : t('agents.skillSelectPluginFirst')}
          optionFilterProp="label"
          style={{ width: '100%' }}
          disabled={editor.pluginSources.length === 0 && editor.skillIds.length === 0}
        />
        <Space>
          <Text>{t('agents.status')}</Text>
          <Switch
            checked={editor.enabled}
            onChange={(checked) => onChange((prev) => ({ ...prev, enabled: checked }))}
          />
        </Space>
      </Space>
    </Modal>
  );
}
