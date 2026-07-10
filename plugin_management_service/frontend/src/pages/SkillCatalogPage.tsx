// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { DeleteOutlined, EditOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Button,
  Form,
  Input,
  Modal,
  Popconfirm,
  Select,
  Space,
  Switch,
  Table,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { EnabledTag, VisibilityTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import { contentKindLabel } from '../i18n/labels';
import type { CurrentUser, SkillRecord } from '../types';
import { jsonText, optionalText, parseJsonObject } from './formUtils';

interface SkillCatalogPageProps {
  user: CurrentUser;
}

const contentKinds = [
  'inline_content',
  'cloud_package',
  'git_package',
  'local_connector_file',
  'local_connector_package',
];

export function SkillCatalogPage({ user }: SkillCatalogPageProps) {
  const { t } = useI18n();
  const [form] = Form.useForm();
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState<SkillRecord | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const isAdmin = user.role === 'super_admin';

  const skillsQuery = useQuery({
    queryKey: ['skills', isAdmin],
    queryFn: () => api.listSkills({ include_system: isAdmin, limit: 500 }),
  });

  const saveMutation = useMutation({
    mutationFn: (values: Record<string, unknown>) => {
      const payload = buildSkillPayload(values, isAdmin);
      return editing ? api.updateSkill(editing.id, payload) : api.createSkill(payload);
    },
    onSuccess: () => {
      message.success(t('skill.saved'));
      setModalOpen(false);
      setEditing(null);
      queryClient.invalidateQueries({ queryKey: ['skills'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteSkill,
    onSuccess: () => {
      message.success(t('skill.deleted'));
      queryClient.invalidateQueries({ queryKey: ['skills'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const checkMutation = useMutation({
    mutationFn: api.checkSkill,
    onSuccess: (record) =>
      message.success(t('skill.checkDone', { status: t(`status.${record.status}`) })),
    onError: (error) => message.error((error as Error).message),
  });

  const columns = useMemo<ColumnsType<SkillRecord>>(
    () => [
      {
        title: t('table.name'),
        dataIndex: 'display_name',
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text strong>{record.display_name}</Typography.Text>
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
        title: t('table.content'),
        dataIndex: ['content', 'kind'],
        width: 210,
        render: (value, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text>{contentKindLabel(value, t)}</Typography.Text>
            <Typography.Text type="secondary">
              {record.content.source_path || record.content.repository || record.content.package_id}
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
        width: 230,
        render: (_, record) => (
          <Space>
            <Button
              icon={<ReloadOutlined />}
              size="small"
              onClick={() => checkMutation.mutate(record.id)}
            >
              {t('common.check')}
            </Button>
            <Button icon={<EditOutlined />} size="small" onClick={() => openEdit(record)}>
              {t('common.edit')}
            </Button>
            <Popconfirm
              title={t('skill.deleteConfirm')}
              onConfirm={() => deleteMutation.mutate(record.id)}
            >
              <Button danger icon={<DeleteOutlined />} size="small" />
            </Popconfirm>
          </Space>
        ),
      },
    ],
    [checkMutation, deleteMutation, t],
  );

  function openCreate() {
    setEditing(null);
    form.setFieldsValue({
      visibility: 'private',
      enabled: true,
      content_kind: 'inline_content',
      local_connector_json: '',
      metadata_json: '{}',
    });
    setModalOpen(true);
  }

  function openEdit(record: SkillRecord) {
    setEditing(record);
    form.setFieldsValue({
      ...record,
      content_kind: record.content.kind,
      inline: record.content.inline,
      package_id: record.content.package_id,
      source_path: record.content.source_path,
      repository: record.content.repository,
      branch: record.content.branch,
      local_connector_json: jsonText(record.content.local_connector),
      metadata_json: jsonText(record.metadata || {}),
    });
    setModalOpen(true);
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('skill.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('skill.description')}</Typography.Text>
        </Space>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
          {t('skill.add')}
        </Button>
      </div>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={skillsQuery.data?.items || []}
        loading={skillsQuery.isLoading}
        tableLayout="fixed"
        scroll={{ x: 1120 }}
        pagination={{ pageSize: 12 }}
      />
      <Modal
        title={t(editing ? 'skill.editTitle' : 'skill.addTitle')}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        onOk={() => form.submit()}
        confirmLoading={saveMutation.isPending}
        width={820}
        destroyOnClose
      >
        <Form form={form} layout="vertical" onFinish={(values) => saveMutation.mutate(values)}>
          <div className="form-grid">
            <Form.Item name="name" label={t('field.internalName')} rules={[{ required: true }]}>
              <Input />
            </Form.Item>
            <Form.Item name="display_name" label={t('field.displayName')}>
              <Input />
            </Form.Item>
            <Form.Item name="visibility" label={t('field.visibility')}>
              <Select
                options={[
                  { value: 'private', label: t('visibility.private') },
                  ...(isAdmin
                    ? [
                        { value: 'public', label: t('visibility.public') },
                        { value: 'system_private', label: t('visibility.system_private') },
                      ]
                    : []),
                ]}
              />
            </Form.Item>
            <Form.Item name="enabled" label={t('field.enabled')} valuePropName="checked">
              <Switch />
            </Form.Item>
          </div>
          <Form.Item name="description" label={t('field.description')}>
            <Input.TextArea rows={2} />
          </Form.Item>
          <div className="form-grid">
            <Form.Item name="content_kind" label={t('field.contentKind')} rules={[{ required: true }]}>
              <Select options={contentKinds.map((value) => ({ value, label: contentKindLabel(value, t) }))} />
            </Form.Item>
            <Form.Item name="package_id" label={t('field.packageId')}>
              <Input />
            </Form.Item>
            <Form.Item name="source_path" label={t('field.sourcePath')}>
              <Input />
            </Form.Item>
            <Form.Item name="repository" label={t('field.repository')}>
              <Input />
            </Form.Item>
            <Form.Item name="branch" label={t('field.branch')}>
              <Input />
            </Form.Item>
          </div>
          <Form.Item name="inline" label={t('field.inlineContent')}>
            <Input.TextArea rows={6} />
          </Form.Item>
          <div className="form-grid two">
            <Form.Item name="local_connector_json" label={t('field.localConnectorJson')}>
              <Input.TextArea rows={4} />
            </Form.Item>
            <Form.Item name="metadata_json" label={t('field.metadataJson')}>
              <Input.TextArea rows={4} />
            </Form.Item>
          </div>
        </Form>
      </Modal>
    </div>
  );
}

function buildSkillPayload(values: Record<string, unknown>, isAdmin: boolean) {
  const payload: Record<string, unknown> = {
    name: optionalText(values.name),
    display_name: optionalText(values.display_name),
    description: optionalText(values.description),
    visibility: values.visibility || 'private',
    enabled: Boolean(values.enabled),
    content: {
      kind: values.content_kind,
      inline: optionalText(values.inline),
      package_id: optionalText(values.package_id),
      source_path: optionalText(values.source_path),
      repository: optionalText(values.repository),
      branch: optionalText(values.branch),
      local_connector: optionalText(values.local_connector_json)
        ? parseJsonObject(values.local_connector_json, {})
        : null,
    },
    metadata: parseJsonObject(values.metadata_json, {}),
  };
  if (!isAdmin && payload.visibility !== 'private') {
    payload.visibility = 'private';
  }
  return payload;
}
