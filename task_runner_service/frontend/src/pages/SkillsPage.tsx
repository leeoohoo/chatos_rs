// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  App as AntdApp,
  Button,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  Modal,
  Select,
  Space,
  Switch,
  Table,
  Tabs,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import {
  CloudDownloadOutlined,
  DeleteOutlined,
  EditOutlined,
  EyeOutlined,
  PlusOutlined,
  ReloadOutlined,
  SearchOutlined,
} from '@ant-design/icons';
import dayjs from 'dayjs';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import type {
  CreateSkillPayload,
  SkillMarketplaceEntry,
  SkillPackageFile,
  SkillRecord,
  UpdateSkillPayload,
} from '../types';

const { TextArea } = Input;

const SECTION_STYLE = {
  width: '100%',
  padding: 16,
  borderRadius: 6,
  background: '#fff',
  border: '1px solid #f0f0f0',
};

const MARKETPLACE_DEFAULT_PAGE_SIZE = 10;

function marketplaceEntryKey(registry?: string | null, packageId?: string | null) {
  return `${registry || 'default'}:${packageId || ''}`;
}

type SkillFormValues = {
  name?: string;
  display_name: string;
  description?: string;
  content?: string;
  locale?: string;
  tagsText?: string;
  source_url?: string;
  enabled?: boolean;
  auto_inject?: boolean;
};

