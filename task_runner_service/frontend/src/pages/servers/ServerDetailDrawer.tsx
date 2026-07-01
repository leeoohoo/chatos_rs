// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Descriptions,
  Drawer,
  Empty,
  Space,
  Tag,
  Typography,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { RemoteServerRecord } from '../../types';
import {
  renderTestStatus,
  serverCreatorLabel,
} from './serverPageUtils';

type ServerDetailDrawerProps = {
  t: TranslateFn;
  open: boolean;
  loading: boolean;
  server: RemoteServerRecord | null;
  testing: boolean;
  authTypeLabel: (value: string) => string;
  onClose: () => void;
  onTest: (serverId: string) => void;
  onEdit: (server: RemoteServerRecord) => void;
  onDelete: (server: RemoteServerRecord) => void;
};

export function ServerDetailDrawer({
  t,
  open,
  loading,
  server,
  testing,
  authTypeLabel,
  onClose,
  onTest,
  onEdit,
  onDelete,
}: ServerDetailDrawerProps) {
  return (
    <Drawer
      title={server
        ? t('servers.detail.titleWithName', { name: server.name })
        : t('servers.detail.title')}
      open={open}
      width={760}
      onClose={onClose}
    >
      {server ? (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Space wrap>
            <Button
              loading={testing}
              onClick={() => onTest(server.id)}
            >
              {t('servers.detail.testConnection')}
            </Button>
            <Button onClick={() => onEdit(server)}>
              {t('servers.detail.editConfig')}
            </Button>
            <Button danger onClick={() => onDelete(server)}>
              {t('servers.detail.deleteServer')}
            </Button>
          </Space>

          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label={t('servers.detail.serverId')}>{server.id}</Descriptions.Item>
            <Descriptions.Item label={t('servers.detail.name')}>{server.name}</Descriptions.Item>
            <Descriptions.Item label={t('servers.detail.creator')}>
              {serverCreatorLabel(server)}
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.detail.taskId')}>
              {server.task_id || '-'}
            </Descriptions.Item>
            <Descriptions.Item label="Host">
              {server.host}:{server.port}
            </Descriptions.Item>
            <Descriptions.Item label="Username">{server.username}</Descriptions.Item>
            <Descriptions.Item label={t('servers.column.authType')}>
              {authTypeLabel(server.auth_type)}
            </Descriptions.Item>
            <Descriptions.Item label={t('common.status')}>
              <Tag color={server.enabled ? 'success' : 'default'}>
                {server.enabled ? t('common.enabled') : t('common.disabled')}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label="Host Key Policy">
              <Tag color={server.host_key_policy === 'strict' ? 'blue' : 'default'}>
                {server.host_key_policy}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.form.defaultRemotePath')}>
              {server.default_remote_path || '-'}
            </Descriptions.Item>
            <Descriptions.Item label="Password">
              {server.password ? t('servers.detail.passwordSaved') : '-'}
            </Descriptions.Item>
            <Descriptions.Item label="Private Key Path">
              {server.private_key_path || '-'}
            </Descriptions.Item>
            <Descriptions.Item label="Certificate Path">
              {server.certificate_path || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.detail.lastTestStatus')}>
              {renderTestStatus(server.last_test_status, t)}
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.detail.lastTestedAt')}>
              {server.last_tested_at
                ? dayjs(server.last_tested_at).format('YYYY-MM-DD HH:mm:ss')
                : '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.detail.lastTestMessage')}>
              {server.last_test_message || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.detail.lastActiveAt')}>
              {server.last_active_at
                ? dayjs(server.last_active_at).format('YYYY-MM-DD HH:mm:ss')
                : '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.detail.createdAt')}>
              {dayjs(server.created_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
            <Descriptions.Item label={t('common.updatedAt')}>
              {dayjs(server.updated_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
          </Descriptions>

          <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>
            {t('servers.detail.hint')}
          </Typography.Paragraph>
        </Space>
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
      )}
    </Drawer>
  );
}
