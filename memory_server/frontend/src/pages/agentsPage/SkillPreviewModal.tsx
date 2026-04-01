import { Empty, Modal, Space, Spin, Typography } from 'antd';

import type {
  AgentPageTranslate,
  AgentSkillPreviewState,
} from './types';

const { Text } = Typography;

interface SkillPreviewModalProps {
  t: AgentPageTranslate;
  state: AgentSkillPreviewState;
  onCancel: () => void;
}

export function SkillPreviewModal({
  t,
  state,
  onCancel,
}: SkillPreviewModalProps) {
  return (
    <Modal
      open={state.open}
      title={`${t('agents.skillPreview')}: ${state.skill?.name || state.skill?.id || '-'}`}
      footer={null}
      onCancel={onCancel}
      width={860}
    >
      {state.loading ? (
        <div style={{ display: 'flex', justifyContent: 'center', padding: '32px 0' }}>
          <Spin />
        </div>
      ) : !state.skill ? (
        <Empty description={t('agents.skillNotFound')} />
      ) : (
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          <Text strong>{state.skill.id}</Text>
          <Text type="secondary">
            {t('agents.skillSourceType')}: {
              state.skill.plugin_source === 'inline'
                ? t('agents.skillSourceInline')
                : t('agents.skillSourceCenter')
            }
          </Text>
          <Text type="secondary">
            {t('agents.skillPluginSource')}: {state.skill.plugin_source || '-'}
          </Text>
          <Text type="secondary">
            {t('agents.skillSourcePath')}: {state.skill.source_path || '-'}
          </Text>
          <div
            style={{
              maxHeight: 520,
              overflow: 'auto',
              padding: 12,
              border: '1px solid #f0f0f0',
              borderRadius: 8,
              background: '#fafafa',
            }}
          >
            <pre
              style={{
                margin: 0,
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-word',
                fontFamily: 'SFMono-Regular, Consolas, Liberation Mono, Menlo, monospace',
                fontSize: 13,
                lineHeight: 1.6,
              }}
            >
              {state.skill.content || t('agents.skillContentEmpty')}
            </pre>
          </div>
        </Space>
      )}
    </Modal>
  );
}