export function SkillsPage() {
  const { t } = useI18n();
  const { message } = AntdApp.useApp();
  const queryClient = useQueryClient();
  const [form] = Form.useForm<SkillFormValues>();
  const [createForm] = Form.useForm<SkillFormValues>();
  const [keyword, setKeyword] = useState('');
  const [marketplaceKeyword, setMarketplaceKeyword] = useState('');
  const [marketplacePage, setMarketplacePage] = useState(1);
  const [marketplacePageSize, setMarketplacePageSize] = useState(MARKETPLACE_DEFAULT_PAGE_SIZE);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingSkill, setEditingSkill] = useState<SkillRecord | null>(null);
  const [previewSkill, setPreviewSkill] = useState<SkillRecord | null>(null);
  const [previewMarketplaceEntry, setPreviewMarketplaceEntry] =
    useState<SkillMarketplaceEntry | null>(null);
  const [installingMarketplaceKey, setInstallingMarketplaceKey] = useState<string | null>(null);

  const skillsQuery = useQuery({
    queryKey: ['skills', keyword],
    queryFn: () => api.listSkills({ keyword }),
  });
  const bundledSkillsQuery = useQuery({
    queryKey: ['skills', 'bundled'],
    queryFn: api.listBundledSkills,
  });
  const marketplaceQuery = useQuery({
    queryKey: ['skill-marketplace', marketplaceKeyword, marketplacePage, marketplacePageSize],
    queryFn: () =>
      api.searchSkillMarketplace({
        keyword: marketplaceKeyword,
        limit: marketplacePageSize,
        offset: (marketplacePage - 1) * marketplacePageSize,
      }),
  });

  const invalidateSkills = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['skills'] }),
      queryClient.invalidateQueries({ queryKey: ['skill-marketplace'] }),
    ]);
  };

  const createMutation = useMutation({
    mutationFn: api.createSkill,
    onSuccess: async () => {
      await invalidateSkills();
      createForm.resetFields();
      message.success(t('skills.created'));
    },
    onError: (error: Error) => message.error(error.message),
  });
  const updateMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: UpdateSkillPayload }) =>
      api.updateSkill(id, payload),
    onSuccess: async () => {
      await invalidateSkills();
      closeDrawer();
      message.success(t('skills.updated'));
    },
    onError: (error: Error) => message.error(error.message),
  });
  const quickUpdateMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: UpdateSkillPayload }) =>
      api.updateSkill(id, payload),
    onSuccess: invalidateSkills,
    onError: (error: Error) => message.error(error.message),
  });
  const deleteMutation = useMutation({
    mutationFn: api.deleteSkill,
    onSuccess: async () => {
      await invalidateSkills();
      message.success(t('skills.deleted'));
    },
    onError: (error: Error) => message.error(error.message),
  });
  const installMutation = useMutation({
    mutationFn: api.installSkillFromMarketplace,
    onMutate: ({ registry, package_id }) => {
      setInstallingMarketplaceKey(marketplaceEntryKey(registry, package_id));
    },
    onSuccess: async () => {
      await invalidateSkills();
      message.success(t('skills.installed'));
    },
    onError: (error: Error) => message.error(error.message),
    onSettled: () => setInstallingMarketplaceKey(null),
  });

  const skills = skillsQuery.data || [];
  const bundledSkills = bundledSkillsQuery.data || [];
  const marketplaceData = marketplaceQuery.data;
  const autoInjectCount = useMemo(
    () => skills.filter((skill) => skill.enabled && skill.auto_inject).length,
    [skills],
  );

  function openCreateDrawer() {
    setEditingSkill(null);
    form.setFieldsValue(defaultSkillFormValues());
    setDrawerOpen(true);
  }

  function openEditDrawer(skill: SkillRecord) {
    setEditingSkill(skill);
    form.setFieldsValue(skillToFormValues(skill));
    setDrawerOpen(true);
  }

  function closeDrawer() {
    setDrawerOpen(false);
    setEditingSkill(null);
    form.resetFields();
  }

  function handleDrawerSubmit(values: SkillFormValues) {
    const payload = buildSkillPayload(values);
    if (editingSkill) {
      updateMutation.mutate({ id: editingSkill.id, payload });
    } else {
      createMutation.mutate(payload as CreateSkillPayload);
      closeDrawer();
    }
  }

  function handleCreateSubmit(values: SkillFormValues) {
    createMutation.mutate(buildSkillPayload(values) as CreateSkillPayload);
  }

  function confirmDelete(skill: SkillRecord) {
    Modal.confirm({
      title: t('skills.deleteTitle', { name: skill.display_name }),
      content: t('skills.deleteContent'),
      okButtonProps: { danger: true },
      onOk: () => deleteMutation.mutate(skill.id),
    });
  }

  function handleMarketplaceSearch(value: string) {
    setMarketplaceKeyword(value);
    setMarketplacePage(1);
  }

  const skillColumns: ColumnsType<SkillRecord> = [
    {
      title: t('skills.column.skill'),
      dataIndex: 'display_name',
      width: 300,
      render: (_value, record) => (
        <Space direction="vertical" size={2}>
          <Typography.Text strong>{record.display_name}</Typography.Text>
          <Typography.Text type="secondary">{record.name}</Typography.Text>
          {record.description ? (
            <Typography.Text type="secondary">{record.description}</Typography.Text>
          ) : null}
        </Space>
      ),
    },
    {
      title: t('skills.column.source'),
      dataIndex: 'source',
      width: 170,
      render: (_value, record) => (
        <Space direction="vertical" size={4}>
          <Tag color={sourceColor(record.source)}>{record.source}</Tag>
          <Typography.Text type="secondary">{record.locale}</Typography.Text>
          {record.package_file_count ? (
            <Tag>
              {t('skills.packageSummary', {
                files: record.package_file_count,
                size: formatBytes(record.package_total_bytes || 0),
              })}
            </Tag>
          ) : null}
        </Space>
      ),
    },
    {
      title: t('skills.column.owner'),
      key: 'owner',
      width: 210,
      render: (_, record) => <SkillOwnerCell skill={record} t={t} />,
    },
    {
      title: t('skills.column.tags'),
      dataIndex: 'tags',
      render: (tags: string[]) => (
        <Space size={[4, 4]} wrap>
          {(tags || []).map((tag) => (
            <Tag key={tag}>{tag}</Tag>
          ))}
        </Space>
      ),
    },
    {
      title: t('skills.column.runtime'),
      key: 'runtime',
      width: 190,
      render: (_, record) => (
        <Space direction="vertical" size={6}>
          <Switch
            size="small"
            checked={record.enabled}
            checkedChildren={t('common.enabled')}
            unCheckedChildren={t('common.disabled')}
            loading={quickUpdateMutation.isPending}
            onChange={(enabled) =>
              quickUpdateMutation.mutate({ id: record.id, payload: { enabled } })
            }
          />
          <Switch
            size="small"
            checked={record.auto_inject}
            checkedChildren={t('skills.autoInjectShort')}
            unCheckedChildren={t('skills.manualShort')}
            loading={quickUpdateMutation.isPending}
            onChange={(auto_inject) =>
              quickUpdateMutation.mutate({ id: record.id, payload: { auto_inject } })
            }
          />
        </Space>
      ),
    },
    {
      title: t('common.updatedAt'),
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => formatDate(value),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 180,
      render: (_, record) => (
        <Space>
          <Tooltip title={t('common.view')}>
            <Button size="small" icon={<EyeOutlined />} onClick={() => setPreviewSkill(record)} />
          </Tooltip>
          <Tooltip title={t('common.edit')}>
            <Button size="small" icon={<EditOutlined />} onClick={() => openEditDrawer(record)} />
          </Tooltip>
          <Tooltip title={t('common.delete')}>
            <Button
              size="small"
              danger
              icon={<DeleteOutlined />}
              onClick={() => confirmDelete(record)}
            />
          </Tooltip>
        </Space>
      ),
    },
  ];

  const bundledSkillColumns: ColumnsType<SkillRecord> = [
    ...skillColumns.filter((column) => column.key !== 'runtime' && column.key !== 'actions'),
    {
      title: t('common.actions'),
      key: 'actions',
      width: 100,
      render: (_, record) => (
        <Tooltip title={t('common.view')}>
          <Button size="small" icon={<EyeOutlined />} onClick={() => setPreviewSkill(record)} />
        </Tooltip>
      ),
    },
  ];

  const marketplaceColumns: ColumnsType<SkillMarketplaceEntry> = [
    {
      title: t('skills.column.skill'),
      dataIndex: 'display_name',
      render: (_value, record) => (
        <Space direction="vertical" size={2}>
          <Space size={8} wrap>
            <Typography.Text strong>{record.display_name}</Typography.Text>
            <Tag color="blue">{record.locale}</Tag>
            {record.installed ? <Tag color="success">{t('skills.installedStatus')}</Tag> : null}
          </Space>
          <Typography.Text type="secondary">{record.description}</Typography.Text>
          <Typography.Text
            type="secondary"
            style={{ maxWidth: 720 }}
            ellipsis={{ tooltip: record.source_url || record.package_id }}
          >
            {record.registry}: {record.source_url || record.package_id}
          </Typography.Text>
          {record.installed && record.package_file_count ? (
            <Tag>
              {t('skills.packageSummary', {
                files: record.package_file_count,
                size: formatBytes(record.package_total_bytes || 0),
              })}
            </Tag>
          ) : null}
        </Space>
      ),
    },
    {
      title: t('skills.column.tags'),
      dataIndex: 'tags',
      width: 260,
      render: (tags: string[]) => (
        <Space size={[4, 4]} wrap>
          {(tags || []).map((tag) => (
            <Tag key={tag}>{tag}</Tag>
          ))}
        </Space>
      ),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 190,
      render: (_, record) => {
        const rowInstallKey = marketplaceEntryKey(record.registry, record.package_id);
        const isInstallingThis =
          installMutation.isPending && installingMarketplaceKey === rowInstallKey;

        return (
          <Space>
            <Button
              size="small"
              icon={<EyeOutlined />}
              onClick={() => setPreviewMarketplaceEntry(record)}
            >
              {t('common.view')}
            </Button>
            <Button
              size="small"
              type="primary"
              icon={<CloudDownloadOutlined />}
              loading={isInstallingThis}
              disabled={installMutation.isPending && !isInstallingThis}
              onClick={() =>
                installMutation.mutate({
                  registry: record.registry,
                  package_id: record.package_id,
                  enabled: true,
                  auto_inject: false,
                })
              }
            >
              {record.installed ? t('skills.reinstall') : t('skills.install')}
            </Button>
          </Space>
        );
      },
    },
  ];

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          {t('skills.title')}
        </Typography.Title>
        <Typography.Text type="secondary">{t('skills.subtitle')}</Typography.Text>
      </Space>

      <Tabs
        items={[
          {
            key: 'mine',
            label: t('skills.tab.mine'),
            children: (
              <Space direction="vertical" size="middle" style={SECTION_STYLE}>
                <Space style={{ width: '100%', justifyContent: 'space-between' }} align="start">
                  <Space direction="vertical" size={2}>
                    <Typography.Title level={5} style={{ margin: 0 }}>
                      {t('skills.mineTitle')}
                    </Typography.Title>
                    <Typography.Text type="secondary">
                      {t('skills.mineSummary', {
                        total: skills.length,
                        auto: autoInjectCount,
                      })}
                    </Typography.Text>
                  </Space>
                  <Space>
                    <Input.Search
                      allowClear
                      style={{ width: 280 }}
                      placeholder={t('skills.searchPlaceholder')}
                      onSearch={setKeyword}
                      prefix={<SearchOutlined />}
                    />
                    <Button icon={<ReloadOutlined />} onClick={() => skillsQuery.refetch()}>
                      {t('common.refresh')}
                    </Button>
                    <Button type="primary" icon={<PlusOutlined />} onClick={openCreateDrawer}>
                      {t('skills.new')}
                    </Button>
                  </Space>
                </Space>
                <Table<SkillRecord>
                  rowKey="id"
                  columns={skillColumns}
                  dataSource={skills}
                  loading={skillsQuery.isLoading}
                  scroll={{ x: 1220 }}
                  pagination={{ pageSize: 8, showSizeChanger: false }}
                  locale={{
                    emptyText: (
                      <Empty
                        image={Empty.PRESENTED_IMAGE_SIMPLE}
                        description={t('skills.empty')}
                      />
                    ),
                  }}
                />
              </Space>
            ),
          },
          {
            key: 'add',
            label: t('skills.tab.add'),
            children: (
              <Space direction="vertical" size="middle" style={SECTION_STYLE}>
                <Space direction="vertical" size={2}>
                  <Typography.Title level={5} style={{ margin: 0 }}>
                    {t('skills.addTitle')}
                  </Typography.Title>
                  <Typography.Text type="secondary">{t('skills.addSubtitle')}</Typography.Text>
                </Space>
                <SkillForm
                  form={createForm}
                  t={t}
                  saving={createMutation.isPending}
                  onSubmit={handleCreateSubmit}
                  submitLabel={t('skills.create')}
                />
              </Space>
            ),
          },
          {
            key: 'bundled',
            label: t('skills.tab.bundled'),
            children: (
              <Space direction="vertical" size="middle" style={SECTION_STYLE}>
                <Space style={{ width: '100%', justifyContent: 'space-between' }} align="start">
                  <Space direction="vertical" size={2}>
                    <Typography.Title level={5} style={{ margin: 0 }}>
                      {t('skills.bundledTitle')}
                    </Typography.Title>
                    <Typography.Text type="secondary">
                      {t('skills.bundledSummary', { total: bundledSkills.length })}
                    </Typography.Text>
                  </Space>
                  <Button icon={<ReloadOutlined />} onClick={() => bundledSkillsQuery.refetch()}>
                    {t('common.refresh')}
                  </Button>
                </Space>
                <Table<SkillRecord>
                  rowKey="id"
                  columns={bundledSkillColumns}
                  dataSource={bundledSkills}
                  loading={bundledSkillsQuery.isLoading}
                  scroll={{ x: 1120 }}
                  pagination={{ pageSize: 8, showSizeChanger: false }}
                  locale={{
                    emptyText: (
                      <Empty
                        image={Empty.PRESENTED_IMAGE_SIMPLE}
                        description={t('skills.bundledEmpty')}
                      />
                    ),
                  }}
                />
              </Space>
            ),
          },
          {
            key: 'marketplace',
            label: t('skills.tab.marketplace'),
            children: (
              <Space direction="vertical" size="middle" style={SECTION_STYLE}>
                <Space style={{ width: '100%', justifyContent: 'space-between' }} align="start">
                  <Space direction="vertical" size={2}>
                    <Typography.Title level={5} style={{ margin: 0 }}>
                      {t('skills.marketplaceTitle')}
                    </Typography.Title>
                    <Typography.Text type="secondary">
                      {t('skills.marketplaceSubtitle')}
                    </Typography.Text>
                  </Space>
                  <Input.Search
                    allowClear
                    style={{ width: 300 }}
                    placeholder={t('skills.marketplaceSearchPlaceholder')}
                    onSearch={handleMarketplaceSearch}
                    prefix={<SearchOutlined />}
                  />
                </Space>
                <Table<SkillMarketplaceEntry>
                  rowKey={(record) => `${record.registry}:${record.package_id}`}
                  columns={marketplaceColumns}
                  dataSource={marketplaceData?.items || []}
                  loading={marketplaceQuery.isLoading}
                  pagination={{
                    current: marketplacePage,
                    pageSize: marketplacePageSize,
                    total: marketplaceData?.total || 0,
                    showSizeChanger: true,
                    pageSizeOptions: [10, 20, 50],
                    showTotal: (total) => t('skills.marketplaceTotal', { total }),
                    onChange: (nextPage, nextPageSize) => {
                      setMarketplacePage(nextPage);
                      if (nextPageSize !== marketplacePageSize) {
                        setMarketplacePage(1);
                        setMarketplacePageSize(nextPageSize);
                      }
                    },
                  }}
                  locale={{
                    emptyText: (
                      <Empty
                        image={Empty.PRESENTED_IMAGE_SIMPLE}
                        description={t('skills.marketplaceEmpty')}
                      />
                    ),
                  }}
                />
              </Space>
            ),
          },
        ]}
      />

      <Drawer
        title={editingSkill ? t('skills.editTitle') : t('skills.createTitle')}
        open={drawerOpen}
        onClose={closeDrawer}
        width={720}
        destroyOnClose
        extra={
          <Space>
            <Button onClick={closeDrawer}>{t('common.cancel')}</Button>
            <Button
              type="primary"
              loading={updateMutation.isPending || createMutation.isPending}
              onClick={() => form.submit()}
            >
              {t('common.save')}
            </Button>
          </Space>
        }
      >
        <SkillForm
          form={form}
          t={t}
          saving={updateMutation.isPending || createMutation.isPending}
          onSubmit={handleDrawerSubmit}
          submitLabel={t('common.save')}
          hideSubmit
        />
      </Drawer>

      <Modal
        title={previewSkill?.display_name || t('skills.previewTitle')}
        open={Boolean(previewSkill)}
        onCancel={() => setPreviewSkill(null)}
        footer={null}
        width={960}
      >
        {previewSkill ? <SkillDetail skill={previewSkill} t={t} /> : null}
      </Modal>

      <Modal
        title={previewMarketplaceEntry?.display_name || t('skills.previewTitle')}
        open={Boolean(previewMarketplaceEntry)}
        onCancel={() => setPreviewMarketplaceEntry(null)}
        footer={null}
        width={840}
      >
        <SkillPreview content={previewMarketplaceEntry?.preview_content || ''} />
      </Modal>
    </Space>
  );
}

