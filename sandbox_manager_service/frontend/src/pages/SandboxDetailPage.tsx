import { ReloadOutlined, SafetyCertificateOutlined } from '@ant-design/icons';
import { Alert, Button, Descriptions, Space, Tabs, Tag, Typography, message } from 'antd';
import { useMutation, useQuery } from '@tanstack/react-query';
import dayjs from 'dayjs';
import { useParams } from 'react-router-dom';

import { sandboxesApi } from '../api/sandboxes';
import { EventTimeline } from '../components/EventTimeline';
import { SandboxActions } from '../components/SandboxActions';
import { StatusTag } from '../components/StatusTag';
import { useI18n } from '../i18n';

export function SandboxDetailPage() {
  const { t } = useI18n();
  const { sandboxId = '' } = useParams();
  const sandboxQuery = useQuery({
    queryKey: ['sandbox', sandboxId],
    queryFn: () => sandboxesApi.get(sandboxId),
    enabled: Boolean(sandboxId),
  });
  const eventsQuery = useQuery({
    queryKey: ['sandbox-events', sandboxId],
    queryFn: () => sandboxesApi.events(sandboxId),
    enabled: Boolean(sandboxId),
  });
  const healthMutation = useMutation({
    mutationFn: () => sandboxesApi.health(sandboxId),
    onSuccess: (result) => {
      if (result.ok) {
        message.success(t('health.success'));
      } else {
        message.warning(t('health.warning'));
      }
      void eventsQuery.refetch();
    },
    onError: (error) => message.error(error instanceof Error ? error.message : t('health.failure')),
  });
  const sandbox = sandboxQuery.data;
  const health = healthMutation.data;
  const renderHealthTag = (value?: boolean | null) => {
    if (value === true) {
      return <Tag color="success">{t('common.yes')}</Tag>;
    }
    if (value === false) {
      return <Tag color="error">{t('common.no')}</Tag>;
    }
    return <Tag>{t('common.unknown')}</Tag>;
  };

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <div className="page-heading">
        <div>
          <Typography.Title level={3}>{t('detail.title')}</Typography.Title>
          <Typography.Text type="secondary">{sandboxId}</Typography.Text>
        </div>
        <Space>
          <Button
            icon={<ReloadOutlined />}
            onClick={() => {
              void sandboxQuery.refetch();
              void eventsQuery.refetch();
            }}
          >
            {t('common.refresh')}
          </Button>
          <Button
            icon={<SafetyCertificateOutlined />}
            loading={healthMutation.isPending}
            disabled={!sandbox}
            onClick={() => healthMutation.mutate()}
          >
            {t('common.healthCheck')}
          </Button>
          {sandbox ? <SandboxActions sandbox={sandbox} size="middle" /> : null}
        </Space>
      </div>

      <div className="surface">
        <Descriptions bordered column={2}>
          <Descriptions.Item label={t('common.status')}>
            {sandbox ? <StatusTag status={sandbox.status} /> : '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('common.backend')}>{sandbox?.backend ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('health.backendId')}>{sandbox?.backend_id ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('common.lease')}>{sandbox?.id ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('common.sandbox')}>{sandbox?.sandbox_id ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('common.tenant')}>{sandbox?.tenant_id ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('common.user')}>{sandbox?.user_id ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('common.project')}>{sandbox?.project_id ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('common.run')}>{sandbox?.run_id ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('common.agent')}>{sandbox?.agent_endpoint ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('common.network')}>{sandbox?.network.mode ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('common.workspace')} span={2}>
            <Typography.Text copyable>{sandbox?.run_workspace ?? '-'}</Typography.Text>
          </Descriptions.Item>
          <Descriptions.Item label={t('common.lastError')} span={2}>
            {sandbox?.last_error ?? '-'}
          </Descriptions.Item>
        </Descriptions>
      </div>

      {health ? (
        <div className="surface">
          <Space direction="vertical" size={14} style={{ width: '100%' }}>
            <div>
              <Typography.Title level={4}>{t('health.title')}</Typography.Title>
              <Typography.Text type="secondary">{t('health.subtitle')}</Typography.Text>
            </div>
            <Alert
              showIcon
              type={health.ok ? 'success' : 'warning'}
              message={health.ok ? t('health.healthy') : t('health.unhealthy')}
              description={health.message}
            />
            <Descriptions bordered column={2}>
              <Descriptions.Item label={t('health.checkedAt')}>
                {dayjs(health.checked_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
              <Descriptions.Item label={t('common.status')}>
                <StatusTag status={health.status} />
              </Descriptions.Item>
              <Descriptions.Item label={t('health.backendAlive')}>
                {renderHealthTag(health.backend_alive)}
              </Descriptions.Item>
              <Descriptions.Item label={t('health.agentAlive')}>
                {renderHealthTag(health.agent_alive)}
              </Descriptions.Item>
              <Descriptions.Item label={t('health.workspaceAlive')}>
                {renderHealthTag(health.workspace_alive)}
              </Descriptions.Item>
              <Descriptions.Item label={t('health.backendId')}>
                {health.backend_id ?? '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('common.agent')} span={2}>
                {health.agent_endpoint ?? '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('health.checks')} span={2}>
                <Space wrap>
                  {health.checks.map((check) => (
                    <Tag key={check.name} color={check.ok ? 'success' : 'error'}>
                      {t(`health.check.${check.name}`)} · {check.ok ? t('common.yes') : t('common.no')}
                    </Tag>
                  ))}
                </Space>
              </Descriptions.Item>
              <Descriptions.Item label={t('health.message')} span={2}>
                <Space direction="vertical" size={4}>
                  {health.checks.map((check) => (
                    <Typography.Text key={check.name} type={check.ok ? 'secondary' : 'danger'}>
                      {t(`health.check.${check.name}`)}: {check.message}
                    </Typography.Text>
                  ))}
                </Space>
              </Descriptions.Item>
            </Descriptions>
          </Space>
        </div>
      ) : null}

      <div className="surface">
        <Tabs
          items={[
            {
              key: 'limits',
              label: t('common.resources'),
              children: sandbox ? (
                <Descriptions bordered column={4}>
                  <Descriptions.Item label="CPU">{sandbox.resource_limits.cpu}</Descriptions.Item>
                  <Descriptions.Item label={t('create.memoryMb')}>
                    {sandbox.resource_limits.memory_mb} MB
                  </Descriptions.Item>
                  <Descriptions.Item label={t('create.diskMb')}>
                    {sandbox.resource_limits.disk_mb} MB
                  </Descriptions.Item>
                  <Descriptions.Item label={t('create.maxProcesses')}>
                    {sandbox.resource_limits.max_processes}
                  </Descriptions.Item>
                </Descriptions>
              ) : null,
            },
            {
              key: 'events',
              label: t('common.events'),
              children: <EventTimeline events={eventsQuery.data ?? []} />,
            },
          ]}
        />
      </div>
    </Space>
  );
}
