// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Collapse,
  Descriptions,
  Space,
  Tag,
  Typography,
} from 'antd';

import { buildApiUrl } from '../../api/client';
import { useI18n } from '../../i18n/I18nProvider';
import type { McpServerInfo } from '../../types';
import {
  MCP_CARD_STYLE,
  profileDescription,
  profileLabel,
  TOOL_PROFILE_COLORS,
} from './mcpCatalogPageUtils';

export function ExternalMcpServerCard({
  info,
  onRefresh,
}: {
  info: McpServerInfo;
  onRefresh: () => void;
}) {
  const { t } = useI18n();
  const profiles =
    info.tool_profiles && info.tool_profiles.length
      ? info.tool_profiles
      : [
          {
            key: 'admin_full',
            label: 'Admin / full metadata',
            description: 'Complete server metadata list before user/profile access filtering.',
            tool_names: info.tool_names,
          },
        ];

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space direction="vertical" size="middle" style={MCP_CARD_STYLE}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }} align="start">
          <Space direction="vertical" size={0}>
            <Typography.Title level={5} style={{ margin: 0 }}>
              {t('mcpCatalog.externalServerTitle')}
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('mcpCatalog.externalServerSubtitle')}
            </Typography.Text>
          </Space>
          <Button onClick={onRefresh}>{t('common.refresh')}</Button>
        </Space>

        <Space wrap>
          {info.transports.map((transport) => (
            <Tag key={transport} color="blue">
              {transport}
            </Tag>
          ))}
        </Space>

        <Descriptions bordered column={1} size="small">
          <Descriptions.Item label="HTTP Endpoint">
            {info.http_endpoint_path ? (
              <Typography.Text code>{buildApiUrl(info.http_endpoint_path)}</Typography.Text>
            ) : (
              '-'
            )}
          </Descriptions.Item>
          <Descriptions.Item label="stdio Command">
            {info.stdio_command ? (
              <Typography.Text code>
                {[info.stdio_command, ...info.stdio_args].join(' ')}
              </Typography.Text>
            ) : (
              '-'
            )}
          </Descriptions.Item>
          <Descriptions.Item label={t('mcpCatalog.metadataToolCount')}>
            {info.tool_names.length}
          </Descriptions.Item>
        </Descriptions>
      </Space>

      <Space direction="vertical" size="middle" style={MCP_CARD_STYLE}>
        <Space direction="vertical" size={0}>
          <Typography.Title level={5} style={{ margin: 0 }}>
            {t('mcpCatalog.profileToolsTitle')}
          </Typography.Title>
          <Typography.Text type="secondary">
            {t('mcpCatalog.profileToolsSubtitle')}
          </Typography.Text>
        </Space>

        <Collapse
          items={profiles.map((profile) => ({
            key: profile.key,
            label: (
              <Space wrap>
                <Typography.Text strong>{profileLabel(profile, t)}</Typography.Text>
                <Tag color={TOOL_PROFILE_COLORS[profile.key] || 'default'}>
                  {t('mcpCatalog.toolCount', { count: profile.tool_names.length })}
                </Tag>
                <Typography.Text type="secondary">
                  {profileDescription(profile, t)}
                </Typography.Text>
              </Space>
            ),
            children: <ToolNameList names={profile.tool_names} />,
          }))}
        />
      </Space>
    </Space>
  );
}

function ToolNameList({ names }: { names: string[] }) {
  const { t } = useI18n();

  if (!names.length) {
    return <Typography.Text type="secondary">{t('common.noData')}</Typography.Text>;
  }

  return (
    <Space wrap size={[6, 6]}>
      {names.map((name) => (
        <Tag key={name}>{name}</Tag>
      ))}
    </Space>
  );
}