function SkillForm({
  form,
  t,
  saving,
  submitLabel,
  hideSubmit,
  onSubmit,
}: {
  form: ReturnType<typeof Form.useForm<SkillFormValues>>[0];
  t: (key: string, params?: Record<string, string | number>) => string;
  saving: boolean;
  submitLabel: string;
  hideSubmit?: boolean;
  onSubmit: (values: SkillFormValues) => void;
}) {
  return (
    <Form<SkillFormValues>
      form={form}
      layout="vertical"
      initialValues={defaultSkillFormValues()}
      onFinish={onSubmit}
    >
      <Space align="start" style={{ width: '100%' }}>
        <Form.Item
          name="display_name"
          label={t('skills.form.displayName')}
          rules={[{ required: true, message: t('skills.form.displayNameRequired') }]}
          style={{ flex: 1, minWidth: 260 }}
        >
          <Input placeholder="Frontend Reviewer" />
        </Form.Item>
        <Form.Item name="name" label={t('skills.form.name')} style={{ width: 240 }}>
          <Input placeholder="frontend-reviewer" />
        </Form.Item>
        <Form.Item name="locale" label={t('skills.form.locale')} style={{ width: 140 }}>
          <Select
            options={[
              { label: 'zh-CN', value: 'zh-CN' },
              { label: 'en-US', value: 'en-US' },
            ]}
          />
        </Form.Item>
      </Space>

      <Form.Item name="description" label={t('common.description')}>
        <Input placeholder={t('skills.form.descriptionPlaceholder')} />
      </Form.Item>

      <Form.Item name="tagsText" label={t('skills.form.tags')}>
        <Input placeholder="review, frontend, react" />
      </Form.Item>

      <Space align="start" style={{ width: '100%' }}>
        <Form.Item name="enabled" label={t('common.status')} valuePropName="checked">
          <Switch checkedChildren={t('common.enabled')} unCheckedChildren={t('common.disabled')} />
        </Form.Item>
        <Form.Item
          name="auto_inject"
          label={t('skills.form.autoInject')}
          valuePropName="checked"
        >
          <Switch
            checkedChildren={t('skills.autoInjectShort')}
            unCheckedChildren={t('skills.manualShort')}
          />
        </Form.Item>
      </Space>

      <Form.Item name="source_url" label={t('skills.form.sourceUrl')}>
        <Input placeholder="https://raw.githubusercontent.com/.../SKILL.md" />
      </Form.Item>

      <Form.Item
        name="content"
        label={t('skills.form.content')}
        rules={[
          ({ getFieldValue }) => ({
            validator: async (_, value) => {
              const sourceUrl = String(getFieldValue('source_url') || '').trim();
              if (String(value || '').trim() || sourceUrl) {
                return;
              }
              throw new Error(t('skills.form.contentRequired'));
            },
          }),
        ]}
      >
        <TextArea rows={14} placeholder="# Skill\n\n..." />
      </Form.Item>

      {hideSubmit ? null : (
        <Form.Item>
          <Button type="primary" htmlType="submit" loading={saving}>
            {submitLabel}
          </Button>
        </Form.Item>
      )}
    </Form>
  );
}

