// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Collapse,
  Descriptions,
  Space,
  Statistic,
  Tag,
  Typography,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { RemoteOperationStats } from '../shared/remoteOperationUtils';
import { CodeParagraph } from './payloadView';
import type { RemoteOperationView } from './runEventUtils';
import {
  formatRemoteEndpoint,
  formatRemoteVolume,
} from './runEventUtils';

type RunRemoteOperationsSectionProps = {
  t: TranslateFn;
  operations: RemoteOperationView[];
  stats: RemoteOperationStats;
  onManageServers: () => void;
  onOpenServer: (serverId: string) => void;
};

export function RunRemoteOperationsSection({
  t,
  operations,
  stats,
  onManageServers,
  onOpenServer,
}: RunRemoteOperationsSectionProps) {
  if (!operations.length) {
    return null;
  }

  return (
    <div>
      <Space
        style={{ justifyContent: 'space-between', width: '100%', marginBottom: 12 }}
        align="start"
      >
        <Space direction="vertical" size={0}>
          <Typography.Title level={5} style={{ margin: 0 }}>
            {t('runs.remote.title')}
          </Typography.Title>
          <Typography.Text type="secondary">
            {t('runs.remote.description')}
          </Typography.Text>
        </Space>
        <Button size="small" onClick={onManageServers}>
          {t('runs.remote.manageServers')}
        </Button>
      </Space>

      <Space size="large" wrap style={{ marginBottom: 12 }}>
        <Statistic title={t('tasks.detail.remoteOperationCount')} value={stats.total} />
        <Statistic title={t('tasks.detail.involvedServers')} value={stats.serverCount} />
        <Statistic title={t('tasks.detail.success')} value={stats.successCount} />
        <Statistic title={t('tasks.detail.failed')} value={stats.failedCount} />
      </Space>

      <Collapse
        ghost
        items={operations.map((operation, index) => ({
          key: `${operation.toolCallId || operation.name}-${index}`,
          label: (
            <Space wrap>
              <Tag color={operation.success ? 'success' : 'error'}>
                {operation.success ? t('common.success') : t('common.failed')}
              </Tag>
              <Typography.Text strong>{operation.name}</Typography.Text>
              {operation.connectionName ? (
                <Button
                  type="link"
                  size="small"
                  style={{ paddingInline: 0 }}
                  onClick={(event) => {
                    event.preventDefault();
                    if (!operation.connectionId) {
                      onManageServers();
                      return;
                    }
                    onOpenServer(operation.connectionId);
                  }}
                >
                  {operation.connectionName}
                </Button>
              ) : operation.connectionId ? (
                <Typography.Text code>{operation.connectionId.slice(0, 12)}</Typography.Text>
              ) : null}
              {operation.summary ? (
                <Typography.Text type="secondary">{operation.summary}</Typography.Text>
              ) : null}
            </Space>
          ),
          children: (
            <Space direction="vertical" size="middle" style={{ width: '100%' }}>
              <Descriptions bordered column={1} size="small">
                <Descriptions.Item label={t('runs.remote.operation')}>{operation.name}</Descriptions.Item>
                <Descriptions.Item label={t('tasks.detail.server')}>
                  {operation.connectionName || operation.connectionId || '-'}
                </Descriptions.Item>
                <Descriptions.Item label={t('tasks.detail.host')}>
                  {formatRemoteEndpoint(
                    operation.username,
                    operation.host,
                    operation.port,
                  ) || '-'}
                </Descriptions.Item>
                <Descriptions.Item label={t('runs.remote.command')}>
                  {operation.command || '-'}
                </Descriptions.Item>
                <Descriptions.Item label={t('runs.remote.path')}>
                  {operation.path || '-'}
                </Descriptions.Item>
                <Descriptions.Item label={t('tasks.detail.remoteHost')}>
                  {operation.remoteHost || '-'}
                </Descriptions.Item>
                <Descriptions.Item label={t('runs.remote.outputTruncated')}>
                  {operation.outputTruncated === undefined
                    ? '-'
                    : operation.outputTruncated
                      ? t('common.yes')
                      : t('common.no')}
                </Descriptions.Item>
                <Descriptions.Item label={t('runs.remote.volume')}>
                  {formatRemoteVolume(operation)}
                </Descriptions.Item>
              </Descriptions>

              {operation.content ? (
                <div>
                  <Typography.Text strong>{t('runs.detail.resultSummary')}</Typography.Text>
                  <CodeParagraph value={operation.content} />
                </div>
              ) : null}

              {operation.output ? (
                <div>
                  <Typography.Text strong>{t('runs.remote.commandOutput')}</Typography.Text>
                  <CodeParagraph value={operation.output} />
                </div>
              ) : null}

              {operation.result !== undefined ? (
                <div>
                  <Typography.Text strong>{t('runs.remote.structuredResult')}</Typography.Text>
                  <CodeParagraph value={operation.result} />
                </div>
              ) : null}
            </Space>
          ),
        }))}
      />
    </div>
  );
}
