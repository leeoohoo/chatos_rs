// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { DeleteOutlined, LinkOutlined, ReloadOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Alert,
  Button,
  Form,
  Input,
  Popconfirm,
  Select,
  Space,
  Table,
  Tag,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useEffect, useMemo, useRef, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { useI18n } from '../i18n/I18nProvider';
import type {
  PluginCloudOAuthConnectionRecord,
  PluginComponentDescriptor,
  PluginMcpCloudRuntimeMetadata,
  PluginReleaseRecord,
} from '../pluginTypes';

type OAuthFormValues = {
  provider: string;
  authorization_server?: string;
  client_id?: string;
  client_secret?: string;
  token_endpoint_auth_method?: string;
};

type OAuthPopupMessage = {
  type?: string;
  ok?: boolean;
  connection_id?: string;
  message?: string;
};

function oauthScopesByProvider(
  release: PluginReleaseRecord | undefined,
  componentKey: string | undefined,
): Map<string, string[]> {
  const groups = new Map<string, Set<string>>();
  if (!release || !componentKey) {
    return new Map();
  }
  const component = release.components.find((item) => item.component_key === componentKey);
  const requirements = [
    ...release.permissions.filter(
      (permission) =>
        permission.components.length === 0 || permission.components.includes(componentKey),
    ),
    ...(component?.permissions || []),
  ];
  requirements.forEach((requirement) => {
    const prefix = 'oauth.scope:';
    if (!requirement.permission.startsWith(prefix)) {
      return;
    }
    const value = requirement.permission.slice(prefix.length);
    const separator = value.indexOf(':');
    if (separator <= 0 || separator === value.length - 1) {
      return;
    }
    const provider = value.slice(0, separator);
    const scope = value.slice(separator + 1);
    const scopes = groups.get(provider) || new Set<string>();
    scopes.add(scope);
    groups.set(provider, scopes);
  });
  return new Map(
    [...groups.entries()].map(([provider, scopes]) => [provider, [...scopes].sort()]),
  );
}

function oauthComponents(
  release: PluginReleaseRecord | undefined,
  runtimes: PluginMcpCloudRuntimeMetadata[],
): PluginComponentDescriptor[] {
  if (!release) {
    return [];
  }
  const oauthRuntimeKeys = new Set(
    runtimes
      .filter((runtime) => runtime.transport === 'http' && Boolean(runtime.oauth_resource))
      .map((runtime) => runtime.component_key),
  );
  return release.components.filter(
    (component) =>
      component.kind === 'mcp_server' &&
      component.execution_host !== 'local' &&
      oauthRuntimeKeys.has(component.component_key) &&
      oauthScopesByProvider(release, component.component_key).size > 0,
  );
}

function connectionStatusColor(connection: PluginCloudOAuthConnectionRecord): string {
  if (connection.connected && !connection.needs_auth) {
    return 'green';
  }
  if (connection.needs_auth) {
    return 'red';
  }
  return 'default';
}