function SkillPreview({ content }: { content: string }) {
  return (
    <pre
      style={{
        maxHeight: 520,
        overflow: 'auto',
        padding: 12,
        margin: 0,
        borderRadius: 6,
        background: '#0f172a',
        color: '#e2e8f0',
        whiteSpace: 'pre-wrap',
        wordBreak: 'break-word',
      }}
    >
      {content}
    </pre>
  );
}

function SkillOwnerCell({
  skill,
  t,
}: {
  skill: SkillRecord;
  t: (key: string, params?: Record<string, string | number>) => string;
}) {
  return (
    <Space direction="vertical" size={2}>
      <Typography.Text>{skillOwnerLabel(skill, t)}</Typography.Text>
      <Typography.Text type="secondary">{skillOwnerSecondary(skill, t)}</Typography.Text>
    </Space>
  );
}

function SkillDetail({
  skill,
  t,
}: {
  skill: SkillRecord;
  t: (key: string, params?: Record<string, string | number>) => string;
}) {
  return (
    <Space direction="vertical" size="middle" style={{ width: '100%' }}>
      <Descriptions size="small" column={2} bordered>
        <Descriptions.Item label={t('skills.owner')}>
          {skillOwnerLabel(skill, t)}
        </Descriptions.Item>
        <Descriptions.Item label={t('skills.creator')}>
          {skillCreatorLabel(skill, t)}
        </Descriptions.Item>
        <Descriptions.Item label={t('skills.scope')}>
          <Tag>{skill.scope}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label={t('skills.installStatus')}>
          <Tag color={skill.install_status === 'installed' ? 'success' : 'default'}>
            {skill.install_status}
          </Tag>
        </Descriptions.Item>
      </Descriptions>
      <Tabs
        items={[
          {
            key: 'content',
            label: t('skills.detail.content'),
            children: <SkillPreview content={skill.content || ''} />,
          },
          {
            key: 'package',
            label: t('skills.detail.package'),
            children: <SkillPackageDetails skill={skill} t={t} />,
          },
        ]}
      />
    </Space>
  );
}

