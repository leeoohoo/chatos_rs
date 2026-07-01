// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Alert, Collapse, Space, Tag, Typography } from 'antd';

import { useI18n } from '../i18n/I18nProvider';
import type { McpPromptPreviewResponse } from '../types';

export function McpPromptPreviewCard({ preview }: { preview: McpPromptPreviewResponse }) {
  const { t } = useI18n();

  return (
    <Space direction="vertical" size="middle" style={{ width: '100%' }}>
      {preview.build.runtime_limitations ? (
        <Alert
          type="warning"
          showIcon
          message={t('mcpPreview.runtimeLimitations')}
          description={preview.build.runtime_limitations}
        />
      ) : null}

      <Space wrap>
        <Tag color={preview.enabled ? 'success' : 'default'}>
          {preview.enabled ? t('common.enabled') : t('common.disabled')}
        </Tag>
        <Tag color="blue">{preview.builtin_prompt_mode}</Tag>
        <Tag>{preview.builtin_prompt_locale}</Tag>
        <Tag color="processing">
          {t('mcpPreview.kindsCount', { count: preview.selected_builtin_kinds.length })}
        </Tag>
        <Tag color="cyan">
          {t('mcpPreview.sectionsCount', { count: preview.build.selected_section_ids.length })}
        </Tag>
      </Space>

      <div>
        <Typography.Text strong>{t('mcpPreview.activeKinds')}</Typography.Text>
        <div style={{ marginTop: 8 }}>
          <Space size={[8, 8]} wrap>
            {preview.selected_builtin_kinds.length ? (
              preview.selected_builtin_kinds.map((kind) => <Tag key={kind}>{kind}</Tag>)
            ) : (
              <Typography.Text type="secondary">{t('mcpPreview.noActiveKinds')}</Typography.Text>
            )}
          </Space>
        </div>
      </div>

      <Collapse
        ghost
        items={[
          {
            key: 'sections',
            label: t('mcpPreview.sections', { count: preview.build.selected_section_ids.length }),
            children: (
              <Space size={[8, 8]} wrap>
                {preview.build.selected_section_ids.map((item) => (
                  <Tag key={item} color="blue">
                    {item}
                  </Tag>
                ))}
              </Space>
            ),
          },
          {
            key: 'servers',
            label: t('mcpPreview.servers', {
              count: preview.build.active_builtin_server_names.length,
            }),
            children: (
              <Space size={[8, 8]} wrap>
                {preview.build.active_builtin_server_names.map((item) => (
                  <Tag key={item} color="processing">
                    {item}
                  </Tag>
                ))}
              </Space>
            ),
          },
        ]}
      />

      <div>
        <Typography.Text strong>{t('mcpPreview.promptContent')}</Typography.Text>
        <Typography.Paragraph
          style={{
            background: '#fafafa',
            padding: 12,
            borderRadius: 6,
            marginBottom: 0,
            marginTop: 8,
            whiteSpace: 'pre-wrap',
            fontFamily: 'monospace',
          }}
        >
          {preview.build.prompt || t('mcpPreview.emptyPrompt')}
        </Typography.Paragraph>
      </div>
    </Space>
  );
}
