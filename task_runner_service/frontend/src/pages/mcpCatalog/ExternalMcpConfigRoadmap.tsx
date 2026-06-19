import {
  Descriptions,
  Space,
  Tag,
  Typography,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import { MCP_CARD_STYLE } from './mcpCatalogPageUtils';

type ExternalMcpConfigRoadmapProps = {
  t: TranslateFn;
};

export function ExternalMcpConfigRoadmap({ t }: ExternalMcpConfigRoadmapProps) {
  return (
    <Space direction="vertical" size="middle" style={MCP_CARD_STYLE}>
      <Typography.Title level={5} style={{ margin: 0 }}>
        {t('mcpCatalog.externalConfigRoadmapTitle')}
      </Typography.Title>
      <Descriptions bordered column={1} size="small">
        <Descriptions.Item label={t('mcpCatalog.externalConfigRuntime')}>
          <Tag color="success">{t('mcpCatalog.externalConfigRuntimeReady')}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label={t('mcpCatalog.externalConfigStorage')}>
          <Tag color="success">{t('mcpCatalog.externalConfigStorageReady')}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label={t('mcpCatalog.externalConfigTaskBinding')}>
          <Tag color="success">{t('mcpCatalog.externalConfigTaskBindingReady')}</Tag>
        </Descriptions.Item>
      </Descriptions>
    </Space>
  );
}