function SkillPackageDetails({
  skill,
  t,
}: {
  skill: SkillRecord;
  t: (key: string, params?: Record<string, string | number>) => string;
}) {
  const files = skill.package_manifest || [];
  const hasPackage =
    Boolean(skill.package_root) ||
    files.length > 0 ||
    Boolean(skill.package_file_count) ||
    Boolean(skill.source_repo);
  const fileColumns: ColumnsType<SkillPackageFile> = [
    {
      title: t('skills.package.filePath'),
      dataIndex: 'path',
      render: (value: string) => (
        <Typography.Text style={{ maxWidth: 420 }} ellipsis={{ tooltip: value }}>
          {value}
        </Typography.Text>
      ),
    },
    {
      title: t('skills.package.fileSize'),
      dataIndex: 'size_bytes',
      width: 120,
      render: (value: number) => formatBytes(value),
    },
    {
      title: t('skills.package.sourceUrl'),
      dataIndex: 'source_url',
      width: 320,
      render: (value?: string | null) =>
        value ? (
          <Typography.Text style={{ maxWidth: 300 }} ellipsis={{ tooltip: value }}>
            {value}
          </Typography.Text>
        ) : (
          '-'
        ),
    },
  ];

  if (!hasPackage) {
    return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('skills.packageEmpty')} />;
  }

  return (
    <Space direction="vertical" size="middle" style={{ width: '100%' }}>
      <Descriptions size="small" column={1} bordered>
        <Descriptions.Item label={t('skills.package.sourceRepo')}>
          {skill.source_repo || '-'}
        </Descriptions.Item>
        <Descriptions.Item label={t('skills.package.sourceRef')}>
          {skill.source_ref || '-'}
        </Descriptions.Item>
        <Descriptions.Item label={t('skills.package.sourcePath')}>
          {skill.source_path || '-'}
        </Descriptions.Item>
        <Descriptions.Item label={t('skills.package.localRoot')}>
          <Typography.Text ellipsis={{ tooltip: skill.package_root || '-' }}>
            {skill.package_root || '-'}
          </Typography.Text>
        </Descriptions.Item>
        <Descriptions.Item label={t('skills.package.files')}>
          {t('skills.packageSummary', {
            files: skill.package_file_count || files.length,
            size: formatBytes(skill.package_total_bytes || 0),
          })}
        </Descriptions.Item>
      </Descriptions>
      <Table<SkillPackageFile>
        rowKey={(record) => record.path}
        size="small"
        columns={fileColumns}
        dataSource={files}
        pagination={{ pageSize: 8, showSizeChanger: false }}
        locale={{
          emptyText: (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('skills.packageNoFiles')} />
          ),
        }}
      />
    </Space>
  );
}

