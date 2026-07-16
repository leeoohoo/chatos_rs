// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  BookOutlined,
  DeleteOutlined,
  EditOutlined,
  PlusOutlined,
  ReloadOutlined,
  ToolOutlined,
} from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Button, Form, Popconfirm, Space, Table, Typography, message } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useRef, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { EnabledTag, RuntimeKindTag, VisibilityTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import { mcpDisplayName } from '../i18n/labels';
import type { CurrentUser, McpProviderSkill, McpRecord, RuntimeKind } from '../types';
import { McpCatalogDialogs } from './mcpCatalog/McpCatalogDialogs';
import {
  adminRuntimeKinds,
  buildMcpPayload,
  isSystemManagedMcp,
  userRuntimeKinds,
} from './mcpCatalog/support';
import { jsonText } from './formUtils';

interface McpCatalogPageProps {
  user: CurrentUser;
}

export function McpCatalogPage({ user }: McpCatalogPageProps) {
  const { t } = useI18n();
  const [form] = Form.useForm();
  const [optimizeForm] = Form.useForm();
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState<McpRecord | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const [descriptorModal, setDescriptorModal] = useState<{
    record: McpRecord;
    view: 'skills' | 'tools';
  } | null>(null);
  const [activeProviderSkillId, setActiveProviderSkillId] = useState<string | null>(null);
  const [optimizeTarget, setOptimizeTarget] = useState<{
    record: McpRecord;
    skill: McpProviderSkill;
  } | null>(null);
  const [optimizedInstructions, setOptimizedInstructions] = useState('');
  const [optimizationThinking, setOptimizationThinking] = useState('');
  const [optimizeStreaming, setOptimizeStreaming] = useState(false);
  const optimizeAbortRef = useRef<AbortController | null>(null);
  const isAdmin = user.role === 'super_admin';
  const runtimeKinds = isAdmin ? adminRuntimeKinds : userRuntimeKinds;
  const runtimeKind = Form.useWatch('runtime_kind', form) as RuntimeKind | undefined;
  const editingSystemManaged = editing ? isSystemManagedMcp(editing) : false;

  const mcpsQuery = useQuery({
    queryKey: ['mcps', isAdmin],
    queryFn: () => api.listMcps({ include_system: isAdmin, limit: 500 }),
  });

  const descriptorQuery = useQuery({
    queryKey: ['mcp-descriptor', descriptorModal?.record.id],
    queryFn: () => api.getMcpDescriptor(descriptorModal!.record.id),
    enabled: Boolean(descriptorModal),
  });

  const aiModelsQuery = useQuery({
    queryKey: ['admin-ai-models'],
    queryFn: api.listAdminAiModels,
    enabled: isAdmin && Boolean(optimizeTarget),
  });

  const saveOptimizedSkillMutation = useMutation({
    mutationFn: () =>
      api.updateMcpProviderSkill(
        optimizeTarget!.record.id,
        optimizeTarget!.skill.id,
        optimizedInstructions,
      ),
    onSuccess: async () => {
      message.success(t('mcp.aiSaved'));
      setOptimizeTarget(null);
      setOptimizedInstructions('');
      optimizeForm.resetFields();
      await queryClient.invalidateQueries({ queryKey: ['mcp-descriptor'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const saveMutation = useMutation({
    mutationFn: (values: Record<string, unknown>) => {
      const payload = editingSystemManaged
        ? { enabled: Boolean(values.enabled) }
        : buildMcpPayload(values, isAdmin);
      return editing ? api.updateMcp(editing.id, payload) : api.createMcp(payload);
    },
    onSuccess: () => {
      message.success(t('mcp.saved'));
      setModalOpen(false);
      setEditing(null);
      queryClient.invalidateQueries({ queryKey: ['mcps'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteMcp,
    onSuccess: () => {
      message.success(t('mcp.deleted'));
      queryClient.invalidateQueries({ queryKey: ['mcps'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const checkMutation = useMutation({
    mutationFn: api.checkMcp,
    onSuccess: (record) => {
      message.success(t('mcp.checkDone', { status: t(`status.${record.status}`) }));
      queryClient.invalidateQueries({ queryKey: ['mcps'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const columns = useMemo<ColumnsType<McpRecord>>(
    () => [
      {
        title: t('table.name'),
        dataIndex: 'display_name',
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text strong>{mcpDisplayName(record, t)}</Typography.Text>
            <Typography.Text type="secondary">{record.name}</Typography.Text>
          </Space>
        ),
      },
      {
        title: t('table.visibility'),
        dataIndex: 'visibility',
        width: 120,
        render: (value) => <VisibilityTag value={value} />,
      },
      {
        title: t('table.runtime'),
        dataIndex: ['runtime', 'kind'],
        width: 210,
        render: (value, record) => (
          <Space direction="vertical" size={0}>
            <RuntimeKindTag value={value} />
            <Typography.Text type="secondary" className="table-secondary-nowrap" ellipsis>
              {record.runtime.builtin_kind || record.runtime.server_name || record.runtime.url || record.runtime.command}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('table.owner'),
        dataIndex: 'owner_user_id',
        width: 180,
        render: (value) => <CompactId value={value} />,
      },
      {
        title: t('table.status'),
        dataIndex: 'enabled',
        width: 100,
        render: (enabled) => <EnabledTag enabled={enabled} />,
      },
      {
        title: t('table.updated'),
        dataIndex: 'updated_at',
        width: 170,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 450,
        render: (_, record) => (
          <Space wrap>
            <Button
              icon={<ReloadOutlined />}
              size="small"
              loading={checkMutation.isPending}
              onClick={() => checkMutation.mutate(record.id)}
            >
              {t('common.check')}
            </Button>
            <Button
              icon={<BookOutlined />}
              size="small"
              onClick={() => {
                setActiveProviderSkillId(null);
                setDescriptorModal({ record, view: 'skills' });
              }}
            >
              {t('mcp.providerSkills')}
            </Button>
            <Button
              icon={<ToolOutlined />}
              size="small"
              onClick={() => setDescriptorModal({ record, view: 'tools' })}
            >
              {t('mcp.toolCatalog')}
            </Button>
            <Button icon={<EditOutlined />} size="small" onClick={() => openEdit(record)}>
              {t(isSystemManagedMcp(record) ? 'common.configure' : 'common.edit')}
            </Button>
            {!isSystemManagedMcp(record) ? (
              <Popconfirm
                title={t('mcp.deleteConfirm')}
                onConfirm={() => deleteMutation.mutate(record.id)}
              >
                <Button danger icon={<DeleteOutlined />} size="small" />
              </Popconfirm>
            ) : null}
          </Space>
        ),
      },
    ],
    [checkMutation, deleteMutation, t],
  );

  function openCreate() {
    setEditing(null);
    form.resetFields();
    form.setFieldsValue({
      visibility: 'private',
      enabled: true,
      runtime_kind: isAdmin ? 'http' : 'local_connector_stdio',
      args_json: '[]',
      env_json: '{}',
      headers_json: '{}',
      local_connector_json: '',
    });
    setModalOpen(true);
  }

  function openEdit(record: McpRecord) {
    setEditing(record);
    form.resetFields();
    if (isSystemManagedMcp(record)) {
      form.setFieldsValue({ enabled: record.enabled });
      setModalOpen(true);
      return;
    }
    form.setFieldsValue({
      ...record,
      runtime_kind: record.runtime.kind,
      command: record.runtime.command,
      cwd: record.runtime.cwd,
      url: record.runtime.url,
      args_json: jsonText(record.runtime.args || []),
      env_json: jsonText(record.runtime.env || {}),
      headers_json: jsonText(record.runtime.headers || {}),
      local_connector_json: jsonText(record.runtime.local_connector),
    });
    setModalOpen(true);
  }

  function closeModal() {
    setModalOpen(false);
    setEditing(null);
    form.resetFields();
  }

  async function streamProviderSkillOptimization(values: {
    model_config_id: string;
    requirement: string;
  }) {
    if (!optimizeTarget) {
      return;
    }
    optimizeAbortRef.current?.abort();
    const controller = new AbortController();
    optimizeAbortRef.current = controller;
    setOptimizedInstructions('');
    setOptimizationThinking('');
    setOptimizeStreaming(true);
    let completed = false;
    try {
      await api.optimizeMcpProviderSkillStream(
        optimizeTarget.record.id,
        {
          model_config_id: values.model_config_id,
          skill_id: optimizeTarget.skill.id,
          requirement: values.requirement,
        },
        (event) => {
          if (event.type === 'chunk') {
            setOptimizedInstructions((current) => current + event.delta);
          } else if (event.type === 'thinking') {
            setOptimizationThinking((current) => `${current}${event.delta}`.slice(-3000));
          } else if (event.type === 'done') {
            completed = true;
            setOptimizedInstructions((current) =>
              event.optimized_instructions.length >= current.length
                ? event.optimized_instructions
                : current,
            );
          }
        },
        controller.signal,
      );
      if (!completed) {
        throw new Error('AI stream ended before the final result was received');
      }
      message.success(t('mcp.aiOptimized'));
    } catch (error) {
      if (!(error instanceof DOMException && error.name === 'AbortError')) {
        message.error(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (optimizeAbortRef.current === controller) {
        optimizeAbortRef.current = null;
      }
      setOptimizeStreaming(false);
    }
  }

  function closeDescriptorModal() {
    setDescriptorModal(null);
    setActiveProviderSkillId(null);
  }

  function startOptimizeForActiveSkill() {
    if (!descriptorModal || descriptorModal.view !== 'skills') {
      return;
    }
    const skills = descriptorQuery.data?.provider_skills || [];
    const skill = skills.find((item) => item.id === activeProviderSkillId) || skills[0];
    if (!skill) {
      return;
    }
    setOptimizeTarget({ record: descriptorModal.record, skill });
    setOptimizedInstructions('');
    setOptimizationThinking('');
    optimizeForm.resetFields();
  }

  function closeOptimizeModal() {
    optimizeAbortRef.current?.abort();
    optimizeAbortRef.current = null;
    setOptimizeTarget(null);
    setOptimizedInstructions('');
    setOptimizationThinking('');
    setOptimizeStreaming(false);
    optimizeForm.resetFields();
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('mcp.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('mcp.description')}</Typography.Text>
        </Space>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
          {t('mcp.add')}
        </Button>
      </div>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={mcpsQuery.data?.items || []}
        loading={mcpsQuery.isLoading}
        tableLayout="fixed"
        scroll={{ x: 1400 }}
        pagination={{ pageSize: 12 }}
      />
      <McpCatalogDialogs
        form={form}
        optimizeForm={optimizeForm}
        editing={editing}
        editingSystemManaged={editingSystemManaged}
        modalOpen={modalOpen}
        closeModal={closeModal}
        onSave={(values) => saveMutation.mutate(values)}
        savePending={saveMutation.isPending}
        isAdmin={isAdmin}
        runtimeKinds={runtimeKinds}
        runtimeKind={runtimeKind}
        descriptorModal={descriptorModal}
        descriptorData={descriptorQuery.data}
        descriptorLoading={descriptorQuery.isLoading}
        descriptorError={descriptorQuery.error}
        activeProviderSkillId={activeProviderSkillId}
        setActiveProviderSkillId={setActiveProviderSkillId}
        onCloseDescriptor={closeDescriptorModal}
        onStartOptimize={startOptimizeForActiveSkill}
        optimizeTarget={optimizeTarget}
        aiModels={aiModelsQuery.data}
        aiModelsLoading={aiModelsQuery.isLoading}
        optimizedInstructions={optimizedInstructions}
        setOptimizedInstructions={setOptimizedInstructions}
        optimizationThinking={optimizationThinking}
        optimizeStreaming={optimizeStreaming}
        onCloseOptimize={closeOptimizeModal}
        onStream={streamProviderSkillOptimization}
        onSaveOptimized={() => saveOptimizedSkillMutation.mutate()}
        saveOptimizedPending={saveOptimizedSkillMutation.isPending}
      />
    </div>
  );
}