export function PluginCloudOAuthPage() {
  const { t } = useI18n();
  const [form] = Form.useForm<OAuthFormValues>();
  const queryClient = useQueryClient();
  const [pluginId, setPluginId] = useState<string>();
  const [releaseId, setReleaseId] = useState<string>();
  const [componentKey, setComponentKey] = useState<string>();
  const expectedCallbackOrigin = useRef<string | undefined>(undefined);
  const activePopup = useRef<Window | null>(null);

  const pluginsQuery = useQuery({
    queryKey: ['visible-plugin-catalog'],
    queryFn: () => api.listPluginCatalog({ limit: 500 }),
  });
  const releasesQuery = useQuery({
    queryKey: ['visible-plugin-releases', pluginId],
    queryFn: () => api.listVisiblePluginReleases(pluginId || ''),
    enabled: Boolean(pluginId),
  });
  const selectedRelease = releasesQuery.data?.items.find((release) => release.id === releaseId);
  const runtimesQuery = useQuery({
    queryKey: ['plugin-cloud-mcp-runtimes', pluginId, releaseId],
    queryFn: () => api.listPluginMcpCloudRuntimes(pluginId || '', releaseId || ''),
    enabled: Boolean(pluginId && releaseId),
  });
  const components = useMemo(
    () => oauthComponents(selectedRelease, runtimesQuery.data?.items || []),
    [runtimesQuery.data, selectedRelease],
  );
  const scopeGroups = useMemo(
    () => oauthScopesByProvider(selectedRelease, componentKey),
    [selectedRelease, componentKey],
  );
  const provider = Form.useWatch('provider', form);
  const selectedScopes = scopeGroups.get(provider || '') || [];
  const connectionsQuery = useQuery({
    queryKey: ['plugin-cloud-oauth', pluginId, releaseId, componentKey],
    queryFn: () =>
      api.listPluginCloudOAuthConnections(pluginId || '', {
        release_id: releaseId || '',
        component_key: componentKey || '',
      }),
    enabled: Boolean(pluginId && releaseId && componentKey),
  });

  useEffect(() => {
    if (!pluginId && pluginsQuery.data?.items[0]?.id) {
      setPluginId(pluginsQuery.data.items[0].id);
    }
  }, [pluginId, pluginsQuery.data]);

  useEffect(() => {
    const activeReleases = (releasesQuery.data?.items || []).filter(
      (release) => !release.revoked_at,
    );
    if (!activeReleases.some((release) => release.id === releaseId)) {
      setReleaseId(activeReleases[0]?.id);
    }
  }, [releaseId, releasesQuery.data]);

  useEffect(() => {
    if (!components.some((component) => component.component_key === componentKey)) {
      setComponentKey(components[0]?.component_key);
    }
  }, [componentKey, components]);

  useEffect(() => {
    const providers = [...scopeGroups.keys()];
    if (!providers.includes(form.getFieldValue('provider'))) {
      form.setFieldValue('provider', providers[0]);
    }
  }, [form, scopeGroups]);

  useEffect(() => {
    const onMessage = (event: MessageEvent<OAuthPopupMessage>) => {
      if (
        !expectedCallbackOrigin.current ||
        event.origin !== expectedCallbackOrigin.current ||
        event.data?.type !== 'chatos-plugin-cloud-oauth'
      ) {
        return;
      }
      expectedCallbackOrigin.current = undefined;
      activePopup.current?.close();
      activePopup.current = null;
      queryClient.invalidateQueries({
        queryKey: ['plugin-cloud-oauth', pluginId, releaseId, componentKey],
      });
      if (event.data.ok) {
        message.success(t('pluginCloudOAuth.completed'));
      } else {
        message.error(event.data.message || t('pluginCloudOAuth.failed'));
      }
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, [componentKey, pluginId, queryClient, releaseId, t]);

  const authorizeMutation = useMutation({
    mutationFn: async (popup: Window) => {
      if (!pluginId || !releaseId || !componentKey) {
        throw new Error(t('pluginCloudOAuth.selectionRequired'));
      }
      const values = form.getFieldsValue();
      const request = api.beginPluginCloudOAuthAuthorization(
        pluginId,
        releaseId,
        componentKey,
        {
          provider: values.provider,
          scopes: selectedScopes,
          authorization_server: values.authorization_server?.trim() || undefined,
          client_id: values.client_id?.trim() || undefined,
          client_secret: values.client_secret || undefined,
          token_endpoint_auth_method: values.token_endpoint_auth_method || undefined,
        },
      );
      form.setFieldValue('client_secret', undefined);
      const result = await request;
      expectedCallbackOrigin.current = result.callback_origin;
      activePopup.current = popup;
      popup.location.replace(result.authorization_url);
      popup.focus();
    },
    onError: (error, popup) => {
      form.setFieldValue('client_secret', undefined);
      popup.close();
      message.error((error as Error).message);
    },
  });
  const deleteMutation = useMutation({
    mutationFn: (connectionId: string) =>
      api.deletePluginCloudOAuthConnection(pluginId || '', connectionId),
    onSuccess: () => {
      message.success(t('pluginCloudOAuth.deleted'));
      queryClient.invalidateQueries({
        queryKey: ['plugin-cloud-oauth', pluginId, releaseId, componentKey],
      });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const columns = useMemo<ColumnsType<PluginCloudOAuthConnectionRecord>>(
    () => [
      {
        title: t('pluginCloudOAuth.provider'),
        dataIndex: 'provider',
        width: 150,
        render: (value, record) => (
          <Space orientation="vertical" size={0}>
            <Typography.Text strong>{value}</Typography.Text>
            <CompactId value={record.id} />
          </Space>
        ),
      },
      {
        title: t('pluginCloudOAuth.resource'),
        dataIndex: 'resource',
        ellipsis: true,
      },
      {
        title: t('pluginCloudOAuth.scopes'),
        dataIndex: 'scopes',
        width: 280,
        render: (values: string[]) => (
          <Space size={[4, 4]} wrap>
            {values.map((value) => (
              <Tag key={value}>{value}</Tag>
            ))}
          </Space>
        ),
      },
      {
        title: t('table.status'),
        key: 'status',
        width: 170,
        render: (_, record) => (
          <Space orientation="vertical" size={2}>
            <Tag color={connectionStatusColor(record)}>
              {record.needs_auth
                ? t('pluginCloudOAuth.needsAuth')
                : record.connected
                  ? t('pluginCloudOAuth.connected')
                  : t('pluginCloudOAuth.disconnected')}
            </Tag>
            {record.refreshable ? (
              <Tag color="blue">{t('pluginCloudOAuth.autoRefresh')}</Tag>
            ) : null}
          </Space>
        ),
      },
      {
        title: t('pluginCloudOAuth.expiresAt'),
        dataIndex: 'expires_at',
        width: 180,
        render: (value) => (value ? <DateTimeCell value={value} /> : '—'),
      },
      {
        title: t('table.updated'),
        dataIndex: 'updated_at',
        width: 180,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 100,
        render: (_, record) => (
          <Popconfirm
            title={t('pluginCloudOAuth.deleteConfirm')}
            onConfirm={() => deleteMutation.mutate(record.id)}
          >
            <Button danger size="small" icon={<DeleteOutlined />}>
              {t('common.delete')}
            </Button>
          </Popconfirm>
        ),
      },
    ],
    [deleteMutation, t],
  );

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space orientation="vertical" size={0}>
          <Typography.Title level={3}>{t('pluginCloudOAuth.title')}</Typography.Title>
          <Typography.Text type="secondary">
            {t('pluginCloudOAuth.description')}
          </Typography.Text>
        </Space>
        <Button
          icon={<ReloadOutlined />}
          onClick={() => connectionsQuery.refetch()}
          loading={connectionsQuery.isFetching}
        >
          {t('pluginCloudOAuth.refresh')}
        </Button>
      </div>

      <Alert
        type="info"
        showIcon
        title={t('pluginCloudOAuth.securityNotice')}
      />

      <div className="card-block">
        <div className="form-grid">
          <Form.Item label={t('pluginCloudOAuth.plugin')}>
            <Select
              showSearch
              value={pluginId}
              loading={pluginsQuery.isLoading}
              onChange={(value) => {
                setPluginId(value);
                setReleaseId(undefined);
                setComponentKey(undefined);
              }}
              options={(pluginsQuery.data?.items || []).map((plugin) => ({
                value: plugin.id,
                label: `${plugin.display_name} (${plugin.name})`,
              }))}
            />
          </Form.Item>
          <Form.Item label={t('pluginCloudOAuth.release')}>
            <Select
              value={releaseId}
              loading={releasesQuery.isLoading}
              onChange={(value) => {
                setReleaseId(value);
                setComponentKey(undefined);
              }}
              options={(releasesQuery.data?.items || [])
                .filter((release) => !release.revoked_at)
                .map((release) => ({
                  value: release.id,
                  label: `${release.version} · ${release.release_channel}`,
                }))}
            />
          </Form.Item>
          <Form.Item label={t('pluginCloudOAuth.component')}>
            <Select
              value={componentKey}
              onChange={setComponentKey}
              options={components.map((component) => ({
                value: component.component_key,
                label: `${component.display_name} (${component.component_key})`,
              }))}
            />
          </Form.Item>
        </div>

        <Form
          form={form}
          layout="vertical"
          onFinish={() => {
            const popup = window.open(
              'about:blank',
              'chatos-plugin-cloud-oauth',
              'popup=yes,width=720,height=760,resizable=yes,scrollbars=yes',
            );
            if (!popup) {
              message.error(t('pluginCloudOAuth.popupBlocked'));
              return;
            }
            authorizeMutation.mutate(popup);
          }}
        >
          <div className="form-grid">
            <Form.Item
              name="provider"
              label={t('pluginCloudOAuth.provider')}
              rules={[{ required: true }]}
            >
              <Select
                options={[...scopeGroups.keys()].map((value) => ({ value, label: value }))}
              />
            </Form.Item>
            <Form.Item
              name="authorization_server"
              label={t('pluginCloudOAuth.authorizationServer')}
              tooltip={t('pluginCloudOAuth.authorizationServerHint')}
            >
              <Input placeholder="https://auth.example.com" />
            </Form.Item>
            <Form.Item name="client_id" label={t('pluginCloudOAuth.clientId')}>
              <Input autoComplete="off" />
            </Form.Item>
            <Form.Item name="client_secret" label={t('pluginCloudOAuth.clientSecret')}>
              <Input.Password autoComplete="new-password" />
            </Form.Item>
            <Form.Item
              name="token_endpoint_auth_method"
              label={t('pluginCloudOAuth.tokenAuthMethod')}
            >
              <Select
                allowClear
                options={[
                  { value: 'none', label: 'none' },
                  { value: 'client_secret_basic', label: 'client_secret_basic' },
                  { value: 'client_secret_post', label: 'client_secret_post' },
                ]}
              />
            </Form.Item>
          </div>
          <Space orientation="vertical" size={8}>
            <Typography.Text type="secondary">
              {t('pluginCloudOAuth.signedScopes')}
            </Typography.Text>
            <Space size={[4, 4]} wrap>
              {selectedScopes.map((scope) => (
                <Tag key={scope} color="blue">
                  {scope}
                </Tag>
              ))}
            </Space>
            <Button
              type="primary"
              htmlType="submit"
              icon={<LinkOutlined />}
              loading={authorizeMutation.isPending}
              disabled={!pluginId || !releaseId || !componentKey || selectedScopes.length === 0}
            >
              {t('pluginCloudOAuth.authorize')}
            </Button>
          </Space>
        </Form>
      </div>

      <Table
        rowKey="id"
        columns={columns}
        dataSource={connectionsQuery.data?.items || []}
        loading={connectionsQuery.isLoading}
        scroll={{ x: 1350 }}
        pagination={{ pageSize: 10 }}
      />
    </div>
  );
}
