// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { DeleteOutlined, EditOutlined, PlusOutlined } from '@ant-design/icons';
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
import { DateTimeCell } from '../components/DisplayCells';
import { VisibilityTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import { sourceKindLabel } from '../i18n/labels';
import type { CurrentUser, SkillPackageRecord } from '../types';
import { jsonText, optionalText, parseJsonArray, parseJsonObject } from './formUtils';

interface SkillPackagesPageProps {
  user: CurrentUser;
}

export function SkillPackagesPage({ user }: SkillPackagesPageProps) {
  const { t } = useI18n();
  const [form] = Form.useForm();
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState<SkillPackageRecord | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const isAdmin = user.role === 'super_admin';

  const packagesQuery = useQuery({
    queryKey: ['skill-packages', isAdmin],
    queryFn: () => api.listSkillPackages({ include_system: isAdmin, limit: 500 }),
  });

  const saveMutation = useMutation({
    mutationFn: (values: Record<string, unknown>) => {
      const payload = buildPackagePayload(values, isAdmin);
      return editing
        ? api.updateSkillPackage(editing.id, payload)
        : api.createSkillPackage(payload);
    },
    onSuccess: () => {
      message.success(t('package.saved'));
      setModalOpen(false);
      setEditing(null);
      queryClient.invalidateQueries({ queryKey: ['skill-packages'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteSkillPackage,
    onSuccess: () => {
      message.success(t('package.deleted'));
      queryClient.invalidateQueries({ queryKey: ['skill-packages'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const columns = useMemo<ColumnsType<SkillPackageRecord>>(
    () => [
      {
        title: t('table.name'),
        dataIndex: 'name',
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text strong>{record.name}</Typography.Text>
            <Typography.Text type="secondary">{record.description}</Typography.Text>
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
        title: t('table.source'),
        dataIndex: 'source_kind',
        width: 180,
        render: (value) => sourceKindLabel(value, t),
      },
      {
        title: t('table.skillCount'),
        dataIndex: 'skill_ids',
        width: 100,
        render: (ids: string[]) => ids?.length || 0,
      },
      {
        title: t('table.installation'),
        dataIndex: 'installed',
        width: 100,
        render: (installed) => t(installed ? 'common.installed' : 'common.notInstalled'),
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
        width: 150,
        render: (_, record) => (
          <Space>
            <Button icon={<EditOutlined />} size="small" onClick={() => openEdit(record)}>
              {t('common.edit')}
            </Button>
            <Popconfirm
              title={t('package.deleteConfirm')}
              onConfirm={() => deleteMutation.mutate(record.id)}
            >
              <Button danger icon={<DeleteOutlined />} size="small" />
            </Popconfirm>
          </Space>
        ),
      },
    ],
    [deleteMutation, t],
  );

  function openCreate() {
    setEditing(null);
    form.setFieldsValue({
      visibility: 'private',
      source_kind: 'git',
      installed: true,
      local_connector_json: '',
      skill_ids_json: '[]',
    });
    setModalOpen(true);
  }

  function openEdit(record: SkillPackageRecord) {
    setEditing(record);
    form.setFieldsValue({
      ...record,
      local_connector_json: jsonText(record.local_connector),
      skill_ids_json: jsonText(record.skill_ids || []),
    });
    setModalOpen(true);
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('package.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('package.description')}</Typography.Text>
        </Space>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
          {t('package.add')}
        </Button>
      </div>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={packagesQuery.data?.items || []}
        loading={packagesQuery.isLoading}
        tableLayout="fixed"
        scroll={{ x: 980 }}
        pagination={{ pageSize: 12 }}
      />
      <Modal
        title={t(editing ? 'package.editTitle' : 'package.addTitle')}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        onOk={() => form.submit()}
        confirmLoading={saveMutation.isPending}
        width={760}
        destroyOnClose
      >
        <Form form={form} layout="vertical" onFinish={(values) => saveMutation.mutate(values)}>
          <div className="form-grid">
            <Form.Item name="name" label={t('table.name')} rules={[{ required: true }]}>
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
            <Form.Item name="source_kind" label={t('field.sourceKind')}>
              <Select
                options={['git', 'local_connector', 'inline_bundle', 'system_seed'].map((value) => ({
                  value,
                  label: sourceKindLabel(value, t),
                }))}
              />
            </Form.Item>
            <Form.Item name="installed" label={t('field.installed')} valuePropName="checked">
              <Switch />
            </Form.Item>
          </div>
          <Form.Item name="description" label={t('field.description')}>
            <Input.TextArea rows={2} />
          </Form.Item>
          <div className="form-grid">
            <Form.Item name="repository" label={t('field.repository')}>
              <Input />
            </Form.Item>
            <Form.Item name="branch" label={t('field.branch')}>
              <Input />
            </Form.Item>
            <Form.Item name="cache_ref" label={t('field.cacheRef')}>
              <Input />
            </Form.Item>
          </div>
          <div className="form-grid two">
            <Form.Item name="skill_ids_json" label={t('field.skillIdsJson')}>
              <Input.TextArea rows={4} />
            </Form.Item>
            <Form.Item name="local_connector_json" label={t('field.localConnectorJson')}>
              <Input.TextArea rows={4} />
            </Form.Item>
          </div>
        </Form>
      </Modal>
    </div>
  );
}

function buildPackagePayload(values: Record<string, unknown>, isAdmin: boolean) {
  const payload: Record<string, unknown> = {
    name: optionalText(values.name),
    description: optionalText(values.description),
    visibility: values.visibility || 'private',
    source_kind: values.source_kind || 'git',
    repository: optionalText(values.repository),
    branch: optionalText(values.branch),
    cache_ref: optionalText(values.cache_ref),
    installed: Boolean(values.installed),
    skill_ids: parseJsonArray(values.skill_ids_json, []),
    local_connector: optionalText(values.local_connector_json)
      ? parseJsonObject(values.local_connector_json, {})
      : null,
  };
  if (!isAdmin && payload.visibility !== 'private') {
    payload.visibility = 'private';
  }
  return payload;
}
