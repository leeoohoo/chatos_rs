// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  ApiOutlined,
  DownloadOutlined,
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

type RedactedDiagnosticsExport = {
  schema_version: 1;
  generated_at: string;
  device_id_redacted: true;
  installation_count: number;
  oauth_connection_count: number;
  selected_oauth_plugin_id?: string;
  installations: Array<{
    plugin_id: string;
    release_id: string;
    version: string;
    platform: string;
    install_status: PluginInstallStatus;
    availability_status: PluginAvailabilityStatus;
    dependency_status: PluginRequirementStatus;
    permission_status: PluginRequirementStatus;
    auth_status: PluginRequirementStatus;
    active: boolean;
    previous_release_id?: string | null;
    installed_at: string;
    last_checked_at: string;
    has_last_error: boolean;
    component_statuses: Array<{
      component_key: string;
      kind: string;
      availability_status: PluginAvailabilityStatus;
      last_checked_at: string;
      has_last_error: boolean;
    }>;
  }>;
  oauth_connections: Array<{
    plugin_id: string;
    release_id: string;
    component_key: string;
    provider: string;
    scopes: string[];
    connected: boolean;
    expires_at?: string | null;
    updated_at: string;
    has_account_display: boolean;
  }>;
};

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

function redactedDiagnosticsExport(
  installations: PluginInstallationRecord[],
  oauthConnections: PluginOAuthConnectionRecord[],
  selectedOauthPluginId?: string,
): RedactedDiagnosticsExport {
  return {
    schema_version: 1,
    generated_at: new Date().toISOString(),
    device_id_redacted: true,
    installation_count: installations.length,
    oauth_connection_count: oauthConnections.length,
    selected_oauth_plugin_id: selectedOauthPluginId,
    installations: installations.map((record) => ({
      plugin_id: record.plugin_id,
      release_id: record.release_id,
      version: record.version,
      platform: record.platform,
      install_status: record.install_status,
      availability_status: record.availability_status,
      dependency_status: record.dependency_status,
      permission_status: record.permission_status,
      auth_status: record.auth_status,
      active: record.active,
      previous_release_id: record.previous_release_id,
      installed_at: record.installed_at,
      last_checked_at: record.last_checked_at,
      has_last_error: Boolean(record.last_error),
      component_statuses: record.component_statuses.map((component) => ({
        component_key: component.component_key,
        kind: component.kind,
        availability_status: component.availability_status,
        last_checked_at: component.last_checked_at,
        has_last_error: Boolean(component.last_error),
      })),
    })),
    oauth_connections: oauthConnections.map((record) => ({
      plugin_id: record.plugin_id,
      release_id: record.release_id,
      component_key: record.component_key,
      provider: record.provider,
      scopes: record.scopes,
      connected: record.connected,
      expires_at: record.expires_at,
      updated_at: record.updated_at,
      has_account_display: Boolean(record.account_display),
    })),
  };
}

function downloadJson(filename: string, value: unknown): void {
  const blob = new Blob([JSON.stringify(value, null, 2)], {
    type: 'application/json;charset=utf-8',
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
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
  const installedItems = installedQuery.data?.items || [];
  const oauthItems = oauthQuery.data?.items || [];

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
        <Space>
          <Button
            icon={<DownloadOutlined />}
            disabled={!installedItems.length}
            onClick={() =>
              downloadJson(
                `plugin-diagnostics-${new Date().toISOString().replace(/:/g, '-')}.json`,
                redactedDiagnosticsExport(
                  installedItems,
                  oauthItems,
                  selectedInstallation?.plugin_id,
                ),
              )
            }
          >
            {t('pluginDiagnostics.exportRedacted')}
          </Button>
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
        </Space>
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
        dataSource={installedItems}
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
        dataSource={oauthItems}
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
