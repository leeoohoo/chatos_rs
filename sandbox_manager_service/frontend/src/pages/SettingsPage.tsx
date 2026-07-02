// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { ReloadOutlined } from '@ant-design/icons';
import { Button, Descriptions, Space, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';

import { systemApi } from '../api/system';
import { useI18n } from '../i18n';

export function SettingsPage() {
  const { t } = useI18n();
  const query = useQuery({
    queryKey: ['system-config'],
    queryFn: systemApi.config,
  });
  const data = query.data;

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <div className="page-heading">
        <div>
          <Typography.Title level={3}>{t('settings.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('settings.subtitle')}</Typography.Text>
        </div>
        <Button icon={<ReloadOutlined />} onClick={() => void query.refetch()}>
          {t('common.refresh')}
        </Button>
      </div>

      <div className="surface">
        <Descriptions bordered column={2}>
          <Descriptions.Item label={t('settings.host')}>{data?.host ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('settings.port')}>{data?.port ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('common.backend')}>{data?.backend ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('settings.workRoot')}>{data?.work_root ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('settings.poolActiveLimit')}>
            {data?.pool_max_active ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.poolPendingLimit')}>
            {data?.pool_max_pending ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('pool.leaseTtl')}>
            {data ? `${data.lease_ttl_seconds}s` : '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('pool.cleanupInterval')}>
            {data ? `${data.cleanup_interval_seconds}s` : '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.agentPort')}>
            {data?.agent_port ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.dockerImage')}>{data?.docker_image ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('settings.dockerNetwork')}>
            {data?.docker_network_mode ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.kataCli')}>
            {data?.kata_container_cli ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.kataRuntime')}>
            {data?.kata_runtime ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.kataImage')}>
            {data?.kata_image ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.kataNetwork')}>
            {data?.kata_network_mode ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.imageTagPrefix')}>
            {data?.image_tag_prefix ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.imageBuildContext')}>
            {data?.image_build_context ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('settings.imageDockerfile')} span={2}>
            {data?.image_dockerfile ?? '-'}
          </Descriptions.Item>
        </Descriptions>
      </div>
    </Space>
  );
}
