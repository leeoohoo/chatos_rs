import { ReloadOutlined } from '@ant-design/icons';
import { Button, Descriptions, Progress, Space, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';

import { sandboxesApi } from '../api/sandboxes';
import { useI18n } from '../i18n';

export function PoolPage() {
  const { t } = useI18n();
  const query = useQuery({
    queryKey: ['pool-status'],
    queryFn: sandboxesApi.poolStatus,
  });
  const data = query.data;
  const percent = data && data.max_active > 0 ? Math.round((data.active / data.max_active) * 100) : 0;

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <div className="page-heading">
        <div>
          <Typography.Title level={3}>{t('pool.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('pool.subtitle')}</Typography.Text>
        </div>
        <Button icon={<ReloadOutlined />} onClick={() => void query.refetch()}>
          {t('common.refresh')}
        </Button>
      </div>

      <div className="surface">
        <Progress percent={percent} status={percent >= 100 ? 'exception' : 'active'} />
        <Descriptions bordered column={2} style={{ marginTop: 18 }}>
          <Descriptions.Item label={t('common.backend')}>{data?.backend ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('pool.active')}>
            {data ? `${data.active} / ${data.max_active}` : '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('pool.pending')}>
            {data ? `${data.pending} / ${data.max_pending}` : '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('pool.leaseTtl')}>
            {data ? `${data.lease_ttl_seconds}s` : '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('pool.cleanupInterval')}>
            {data ? `${data.cleanup_interval_seconds}s` : '-'}
          </Descriptions.Item>
        </Descriptions>
      </div>
    </Space>
  );
}