function defaultSkillFormValues(): SkillFormValues {
  return {
    name: '',
    display_name: '',
    description: '',
    content: '',
    locale: 'zh-CN',
    tagsText: '',
    source_url: '',
    enabled: true,
    auto_inject: false,
  };
}

function skillToFormValues(skill: SkillRecord): SkillFormValues {
  return {
    name: skill.name,
    display_name: skill.display_name,
    description: skill.description || '',
    content: skill.content || '',
    locale: skill.locale || 'zh-CN',
    tagsText: (skill.tags || []).join(', '),
    source_url: skill.source_url || '',
    enabled: skill.enabled,
    auto_inject: skill.auto_inject,
  };
}

function buildSkillPayload(values: SkillFormValues): CreateSkillPayload | UpdateSkillPayload {
  return {
    name: normalizedOptional(values.name),
    display_name: values.display_name.trim(),
    description: normalizedOptional(values.description),
    content: normalizedOptional(values.content),
    locale: values.locale || 'zh-CN',
    tags: parseTags(values.tagsText),
    source_url: normalizedOptional(values.source_url),
    enabled: values.enabled ?? true,
    auto_inject: values.auto_inject ?? false,
  };
}

function parseTags(value?: string): string[] {
  return (value || '')
    .split(/[,，\n]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function normalizedOptional(value?: string): string | undefined {
  const trimmed = (value || '').trim();
  return trimmed ? trimmed : undefined;
}

function skillOwnerLabel(
  skill: SkillRecord,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  return (
    userLabel(skill.owner_display_name, skill.owner_username, skill.owner_user_id) ||
    userLabel(skill.creator_display_name, skill.creator_username, skill.creator_user_id) ||
    t('skills.ownerUnassigned')
  );
}

function skillCreatorLabel(
  skill: SkillRecord,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  return (
    userLabel(skill.creator_display_name, skill.creator_username, skill.creator_user_id) ||
    t('skills.ownerUnassigned')
  );
}

function skillOwnerSecondary(
  skill: SkillRecord,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  const ownerId = normalizedOptional(skill.owner_user_id || undefined);
  const creator = userLabel(skill.creator_display_name, skill.creator_username, skill.creator_user_id);
  if (ownerId && creator && skill.creator_user_id !== skill.owner_user_id) {
    return `${t('skills.creator')}: ${creator}`;
  }
  if (ownerId) {
    return ownerId;
  }
  return t('skills.ownerUnassigned');
}

function userLabel(
  displayName?: string | null,
  username?: string | null,
  userId?: string | null,
): string | undefined {
  const primary = normalizedOptional(displayName || undefined);
  const secondary = normalizedOptional(username || undefined);
  const fallback = normalizedOptional(userId || undefined);
  if (primary && secondary && primary !== secondary) {
    return `${primary} (${secondary})`;
  }
  return primary || secondary || fallback;
}

function formatDate(value?: string): string {
  if (!value) {
    return '-';
  }
  return dayjs(value).format('YYYY-MM-DD HH:mm');
}

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) {
    return '0 B';
  }
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = value;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  return `${size >= 10 || unitIndex === 0 ? size.toFixed(0) : size.toFixed(1)} ${units[unitIndex]}`;
}

function sourceColor(source: string): string {
  if (source === 'registry') {
    return 'blue';
  }
  if (source === 'url') {
    return 'purple';
  }
  if (source === 'bundled') {
    return 'cyan';
  }
  return 'default';
}
