// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  ApiOutlined,
  EyeOutlined,
  ReloadOutlined,
  SearchOutlined,
} from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';
import { Alert, Button, Form, Input, Modal, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { useI18n } from '../i18n/I18nProvider';
import type {
  PluginAvailabilityStatus,
  PluginComponentStatusRecord,
  PluginInstallationRecord,
  PluginInstallStatus,
  PluginOAuthConnectionRecord,
  PluginRequirementStatus,
} from '../pluginTypes';
import { optionalText } from './formUtils';

type DiagnosticsFilters = {
  device_id?: string;
};

type JsonDetail =
  | { titleKey: 'pluginDiagnostics.installationDetails'; value: PluginInstallationRecord }
  | { titleKey: 'pluginDiagnostics.componentDetails'; value: PluginComponentStatusRecord }
  | { titleKey: 'pluginDiagnostics.oauthDetails'; value: PluginOAuthConnectionRecord };

function installStatusColor(value: PluginInstallStatus | string): string {
  if (value === 'installed') {
    return 'green';
  }
  if (value === 'rejected') {
    return 'red';
  }
  if (value === 'not_installed' || value === 'uninstalling') {
    return 'default';
  }
  return 'gold';
}

function availabilityStatusColor(value: PluginAvailabilityStatus | string): string {
  if (value === 'ready') {
    return 'green';
  }
  if (value === 'partially_available') {
    return 'cyan';
  }
  if (value === 'revoked' || value === 'unavailable' || value === 'unsupported_platform') {
    return 'red';
  }
  return 'gold';
}

function requirementStatusColor(value: PluginRequirementStatus | string): string {
  if (value === 'satisfied') {
    return 'green';
  }
  if (value === 'denied' || value === 'missing' || value === 'failed') {
    return 'red';
  }
  return value === 'pending' ? 'gold' : 'default';
}

export function PluginDiagnosticsPage() {
  const { t } = useI18n();
  const [form] = Form.useForm<DiagnosticsFilters>();
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const [selectedInstallation, setSelectedInstallation] =
    useState<PluginInstallationRecord | null>(null);
  const [detail, setDetail] = useState<JsonDetail | null>(null);
  const installedQuery = useQuery({
    queryKey: ['plugin-installations', deviceId],
    queryFn: () => api.listInstalledPlugins({ device_id: deviceId || '' }),
    enabled: Boolean(deviceId),
  });
  const oauthQuery = useQuery({
    queryKey: ['plugin-oauth', deviceId, selectedInstallation?.plugin_id],
    queryFn: () =>
      api.listPluginOAuthConnections(selectedInstallation?.plugin_id || '', {
        device_id: deviceId || '',
      }),
    enabled: Boolean(deviceId && selectedInstallation?.plugin_id),
  });

  const installationColumns = useMemo<ColumnsType<PluginInstallationRecord>>(
    () => [
      {
        title: t('pluginDiagnostics.pluginRelease'),
        key: 'plugin',
        width: 240,
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <CompactId value={record.plugin_id} />
            <CompactId value={record.release_id} />
          </Space>
        ),
      },
      {
        title: t('pluginDiagnostics.versionPlatform'),
        key: 'version',
        width: 150,
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text>{record.version}</Typography.Text>
            <Typography.Text type="secondary">{record.platform}</Typography.Text>
          </Space>
        ),
      },
      {
        title: t('pluginDiagnostics.installAvailability'),
        key: 'availability',
        width: 210,
        render: (_, record) => (
          <Space direction="vertical" size={2}>
            <Tag color={installStatusColor(record.install_status)}>
              {t(`pluginInstallStatus.${record.install_status}`)}
            </Tag>
            <Tag color={availabilityStatusColor(record.availability_status)}>
              {t(`pluginAvailabilityStatus.${record.availability_status}`)}
            </Tag>
          </Space>
        ),
      },
      {
        title: t('pluginDiagnostics.requirements'),
        key: 'requirements',
        width: 260,
        render: (_, record) => (
          <Space wrap size={[4, 4]}>
            <Tag color={requirementStatusColor(record.dependency_status)}>
              {t('pluginDiagnostics.dependency')}: {t(`pluginRequirementStatus.${record.dependency_status}`)}
            </Tag>
            <Tag color={requirementStatusColor(record.permission_status)}>
              {t('pluginDiagnostics.permission')}: {t(`pluginRequirementStatus.${record.permission_status}`)}
            </Tag>
            <Tag color={requirementStatusColor(record.auth_status)}>
              {t('pluginDiagnostics.auth')}: {t(`pluginRequirementStatus.${record.auth_status}`)}
            </Tag>
          </Space>
        ),
      },
      {
        title: t('pluginDiagnostics.components'),
        key: 'components',
        width: 160,
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text>{record.component_statuses.length}</Typography.Text>
            <Button size="small" type="link" onClick={() => setDetail({ titleKey: 'pluginDiagnostics.installationDetails', value: record })}>
              {t('pluginDiagnostics.viewPayload')}
            </Button>
          </Space>
        ),
      },
      {
        title: t('pluginDiagnostics.active'),
        dataIndex: 'active',
        width: 100,
        render: (value) => (
          <Tag color={value ? 'green' : 'default'}>{t(value ? 'common.yes' : 'common.no')}</Tag>
        ),
      },
      {
        title: t('pluginDiagnostics.lastChecked'),
        dataIndex: 'last_checked_at',
        width: 180,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 130,
        fixed: 'right',
        render: (_, record) => (
          <Button
            size="small"
            icon={<ApiOutlined />}
            onClick={() => setSelectedInstallation(record)}
          >
            {t('pluginDiagnostics.oauth')}
          </Button>
        ),
      },
    ],
    [t],
  );

  const componentColumns = useMemo<ColumnsType<PluginComponentStatusRecord>>(
    () => [
      {
        title: t('pluginDiagnostics.component'),
        dataIndex: 'component_key',
        width: 220,
        render: (value) => <CompactId value={value} />,
      },
      {
        title: t('pluginDiagnostics.kind'),
        dataIndex: 'kind',
        width: 160,
        render: (value) => <Tag>{t(`pluginComponentKind.${value}`)}</Tag>,
      },
      {
        title: t('pluginDiagnostics.availability'),
        dataIndex: 'availability_status',
        width: 180,
        render: (value) => (
          <Tag color={availabilityStatusColor(value)}>
            {t(`pluginAvailabilityStatus.${value}`)}
          </Tag>
        ),
      },
      {
        title: t('pluginDiagnostics.lastChecked'),
        dataIndex: 'last_checked_at',
        width: 180,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('pluginDiagnostics.error'),
        dataIndex: 'last_error',
        ellipsis: true,
        render: (value) => value || <Typography.Text type="secondary">-</Typography.Text>,
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 100,
        render: (_, record) => (
          <Button
            size="small"
            icon={<EyeOutlined />}
            onClick={() => setDetail({ titleKey: 'pluginDiagnostics.componentDetails', value: record })}
          >
            {t('common.view')}
          </Button>
        ),
      },
    ],
    [t],
  );

  const oauthColumns = useMemo<ColumnsType<PluginOAuthConnectionRecord>>(
    () => [
      {
        title: t('pluginDiagnostics.provider'),
        dataIndex: 'provider',
        width: 150,
        render: (value) => <Tag color="blue">{value}</Tag>,
      },
      {
        title: t('pluginDiagnostics.connected'),
        dataIndex: 'connected',
        width: 120,
        render: (value) => (
          <Tag color={value ? 'green' : 'default'}>{t(value ? 'common.yes' : 'common.no')}</Tag>
        ),
      },
      {
        title: t('pluginDiagnostics.component'),
        dataIndex: 'component_key',
        width: 220,
        render: (value) => <CompactId value={value} />,
      },
      {
        title: t('pluginDiagnostics.account'),
        dataIndex: 'account_display',
        width: 180,
        render: (value) => value || <Typography.Text type="secondary">-</Typography.Text>,
      },
      {
        title: t('pluginDiagnostics.scopes'),
        dataIndex: 'scopes',
        render: (values: string[]) => (
          <Space wrap size={[4, 4]}>
            {(values || []).length ? (
              values.map((scope) => <Tag key={scope}>{scope}</Tag>)
            ) : (
              <Typography.Text type="secondary">-</Typography.Text>
            )}
          </Space>
        ),
      },
      {
        title: t('pluginDiagnostics.expiresAt'),
        dataIndex: 'expires_at',
        width: 180,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('pluginDiagnostics.updatedAt'),
        dataIndex: 'updated_at',
        width: 180,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 100,
        fixed: 'right',
        render: (_, record) => (
          <Button
            size="small"
            icon={<EyeOutlined />}
            onClick={() => setDetail({ titleKey: 'pluginDiagnostics.oauthDetails', value: record })}
          >
            {t('common.view')}
          </Button>
        ),
      },
    ],
    [t],
  );

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('pluginDiagnostics.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('pluginDiagnostics.description')}</Typography.Text>
        </Space>
        <Button
          icon={<ReloadOutlined />}
          disabled={!deviceId}
          onClick={() => {
            installedQuery.refetch();
            if (selectedInstallation) {
              oauthQuery.refetch();
            }
          }}
        >
          {t('pluginDiagnostics.refresh')}
        </Button>
      </div>
      <Alert
        className="prompt-page-notice"
        type="info"
        showIcon
        message={t('pluginDiagnostics.ownerNotice')}
      />
      <Form
        form={form}
        layout="inline"
        onFinish={(values) => {
          const nextDeviceId = optionalText(values.device_id) || null;
          setDeviceId(nextDeviceId);
          setSelectedInstallation(null);
        }}
      >
        <Form.Item
          name="device_id"
          rules={[{ required: true, message: t('pluginDiagnostics.deviceRequired') }]}
        >
          <Input allowClear placeholder={t('pluginDiagnostics.deviceId')} />
        </Form.Item>
        <Form.Item>
          <Button type="primary" htmlType="submit" icon={<SearchOutlined />}>
            {t('pluginDiagnostics.load')}
          </Button>
        </Form.Item>
      </Form>
      <Table
        rowKey="id"
        className="diagnostics-table"
        columns={installationColumns}
        dataSource={installedQuery.data?.items || []}
        loading={installedQuery.isLoading || installedQuery.isFetching}
        scroll={{ x: 1430 }}
        expandable={{
          expandedRowRender: (record) => (
            <Table
              rowKey={(component) => `${record.id}:${component.component_key}`}
              columns={componentColumns}
              dataSource={record.component_statuses}
              pagination={false}
              size="small"
              scroll={{ x: 980 }}
            />
          ),
          rowExpandable: (record) => record.component_statuses.length > 0,
        }}
        pagination={{
          pageSize: 50,
          total: installedQuery.data?.total || 0,
          showSizeChanger: false,
        }}
      />
      <div className="panel-toolbar diagnostics-panel-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={4}>{t('pluginDiagnostics.oauthTitle')}</Typography.Title>
          <Typography.Text type="secondary">
            {selectedInstallation
              ? t('pluginDiagnostics.oauthDescriptionSelected')
              : t('pluginDiagnostics.oauthDescriptionEmpty')}
          </Typography.Text>
        </Space>
      </div>
      <Table
        rowKey="id"
        columns={oauthColumns}
        dataSource={oauthQuery.data?.items || []}
        loading={oauthQuery.isLoading || oauthQuery.isFetching}
        scroll={{ x: 1330 }}
        pagination={{
          pageSize: 50,
          total: oauthQuery.data?.total || 0,
          showSizeChanger: false,
        }}
      />
      <Modal
        title={detail ? t(detail.titleKey) : ''}
        open={Boolean(detail)}
        onCancel={() => setDetail(null)}
        footer={null}
        width={820}
        destroyOnClose
      >
        <Input.TextArea
          className="code-input"
          rows={18}
          readOnly
          value={detail ? JSON.stringify(detail.value, null, 2) : ''}
        />
      </Modal>
    </div>
  );
}
