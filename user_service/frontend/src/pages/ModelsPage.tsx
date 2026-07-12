// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, App, Button, Card, Empty, Form, Select, Space, Table, Typography } from 'antd';
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons';

import { api } from '../api/client';
import type {
  UserModelConfigRecord,
  UserModelProviderRecord,
  UserSummaryRecord,
} from '../types';
import { ModelProviderDrawer } from './models/ModelProviderDrawer';
import {
  ALL_USERS_SCOPE,
  buildCreateProviderPayload,
  buildModelColumns,
  buildProviderColumns,
  buildUpdateProviderPayload,
  type ProviderFormValues,
} from './models/modelPageUtils';

export function ModelsPage() {
  const { message } = App.useApp();
  const queryClient = useQueryClient();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<UserModelProviderRecord | null>(null);
  const [selectedUserId, setSelectedUserId] = useState<string>();
  const [form] = Form.useForm<ProviderFormValues>();

  const currentUserQuery = useQuery({
    queryKey: ['current-user'],
    queryFn: () => api.currentUser(),
  });
  const usersQuery = useQuery({
    queryKey: ['users'],
    queryFn: () => api.listUsers(),
  });

  const currentUser = currentUserQuery.data?.user;
  const isSuperAdmin = currentUser?.role === 'super_admin';

  useEffect(() => {
    if (!isSuperAdmin && !selectedUserId && currentUser?.id) {
      setSelectedUserId(currentUser.id);
    }
  }, [currentUser?.id, isSuperAdmin, selectedUserId]);

  const scopedUserId = selectedUserId;
  const scopedQueryKey = scopedUserId || ALL_USERS_SCOPE;
  const canLoadModelData = Boolean(currentUser) && (isSuperAdmin || Boolean(scopedUserId));

  const providersQuery = useQuery({
    queryKey: ['model-providers', scopedQueryKey],
    queryFn: () => api.listModelProviders(scopedUserId),
    enabled: canLoadModelData,
  });

  const modelConfigsQuery = useQuery({
    queryKey: ['model-configs', scopedQueryKey],
    queryFn: () => api.listModelConfigs(scopedUserId),
    enabled: canLoadModelData,
  });

  const modelSettingsQuery = useQuery({
    queryKey: ['model-settings', selectedUserId],
    queryFn: () => api.getModelSettings(selectedUserId || ''),
    enabled: Boolean(selectedUserId),
  });

  const createProviderMutation = useMutation({
    mutationFn: api.createModelProvider,
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Provider saved');
      closeDrawer();
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const updateProviderMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: ReturnType<typeof buildUpdateProviderPayload> }) =>
      api.updateModelProvider(id, payload),
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Provider updated');
      closeDrawer();
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const refreshProviderMutation = useMutation({
    mutationFn: (provider: UserModelProviderRecord) => api.refreshModelProvider(provider.id, {}),
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Provider models refreshed');
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const deleteProviderMutation = useMutation({
    mutationFn: (id: string) => api.deleteModelProvider(id),
    onSuccess: async () => {
      message.success('Provider deleted');
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const saveSettingsMutation = useMutation({
    mutationFn: (modelConfigId: string | null) =>
      api.updateModelSettings({
        user_id: selectedUserId,
        memory_summary_model_config_id: modelConfigId,
      }),
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Memory summary model saved');
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const userOptions = useMemo(
    () =>
      (usersQuery.data || []).map((item: UserSummaryRecord) => ({
        label: `${item.display_name || item.username} (${item.username})`,
        value: item.id,
      })),
    [usersQuery.data],
  );

  const currentProviders = providersQuery.data || [];
  const currentConfigs = modelConfigsQuery.data || [];
  const memoryEligibleConfigs = selectedUserId
    ? currentConfigs.filter((item) => item.owner_user_id === selectedUserId && item.model_name.trim())
    : [];
  const providerColumns = useMemo(
    () =>
      buildProviderColumns({
        users: usersQuery.data,
        onRefresh: (record) => refreshProviderMutation.mutate(record),
        onEdit: openEditDrawer,
        onDelete: (id) => deleteProviderMutation.mutate(id),
        deleteLoading: deleteProviderMutation.isPending,
      }),
    [deleteProviderMutation, refreshProviderMutation, usersQuery.data],
  );
  const modelColumns = useMemo(() => buildModelColumns(usersQuery.data), [usersQuery.data]);

  function showWarnings(warnings?: string[]) {
    if (!warnings || warnings.length === 0) {
      return;
    }
    message.warning(warnings.join(' | '), 6);
  }

  function showError(error: unknown) {
    message.error(error instanceof Error ? error.message : 'Operation failed');
  }

  async function invalidateCurrentUserModelQueries() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['model-providers'] }),
      queryClient.invalidateQueries({ queryKey: ['model-configs'] }),
      queryClient.invalidateQueries({ queryKey: ['model-settings'] }),
    ]);
  }

  function openCreateDrawer() {
    setEditingProvider(null);
    form.resetFields();
    form.setFieldsValue({
      owner_user_id: selectedUserId,
      provider: 'gpt',
      enabled: true,
      supports_images: false,
      supports_reasoning: false,
      supports_responses: true,
      clear_api_key: false,
    });
    setDrawerOpen(true);
  }

  function openEditDrawer(record: UserModelProviderRecord) {
    setEditingProvider(record);
    form.setFieldsValue({
      owner_user_id: record.owner_user_id,
      name: record.name,
      provider: record.provider,
      api_key: '',
      clear_api_key: false,
      base_url: record.base_url || '',
      enabled: record.enabled,
      supports_images: record.supports_images,
      supports_reasoning: record.supports_reasoning,
      supports_responses: record.supports_responses,
    });
    setDrawerOpen(true);
  }

  function closeDrawer() {
    setDrawerOpen(false);
    setEditingProvider(null);
    form.resetFields();
  }

  function submit(values: ProviderFormValues) {
    if (!selectedUserId && !values.owner_user_id) {
      message.error('Owner user is required');
      return;
    }

    if (editingProvider) {
      updateProviderMutation.mutate({
        id: editingProvider.id,
        payload: buildUpdateProviderPayload(values),
      });
      return;
    }

    createProviderMutation.mutate(
      buildCreateProviderPayload({
        values,
        isSuperAdmin,
        selectedUserId,
      }),
    );
  }

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          justifyContent: 'space-between',
          gap: 16,
          width: '100%',
        }}
      >
        <Space direction="vertical" size={0}>
          <Typography.Title level={3} style={{ margin: 0 }}>
            AI Providers & Models
          </Typography.Title>
          <Typography.Text type="secondary">
            Save provider credentials here. User service fetches concrete models from the provider
            catalog for Chat OS, Task Runner, and Memory Engine.
          </Typography.Text>
        </Space>
        <Space wrap>
          {isSuperAdmin ? (
            <Select
              value={selectedUserId || ALL_USERS_SCOPE}
              options={[{ label: 'All users', value: ALL_USERS_SCOPE }, ...userOptions]}
              onChange={(value) => setSelectedUserId(value === ALL_USERS_SCOPE ? undefined : value)}
              style={{ width: 280 }}
              placeholder="Select owner user"
            />
          ) : null}
          <Button
            icon={<ReloadOutlined />}
            onClick={() => {
              void providersQuery.refetch();
              void modelConfigsQuery.refetch();
              if (selectedUserId) {
                void modelSettingsQuery.refetch();
              }
            }}
            loading={
              providersQuery.isFetching ||
              modelConfigsQuery.isFetching ||
              modelSettingsQuery.isFetching
            }
          >
            Refresh
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreateDrawer}>
            New Provider
          </Button>
        </Space>
      </div>

      <Card title="Providers">
        <Table<UserModelProviderRecord>
          rowKey="id"
          columns={providerColumns}
          dataSource={currentProviders}
          loading={providersQuery.isLoading}
          pagination={{ pageSize: 10, showSizeChanger: true }}
          expandable={{
            expandedRowRender: (record) =>
              record.sync_warnings && record.sync_warnings.length > 0 ? (
                <Alert
                  type="warning"
                  showIcon
                  message="Refresh warnings"
                  description={record.sync_warnings.join(' | ')}
                />
              ) : null,
            rowExpandable: (record) => Boolean(record.sync_warnings?.length),
          }}
          locale={{
            emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="No provider" />,
          }}
        />
      </Card>

      <Card title="Memory Engine Summary Model">
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          {!selectedUserId ? (
            <Alert
              type="info"
              showIcon
              message="Select one user to edit memory settings"
              description="Super admin can view all providers and imported models at once, but memory summary defaults are saved per user."
            />
          ) : (
            <>
              <Typography.Text type="secondary">
                Choose the default concrete model for this user's memory summary jobs.
              </Typography.Text>
              <Space wrap style={{ width: '100%' }}>
                <Select
                  value={modelSettingsQuery.data?.memory_summary_model_config_id ?? undefined}
                  allowClear
                  style={{ minWidth: 320 }}
                  placeholder="Select summary model"
                  options={memoryEligibleConfigs.map((item) => ({
                    label: `${item.name} | ${item.model_name}`,
                    value: item.id,
                  }))}
                  onChange={(value) => saveSettingsMutation.mutate(value ?? null)}
                  loading={modelSettingsQuery.isLoading}
                />
              </Space>
              {memoryEligibleConfigs.length === 0 ? (
                <Alert
                  type="info"
                  showIcon
                  message="No concrete model available"
                  description="Create a provider and refresh its model catalog before choosing a memory summary model."
                />
              ) : null}
            </>
          )}
        </Space>
      </Card>

      <Card title="Imported Concrete Models">
        <Table<UserModelConfigRecord>
          rowKey="id"
          columns={modelColumns}
          dataSource={currentConfigs}
          loading={modelConfigsQuery.isLoading}
          pagination={{ pageSize: 10, showSizeChanger: true }}
          locale={{
            emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="No imported model" />,
          }}
        />
      </Card>

      <ModelProviderDrawer
        open={drawerOpen}
        editingProvider={editingProvider}
        isSuperAdmin={isSuperAdmin}
        userOptions={userOptions}
        form={form}
        saveLoading={createProviderMutation.isPending || updateProviderMutation.isPending}
        onClose={closeDrawer}
        onSubmit={submit}
      />
    </Space>
  );
}
