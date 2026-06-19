import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Form,
  Modal,
  Space,
  message,
} from 'antd';

import { api } from '../../api/client';
import { useI18n } from '../../i18n/I18nProvider';
import type {
  ExternalMcpConfigRecord,
  UpdateExternalMcpConfigPayload,
} from '../../types';
import {
  buildExternalMcpConfigPayload,
  type ExternalMcpConfigFormValues,
} from './mcpCatalogPageUtils';
import { ExternalMcpConfigDrawer } from './ExternalMcpConfigDrawer';
import { ExternalMcpConfigListSection } from './ExternalMcpConfigListSection';
import { ExternalMcpConfigRoadmap } from './ExternalMcpConfigRoadmap';

export function ExternalMcpConfigTab() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [messageApi, contextHolder] = message.useMessage();
  const [form] = Form.useForm<ExternalMcpConfigFormValues>();
  const transport = Form.useWatch('transport', form) || 'stdio';
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingConfig, setEditingConfig] = useState<ExternalMcpConfigRecord | null>(null);
  const configsQuery = useQuery({
    queryKey: ['external-mcp-configs'],
    queryFn: api.listExternalMcpConfigs,
  });
  const createMutation = useMutation({
    mutationFn: api.createExternalMcpConfig,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['external-mcp-configs'] });
      messageApi.success(t('mcpCatalog.externalConfigCreated'));
      closeDrawer();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const updateMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: UpdateExternalMcpConfigPayload }) =>
      api.updateExternalMcpConfig(id, payload),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['external-mcp-configs'] });
      messageApi.success(t('mcpCatalog.externalConfigUpdated'));
      closeDrawer();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const deleteMutation = useMutation({
    mutationFn: api.deleteExternalMcpConfig,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['external-mcp-configs'] });
      messageApi.success(t('mcpCatalog.externalConfigDeleted'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  function openCreateDrawer() {
    setEditingConfig(null);
    form.setFieldsValue({
      name: '',
      transport: 'stdio',
      command: '',
      argsText: '',
      url: '',
      headersText: '{}',
      envText: '{}',
      cwd: '',
      enabled: true,
    });
    setDrawerOpen(true);
  }

  function openEditDrawer(config: ExternalMcpConfigRecord) {
    setEditingConfig(config);
    form.setFieldsValue({
      name: config.name,
      transport: config.transport === 'http' ? 'http' : 'stdio',
      command: config.command || '',
      argsText: (config.args || []).join('\n'),
      url: config.url || '',
      headersText: JSON.stringify(config.headers || {}, null, 2),
      envText: JSON.stringify(config.env || {}, null, 2),
      cwd: config.cwd || '',
      enabled: config.enabled,
    });
    setDrawerOpen(true);
  }

  function closeDrawer() {
    setDrawerOpen(false);
    setEditingConfig(null);
    form.resetFields();
  }

  function confirmDelete(config: ExternalMcpConfigRecord) {
    Modal.confirm({
      title: t('mcpCatalog.externalConfigDeleteTitle', { name: config.name }),
      content: t('mcpCatalog.externalConfigDeleteContent'),
      okButtonProps: { danger: true },
      onOk: () => deleteMutation.mutate(config.id),
    });
  }

  function handleSubmit(values: ExternalMcpConfigFormValues) {
    let payload: ReturnType<typeof buildExternalMcpConfigPayload>;
    try {
      payload = buildExternalMcpConfigPayload(values);
    } catch (error) {
      messageApi.error(error instanceof Error ? error.message : String(error));
      return;
    }

    if (editingConfig) {
      updateMutation.mutate({ id: editingConfig.id, payload });
    } else {
      createMutation.mutate(payload);
    }
  }

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      {contextHolder}

      <ExternalMcpConfigListSection
        t={t}
        configs={configsQuery.data || []}
        loading={configsQuery.isLoading}
        onCreate={openCreateDrawer}
        onEdit={openEditDrawer}
        onDelete={confirmDelete}
      />

      <ExternalMcpConfigRoadmap t={t} />

      <ExternalMcpConfigDrawer
        t={t}
        open={drawerOpen}
        editingConfig={editingConfig}
        form={form}
        transport={transport}
        saving={createMutation.isPending || updateMutation.isPending}
        onClose={closeDrawer}
        onSubmit={handleSubmit}
      />
    </Space>
  );
}
