import { Collapse, Empty, Modal, Space, Spin, Typography } from 'antd';

import type {
  AgentPageTranslate,
  AgentPluginPreviewState,
} from './types';

const { Text } = Typography;

const codeStyle = {
  margin: 0,
  whiteSpace: 'pre-wrap' as const,
  wordBreak: 'break-word' as const,
  fontFamily: 'SFMono-Regular, Consolas, Liberation Mono, Menlo, monospace',
  fontSize: 13,
  lineHeight: 1.6,
};

const panelStyle = {
  maxHeight: 280,
  overflow: 'auto' as const,
  padding: 12,
  border: '1px solid #f0f0f0',
  borderRadius: 8,
  background: '#fafafa',
};

interface PluginPreviewModalProps {
  t: AgentPageTranslate;
  state: AgentPluginPreviewState;
  resolvePluginDisplayName: (pluginSource: string) => string;
  onCancel: () => void;
}

export function PluginPreviewModal({
  t,
  state,
  resolvePluginDisplayName,
  onCancel,
}: PluginPreviewModalProps) {
  return (
    <Modal
      open={state.open}
      title={`${t('agents.pluginPreview')}: ${
        state.plugin?.name?.trim()
          || resolvePluginDisplayName(state.source)
          || '-'
      }`}
      footer={null}
      onCancel={onCancel}
      width={920}
    >
      {state.loading ? (
        <div style={{ display: 'flex', justifyContent: 'center', padding: '32px 0' }}>
          <Spin />
        </div>
      ) : (
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          <Text strong>{state.source || '-'}</Text>
          <Text type="secondary">
            {t('agents.pluginCategory')}: {state.plugin?.category?.trim() || '-'}
          </Text>
          <Text type="secondary">
            {t('agents.pluginVersion')}: {state.plugin?.version?.trim() || '-'}
          </Text>
          <Text type="secondary">
            {t('agents.pluginRepository')}: {state.plugin?.repository?.trim() || '-'}
          </Text>
          <Text type="secondary">
            {t('agents.pluginBranch')}: {state.plugin?.branch?.trim() || '-'}
          </Text>
          <Text strong>{t('agents.pluginDescription')}</Text>
          <div style={{ ...panelStyle, maxHeight: 220 }}>
            <pre style={codeStyle}>
              {state.plugin?.description?.trim() || t('agents.pluginDescriptionEmpty')}
            </pre>
          </div>
          <Text strong>{t('agents.pluginMainContent')}</Text>
          <div style={panelStyle}>
            <pre style={codeStyle}>
              {state.plugin?.content?.trim() || t('agents.pluginMainContentEmpty')}
            </pre>
          </div>
          <Text strong>{t('agents.pluginCommands')}</Text>
          {(state.plugin?.commands || []).length === 0 ? (
            <Empty description={t('agents.pluginNoCommands')} />
          ) : (
            <Collapse
              size="small"
              items={(state.plugin?.commands || []).map((command, index) => ({
                key: `${command.source_path || command.name || index}`,
                label: `${command.name || '-'} (${command.source_path || '-'})`,
                children: (
                  <div style={{ ...panelStyle, maxHeight: 260, padding: 10 }}>
                    <pre style={codeStyle}>
                      {command.content || t('agents.pluginCommandContentEmpty')}
                    </pre>
                  </div>
                ),
              }))}
            />
          )}
          <Text strong>{t('agents.pluginSkills')}</Text>
          {state.skills.length === 0 ? (
            <Empty description={t('agents.pluginNoSkills')} />
          ) : (
            <Collapse
              size="small"
              defaultActiveKey={state.skills.map((skill) => skill.id)}
              items={state.skills.map((skill) => ({
                key: skill.id,
                label: `${skill.name || t('agents.unnamedSkill')} (${skill.id})`,
                children: (
                  <Space direction="vertical" size={8} style={{ width: '100%' }}>
                    <Text type="secondary">
                      {t('agents.skillSourcePath')}: {skill.source_path || '-'}
                    </Text>
                    <div style={{ ...panelStyle, padding: 10 }}>
                      <pre style={codeStyle}>
                        {skill.content || t('agents.skillContentEmpty')}
                      </pre>
                    </div>
                  </Space>
                ),
              }))}
            />
          )}
        </Space>
      )}
    </Modal>
  );
}
