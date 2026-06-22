import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useSearchParams } from 'react-router-dom';
import {
  Form,
  Modal,
  Space,
  message,
} from 'antd';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import type {
  RemoteServerAuthType,
  RemoteServerRecord,
  RemoteServerTestResponse,
  UpdateRemoteServerPayload,
} from '../types';
import { ServerDetailDrawer } from './servers/ServerDetailDrawer';
import { ServerEditorDrawer } from './servers/ServerEditorDrawer';
import { ServerListTable } from './servers/ServerListTable';
import { ServerListToolbar } from './servers/ServerListToolbar';
import { ServerStatsBar } from './servers/ServerStatsBar';
import { ServerTestResultModal } from './servers/ServerTestResultModal';
import {
  buildRemoteServerPayload,
  buildRemoteServerTestPayload,
  getAuthTypeLabel,
  normalizeAuthType,
  normalizeHostKeyPolicy,
  type RemoteServerFormValues,
} from './servers/serverPageUtils';
import { useServersPageData } from './servers/useServersPageData';

export function ServersPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const [messageApi, contextHolder] = message.useMessage();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingServer, setEditingServer] = useState<RemoteServerRecord | null>(null);
  const [testResult, setTestResult] = useState<RemoteServerTestResponse | null>(null);
  const [keywordFilter, setKeywordFilter] = useState('');
  const [authTypeFilter, setAuthTypeFilter] = useState<'all' | RemoteServerAuthType>('all');
  const [enabledFilter, setEnabledFilter] = useState<'all' | 'enabled' | 'disabled'>('all');
  const [testingServerId, setTestingServerId] = useState<string | null>(null);
  const [form] = Form.useForm<RemoteServerFormValues>();
  const routeServerId = searchParams.get('server_id') || undefined;
  const authType = Form.useWatch('auth_type', form) || 'password';
  const {
    authTypeOptions,
    authTypeFilterOptions,
    enabledFilterOptions,
    serversQuery,
    selectedServerQuery,
    selectedServer,
    filteredServers,
    stats,
  } = useServersPageData({
    t,
    routeServerId,
    keywordFilter,
    authTypeFilter,
    enabledFilter,
  });
  const authTypeLabel = (value: string) => getAuthTypeLabel(value, t);

  const createServerMutation = useMutation({
    mutationFn: api.createRemoteServer,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['remote-servers'] }),
        queryClient.invalidateQueries({ queryKey: ['mcp-catalog'] }),
      ]);
      messageApi.success(t('servers.created'));
      closeEditor();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const updateServerMutation = useMutation({
    mutationFn: ({
      id,
      payload,
    }: {
      id: string;
      payload: UpdateRemoteServerPayload;
    }) => api.updateRemoteServer(id, payload),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['remote-servers'] }),
        queryClient.invalidateQueries({ queryKey: ['remote-server'] }),
        queryClient.invalidateQueries({ queryKey: ['mcp-catalog'] }),
      ]);
      messageApi.success(t('servers.updated'));
      closeEditor();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const deleteServerMutation = useMutation({
    mutationFn: api.deleteRemoteServer,
    onSuccess: async (_, id) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['remote-servers'] }),
        queryClient.invalidateQueries({ queryKey: ['remote-server'] }),
        queryClient.invalidateQueries({ queryKey: ['mcp-catalog'] }),
      ]);
      if (routeServerId === id) {
        closeDetailDrawer();
      }
      messageApi.success(t('servers.deleted'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const testDraftMutation = useMutation({
    mutationFn: api.testRemoteServerDraft,
    onSuccess: async (result) => {
      setTestResult(result);
      await queryClient.invalidateQueries({ queryKey: ['remote-servers'] });
      if (result.ok) {
        messageApi.success(t('servers.draftTestSuccess'));
      } else {
        messageApi.warning(t('servers.draftTestFailed'));
      }
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const testSavedMutation = useMutation({
    mutationFn: (id: string) => api.testRemoteServer(id),
    onMutate: (id: string) => {
      setTestingServerId(id);
    },
    onSuccess: async (result) => {
      setTestResult(result);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['remote-servers'] }),
        queryClient.invalidateQueries({ queryKey: ['remote-server'] }),
      ]);
      if (result.ok) {
        messageApi.success(t('servers.testSuccess'));
      } else {
        messageApi.warning(t('servers.testFailed'));
      }
    },
    onError: (error: Error) => messageApi.error(error.message),
    onSettled: () => {
      setTestingServerId(null);
    },
  });

  function openCreateDrawer() {
    setEditingServer(null);
    form.setFieldsValue({
      name: '',
      host: '',
      port: 22,
      username: '',
      auth_type: 'password',
      password: '',
      private_key_path: '',
      certificate_path: '',
      default_remote_path: '',
      host_key_policy: 'accept_new',
      enabled: true,
    });
    setDrawerOpen(true);
  }

  function openEditDrawer(server: RemoteServerRecord) {
    setEditingServer(server);
    form.setFieldsValue({
      name: server.name,
      host: server.host,
      port: server.port,
      username: server.username,
      auth_type: normalizeAuthType(server.auth_type),
      password: server.password || '',
      private_key_path: server.private_key_path || '',
      certificate_path: server.certificate_path || '',
      default_remote_path: server.default_remote_path || '',
      host_key_policy: normalizeHostKeyPolicy(server.host_key_policy),
      enabled: server.enabled,
    });
    setDrawerOpen(true);
  }

  function closeEditor() {
    setDrawerOpen(false);
    setEditingServer(null);
    form.resetFields();
  }

  function openDetailDrawer(serverId: string) {
    const next = new URLSearchParams(searchParams);
    next.set('server_id', serverId);
    setSearchParams(next);
  }

  function closeDetailDrawer() {
    const next = new URLSearchParams(searchParams);
    next.delete('server_id');
    setSearchParams(next);
  }

  function confirmDelete(server: RemoteServerRecord) {
    Modal.confirm({
      title: t('servers.deleteConfirmTitle', { name: server.name }),
      content: t('servers.deleteConfirmContent'),
      okButtonProps: { danger: true },
      onOk: () => deleteServerMutation.mutate(server.id),
    });
  }

  function handleSubmit(values: RemoteServerFormValues) {
    const payload = buildRemoteServerPayload(values);
    if (editingServer) {
      updateServerMutation.mutate({ id: editingServer.id, payload });
      return;
    }
    createServerMutation.mutate(payload);
  }

  async function handleDraftTest() {
    const values = await form.validateFields();
    const payload = buildRemoteServerTestPayload(values);
    testDraftMutation.mutate(payload);
  }

  return (
    <>
      {contextHolder}
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <ServerListToolbar
          t={t}
          keywordFilter={keywordFilter}
          authTypeFilter={authTypeFilter}
          enabledFilter={enabledFilter}
          authTypeFilterOptions={authTypeFilterOptions}
          enabledFilterOptions={enabledFilterOptions}
          onKeywordFilterChange={setKeywordFilter}
          onAuthTypeFilterChange={setAuthTypeFilter}
          onEnabledFilterChange={setEnabledFilter}
          onClearFilters={() => {
            setKeywordFilter('');
            setAuthTypeFilter('all');
            setEnabledFilter('all');
          }}
          onRefresh={() => serversQuery.refetch()}
          onCreate={openCreateDrawer}
        />

        <ServerStatsBar
          t={t}
          visible={stats.visible}
          enabled={stats.enabled}
          testPassed={stats.testPassed}
          strict={stats.strict}
        />

        <ServerListTable
          t={t}
          servers={filteredServers}
          loading={serversQuery.isLoading}
          testing={testSavedMutation.isPending}
          testingServerId={testingServerId}
          onOpenDetail={openDetailDrawer}
          onOpenEdit={openEditDrawer}
          onTest={(serverId) => testSavedMutation.mutate(serverId)}
          onDelete={confirmDelete}
        />
      </Space>

      <ServerEditorDrawer
        t={t}
        open={drawerOpen}
        editingServer={editingServer}
        form={form}
        authType={authType}
        authTypeOptions={authTypeOptions}
        saving={createServerMutation.isPending || updateServerMutation.isPending}
        testingDraft={testDraftMutation.isPending}
        onClose={closeEditor}
        onDraftTest={handleDraftTest}
        onSubmit={handleSubmit}
      />

      <ServerDetailDrawer
        t={t}
        open={Boolean(routeServerId)}
        loading={selectedServerQuery.isLoading}
        server={selectedServer}
        testing={Boolean(
          selectedServer &&
            testSavedMutation.isPending &&
            testingServerId === selectedServer.id,
        )}
        authTypeLabel={authTypeLabel}
        onClose={closeDetailDrawer}
        onTest={(serverId) => testSavedMutation.mutate(serverId)}
        onEdit={(server) => {
          closeDetailDrawer();
          openEditDrawer(server);
        }}
        onDelete={confirmDelete}
      />

      <ServerTestResultModal
        t={t}
        result={testResult}
        authTypeLabel={authTypeLabel}
        onClose={() => setTestResult(null)}
      />
    </>
  );
}
