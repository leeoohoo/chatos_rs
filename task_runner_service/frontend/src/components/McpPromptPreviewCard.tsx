import { Alert, Collapse, Space, Tag, Typography } from 'antd';

import type { McpPromptPreviewResponse } from '../types';

export function McpPromptPreviewCard({ preview }: { preview: McpPromptPreviewResponse }) {
  return (
    <Space direction="vertical" size="middle" style={{ width: '100%' }}>
      {preview.build.runtime_limitations ? (
        <Alert
          type="warning"
          showIcon
          message="Runtime Limitations"
          description={preview.build.runtime_limitations}
        />
      ) : null}

      <Space wrap>
        <Tag color={preview.enabled ? 'success' : 'default'}>
          {preview.enabled ? 'enabled' : 'disabled'}
        </Tag>
        <Tag>{preview.init_mode}</Tag>
        <Tag color="blue">{preview.builtin_prompt_mode}</Tag>
        <Tag>{preview.builtin_prompt_locale}</Tag>
        <Tag color="processing">kinds: {preview.selected_builtin_kinds.length}</Tag>
        <Tag color="cyan">sections: {preview.build.selected_section_ids.length}</Tag>
      </Space>

      <div>
        <Typography.Text strong>Active Builtin Kinds</Typography.Text>
        <div style={{ marginTop: 8 }}>
          <Space size={[8, 8]} wrap>
            {preview.selected_builtin_kinds.length ? (
              preview.selected_builtin_kinds.map((kind) => <Tag key={kind}>{kind}</Tag>)
            ) : (
              <Typography.Text type="secondary">当前没有启用 builtin kinds</Typography.Text>
            )}
          </Space>
        </div>
      </div>

      <Collapse
        ghost
        items={[
          {
            key: 'sections',
            label: `Section IDs (${preview.build.selected_section_ids.length})`,
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
            label: `Builtin Servers (${preview.build.active_builtin_server_names.length})`,
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
        <Typography.Text strong>Prompt Content</Typography.Text>
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
          {preview.build.prompt || '当前配置下没有生成 builtin MCP prompt'}
        </Typography.Paragraph>
      </div>
    </Space>
  );
}
