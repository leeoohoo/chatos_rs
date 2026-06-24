import { Fragment, useEffect, useMemo, useState } from 'react';
import type { CSSProperties, ReactNode } from 'react';
import {
  DeleteOutlined,
  EditOutlined,
  EyeOutlined,
  FileTextOutlined,
  LinkOutlined,
  PlusOutlined,
  ReloadOutlined,
  SaveOutlined,
} from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Button,
  Card,
  Col,
  Descriptions,
  Drawer,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Row,
  Select,
  Space,
  Statistic,
  Table,
  Tabs,
  Tag,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';
import { Link, useParams } from 'react-router-dom';

import { api } from '../api/client';
import type {
  CreateRequirementPayload,
  CreateWorkItemPayload,
  DependencyGraphEdge,
  DependencyGraphNode,
  ProjectProfileRecord,
  ProjectWorkItemRecord,
  RequirementDocumentRecord,
  RequirementRecord,
  RequirementStatus,
  UpsertProjectProfilePayload,
} from '../types';

const requirementStatusDisplayOptions = [
  { value: 'draft', label: '草稿' },
  { value: 'reviewing', label: '评审中' },
  { value: 'approved', label: '已确认' },
  { value: 'in_progress', label: '实现中' },
  { value: 'done', label: '已完成' },
  { value: 'cancelled', label: '已取消' },
  { value: 'archived', label: '已归档' },
] satisfies Array<{ value: RequirementStatus; label: string }>;

const requirementStatusOptions = requirementStatusDisplayOptions.filter(
  (option) => option.value !== 'archived',
);

const workItemStatusDisplayOptions = [
  { value: 'todo', label: '待处理' },
  { value: 'ready', label: '已就绪' },
  { value: 'in_progress', label: '进行中' },
  { value: 'blocked', label: '阻塞' },
  { value: 'done', label: '完成' },
  { value: 'cancelled', label: '取消' },
  { value: 'archived', label: '已归档' },
] satisfies Array<{ value: ProjectWorkItemRecord['status']; label: string }>;

const workItemStatusOptions = workItemStatusDisplayOptions.filter(
  (option) => option.value !== 'archived',
);

type WorkItemFormValues = CreateWorkItemPayload & {
  requirement_id: string;
  tags_text?: string;
};

interface DocFormValues {
  title?: string;
  content: string;
}

interface GraphRelationRow {
  key: string;
  edge: DependencyGraphEdge;
  from?: DependencyGraphNode;
  to?: DependencyGraphNode;
}

type MarkdownPreviewBlock =
  | { type: 'heading'; level: 1 | 2 | 3 | 4; text: string }
  | { type: 'paragraph'; text: string }
  | { type: 'ul' | 'ol'; items: string[] }
  | { type: 'blockquote'; text: string }
  | { type: 'code'; language?: string; text: string };

type ProfileMarkdownFieldName = 'background' | 'introduction';

export function ProjectDetailPage() {
  const { projectId } = useParams<{ projectId: string }>();
  const queryClient = useQueryClient();
  const [messageApi, contextHolder] = message.useMessage();
  const [profileForm] = Form.useForm<UpsertProjectProfilePayload>();
  const [requirementForm] = Form.useForm<CreateRequirementPayload>();
  const [workItemForm] = Form.useForm<WorkItemFormValues>();
  const [docForm] = Form.useForm<DocFormValues>();
  const [requirementModalOpen, setRequirementModalOpen] = useState(false);
  const [workItemModalOpen, setWorkItemModalOpen] = useState(false);
  const [requirementDetailTarget, setRequirementDetailTarget] = useState<RequirementRecord | null>(null);
  const [workItemDetailTarget, setWorkItemDetailTarget] = useState<ProjectWorkItemRecord | null>(null);
  const [requirementDepTarget, setRequirementDepTarget] = useState<RequirementRecord | null>(null);
  const [workItemDepTarget, setWorkItemDepTarget] = useState<ProjectWorkItemRecord | null>(null);
  const [requirementDepIds, setRequirementDepIds] = useState<string[]>([]);
  const [workItemDepIds, setWorkItemDepIds] = useState<string[]>([]);
  const [docTarget, setDocTarget] = useState<RequirementRecord | null>(null);
  const [editingProfileField, setEditingProfileField] = useState<ProfileMarkdownFieldName | null>(null);
  const profileBackground = Form.useWatch('background', profileForm);
  const profileIntroduction = Form.useWatch('introduction', profileForm);

  const projectQuery = useQuery({
    queryKey: ['project', projectId],
    queryFn: () => api.getProject(projectId!),
    enabled: Boolean(projectId),
  });
  const profileQuery = useQuery({
    queryKey: ['project-profile', projectId],
    queryFn: () => api.getProjectProfile(projectId!),
    enabled: Boolean(projectId),
  });
  const requirementsQuery = useQuery({
    queryKey: ['requirements', projectId],
    queryFn: () => api.listRequirements(projectId!),
    enabled: Boolean(projectId),
  });
  const workItemsQuery = useQuery({
    queryKey: ['work-items', projectId],
    queryFn: () => api.listProjectWorkItems(projectId!),
    enabled: Boolean(projectId),
  });
  const graphQuery = useQuery({
    queryKey: ['project-graph', projectId],
    queryFn: () => api.getProjectDependencyGraph(projectId!),
    enabled: Boolean(projectId),
  });
  const requirementDepsQuery = useQuery({
    queryKey: ['requirement-deps', requirementDepTarget?.id],
    queryFn: () => api.listRequirementDependencies(requirementDepTarget!.id),
    enabled: Boolean(requirementDepTarget),
  });
  const workItemDepsQuery = useQuery({
    queryKey: ['work-item-deps', workItemDepTarget?.id],
    queryFn: () => api.listWorkItemDependencies(workItemDepTarget!.id),
    enabled: Boolean(workItemDepTarget),
  });
  const docQuery = useQuery({
    queryKey: ['technical-overview', docTarget?.id],
    queryFn: () => api.getRequirementTechnicalOverview(docTarget!.id),
    enabled: Boolean(docTarget),
  });

  useEffect(() => {
    if (profileQuery.data) {
      profileForm.setFieldsValue({
        background: profileQuery.data.background || undefined,
        introduction: profileQuery.data.introduction || undefined,
      });
    }
  }, [profileForm, profileQuery.data]);

  useEffect(() => {
    if (requirementDepsQuery.data) {
      setRequirementDepIds(
        requirementDepsQuery.data.map((item) => item.prerequisite_requirement_id),
      );
    }
  }, [requirementDepsQuery.data]);

  useEffect(() => {
    if (workItemDepsQuery.data) {
      setWorkItemDepIds(workItemDepsQuery.data.map((item) => item.prerequisite_work_item_id));
    }
  }, [workItemDepsQuery.data]);

  useEffect(() => {
    if (docQuery.data) {
      docForm.setFieldsValue({
        title: docQuery.data.title || '实现技术总体文档',
        content: docQuery.data.content || '',
      });
    }
  }, [docForm, docQuery.data]);

  const invalidateProjectData = () => {
    queryClient.invalidateQueries({ queryKey: ['requirements', projectId] });
    queryClient.invalidateQueries({ queryKey: ['work-items', projectId] });
    queryClient.invalidateQueries({ queryKey: ['project-graph', projectId] });
  };

  const profileMutation = useMutation({
    mutationFn: (payload: UpsertProjectProfilePayload) => api.upsertProjectProfile(projectId!, payload),
    onSuccess: (profile: ProjectProfileRecord) => {
      messageApi.success('项目详情已保存');
      setEditingProfileField(null);
      queryClient.setQueryData(['project-profile', projectId], profile);
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const createRequirementMutation = useMutation({
    mutationFn: (payload: CreateRequirementPayload) => api.createRequirement(projectId!, payload),
    onSuccess: () => {
      messageApi.success('需求已创建');
      setRequirementModalOpen(false);
      requirementForm.resetFields();
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const archiveRequirementMutation = useMutation({
    mutationFn: (id: string) => api.archiveRequirement(id),
    onSuccess: () => {
      messageApi.success('需求已归档');
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const saveRequirementDepsMutation = useMutation({
    mutationFn: () => api.setRequirementDependencies(requirementDepTarget!.id, requirementDepIds),
    onSuccess: () => {
      messageApi.success('需求前置关系已保存');
      setRequirementDepTarget(null);
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const saveDocMutation = useMutation({
    mutationFn: (payload: DocFormValues) =>
      api.upsertRequirementTechnicalOverview(docTarget!.id, {
        title: payload.title,
        format: 'markdown',
        content: payload.content || '',
      }),
    onSuccess: (doc: RequirementDocumentRecord) => {
      messageApi.success('技术总体文档已保存');
      queryClient.setQueryData(['technical-overview', doc.requirement_id], doc);
      setDocTarget(null);
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const createWorkItemMutation = useMutation({
    mutationFn: (values: WorkItemFormValues) => {
      const tags = values.tags_text
        ?.split(',')
        .map((item) => item.trim())
        .filter(Boolean);
      const payload: CreateWorkItemPayload = {
        title: values.title,
        description: values.description,
        status: values.status,
        priority: values.priority,
        assignee_user_id: values.assignee_user_id,
        estimate_points: values.estimate_points,
        due_at: values.due_at,
        sort_order: values.sort_order,
        tags,
      };
      return api.createWorkItem(values.requirement_id, payload);
    },
    onSuccess: () => {
      messageApi.success('项目任务已创建');
      setWorkItemModalOpen(false);
      workItemForm.resetFields();
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const archiveWorkItemMutation = useMutation({
    mutationFn: (id: string) => api.archiveWorkItem(id),
    onSuccess: () => {
      messageApi.success('项目任务已归档');
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const saveWorkItemDepsMutation = useMutation({
    mutationFn: () => api.setWorkItemDependencies(workItemDepTarget!.id, workItemDepIds),
    onSuccess: () => {
      messageApi.success('项目任务前置关系已保存');
      setWorkItemDepTarget(null);
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const requirements = requirementsQuery.data || [];
  const workItems = workItemsQuery.data || [];
  const project = projectQuery.data;
  const graphNodeMap = useMemo(() => {
    const nodes = graphQuery.data?.nodes || [];
    return new Map(nodes.map((node) => [node.id, node]));
  }, [graphQuery.data?.nodes]);
  const graphRelations = useMemo<GraphRelationRow[]>(
    () =>
      (graphQuery.data?.edges || []).map((edge, index) => ({
        key: `${edge.from}-${edge.to}-${edge.edge_type}-${index}`,
        edge,
        from: graphNodeMap.get(edge.from),
        to: graphNodeMap.get(edge.to),
      })),
    [graphNodeMap, graphQuery.data?.edges],
  );
  const blockingRelations = graphRelations.filter((item) => item.edge.edge_type === 'blocks');
  const containsRelations = graphRelations.filter((item) => item.edge.edge_type === 'contains');

  const requirementColumns = useMemo<ColumnsType<RequirementRecord>>(
    () => [
      {
        title: '需求',
        dataIndex: 'title',
        render: (_, record) => (
          <Space direction="vertical" size={2}>
            <Typography.Text strong>{record.title}</Typography.Text>
            {record.summary ? <Typography.Text type="secondary">{record.summary}</Typography.Text> : null}
          </Space>
        ),
      },
      {
        title: '状态',
        dataIndex: 'status',
        width: 120,
        render: (status: RequirementRecord['status']) => requirementStatusTag(status),
      },
      {
        title: '优先级',
        dataIndex: 'priority',
        width: 100,
      },
      {
        title: '更新时间',
        dataIndex: 'updated_at',
        width: 180,
        render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
      },
      {
        title: '操作',
        width: 330,
        render: (_, record) => (
          <Space>
            <Button size="small" icon={<EyeOutlined />} onClick={() => setRequirementDetailTarget(record)}>
              详情
            </Button>
            <Button size="small" icon={<LinkOutlined />} onClick={() => setRequirementDepTarget(record)}>
              前置
            </Button>
            <Button size="small" icon={<FileTextOutlined />} onClick={() => setDocTarget(record)}>
              文档
            </Button>
            <Popconfirm title="归档需求" onConfirm={() => archiveRequirementMutation.mutate(record.id)}>
              <Button size="small" danger icon={<DeleteOutlined />} disabled={record.status === 'archived'} />
            </Popconfirm>
          </Space>
        ),
      },
    ],
    [archiveRequirementMutation],
  );

  const workItemColumns = useMemo<ColumnsType<ProjectWorkItemRecord>>(
    () => [
      {
        title: '项目任务',
        dataIndex: 'title',
        render: (_, record) => (
          <Space direction="vertical" size={2}>
            <Typography.Text strong>{record.title}</Typography.Text>
            {record.description ? (
              <Typography.Text type="secondary">{record.description}</Typography.Text>
            ) : null}
          </Space>
        ),
      },
      {
        title: '所属需求',
        dataIndex: 'requirement_id',
        width: 220,
        render: (requirementId: string) =>
          requirements.find((item) => item.id === requirementId)?.title || requirementId,
      },
      {
        title: '状态',
        dataIndex: 'status',
        width: 120,
        render: (status: ProjectWorkItemRecord['status']) => workItemStatusTag(status),
      },
      {
        title: '标签',
        dataIndex: 'tags',
        width: 180,
        render: (tags: string[]) => (
          <Space size={[4, 4]} wrap>
            {tags.map((tag) => (
              <Tag key={tag}>{tag}</Tag>
            ))}
          </Space>
        ),
      },
      {
        title: '更新时间',
        dataIndex: 'updated_at',
        width: 180,
        render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
      },
      {
        title: '操作',
        width: 250,
        render: (_, record) => (
          <Space>
            <Button size="small" icon={<EyeOutlined />} onClick={() => setWorkItemDetailTarget(record)}>
              详情
            </Button>
            <Button size="small" icon={<LinkOutlined />} onClick={() => setWorkItemDepTarget(record)}>
              前置
            </Button>
            <Popconfirm title="归档项目任务" onConfirm={() => archiveWorkItemMutation.mutate(record.id)}>
              <Button size="small" danger icon={<DeleteOutlined />} disabled={record.status === 'archived'} />
            </Popconfirm>
          </Space>
        ),
      },
    ],
    [archiveWorkItemMutation, requirements],
  );

  if (!projectId) {
    return null;
  }

  return (
    <div className="page">
      {contextHolder}
      <div className="page-header">
        <div>
          <Typography.Title level={3} style={{ margin: 0 }}>
            {project?.name || '项目详情'}
          </Typography.Title>
          <Typography.Text type="secondary">
            <Link to="/projects">项目</Link>
            <span> / {projectId}</span>
          </Typography.Text>
        </div>
        <Space>
          <Button
            icon={<ReloadOutlined />}
            onClick={() => {
              projectQuery.refetch();
              profileQuery.refetch();
              requirementsQuery.refetch();
              workItemsQuery.refetch();
              graphQuery.refetch();
            }}
          >
            刷新
          </Button>
        </Space>
      </div>

      <Tabs
        items={[
          {
            key: 'overview',
            label: '概览',
            children: (
              <Space direction="vertical" size="large" style={{ width: '100%' }}>
                <Descriptions bordered column={2} size="small">
                  <Descriptions.Item label="项目名">{project?.name || '-'}</Descriptions.Item>
                  <Descriptions.Item label="状态">
                    {project ? projectStatusTag(project.status) : '-'}
                  </Descriptions.Item>
                  <Descriptions.Item label="根目录">{project?.root_path || '-'}</Descriptions.Item>
                  <Descriptions.Item label="Git">{project?.git_url || '-'}</Descriptions.Item>
                  <Descriptions.Item label="短描述" span={2}>
                    {project?.description || '-'}
                  </Descriptions.Item>
                </Descriptions>
                <Row gutter={16}>
                  <Col xs={24} md={8}>
                    <Card>
                      <Statistic title="需求数" value={requirements.length} />
                    </Card>
                  </Col>
                  <Col xs={24} md={8}>
                    <Card>
                      <Statistic title="项目任务数" value={workItems.length} />
                    </Card>
                  </Col>
                  <Col xs={24} md={8}>
                    <Card>
                      <Statistic
                        title="阻塞任务"
                        value={workItems.filter((item) => item.status === 'blocked').length}
                      />
                    </Card>
                  </Col>
                </Row>
              </Space>
            ),
          },
          {
            key: 'profile',
            label: '项目详情',
            children: (
              <Form<UpsertProjectProfilePayload>
                form={profileForm}
                layout="vertical"
                onFinish={(values) => profileMutation.mutate(values)}
                style={profileFormStyle}
              >
                <div style={profileToolbarStyle}>
                  <Typography.Title level={4} style={{ margin: 0 }}>
                    项目文档
                  </Typography.Title>
                </div>
                <ProfileMarkdownField
                  title="项目背景"
                  name="background"
                  value={typeof profileBackground === 'string' ? profileBackground : undefined}
                  editing={editingProfileField === 'background'}
                  saving={profileMutation.isPending}
                  onEdit={() => setEditingProfileField('background')}
                  onCancel={() => setEditingProfileField(null)}
                />
                <ProfileMarkdownField
                  title="项目介绍"
                  name="introduction"
                  value={typeof profileIntroduction === 'string' ? profileIntroduction : undefined}
                  editing={editingProfileField === 'introduction'}
                  saving={profileMutation.isPending}
                  onEdit={() => setEditingProfileField('introduction')}
                  onCancel={() => setEditingProfileField(null)}
                />
              </Form>
            ),
          },
          {
            key: 'requirements',
            label: '需求',
            children: (
              <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                <div className="toolbar">
                  <Button type="primary" icon={<PlusOutlined />} onClick={() => setRequirementModalOpen(true)}>
                    新建需求
                  </Button>
                </div>
                <Table<RequirementRecord>
                  rowKey="id"
                  loading={requirementsQuery.isLoading}
                  columns={requirementColumns}
                  dataSource={requirements}
                  pagination={{ pageSize: 8, showSizeChanger: true }}
                  scroll={{ x: 1100 }}
                />
              </Space>
            ),
          },
          {
            key: 'work-items',
            label: '项目任务',
            children: (
              <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                <div className="toolbar">
                  <Button
                    type="primary"
                    icon={<PlusOutlined />}
                    disabled={requirements.length === 0}
                    onClick={() => setWorkItemModalOpen(true)}
                  >
                    新建项目任务
                  </Button>
                </div>
                <Table<ProjectWorkItemRecord>
                  rowKey="id"
                  loading={workItemsQuery.isLoading}
                  columns={workItemColumns}
                  dataSource={workItems}
                  pagination={{ pageSize: 8, showSizeChanger: true }}
                  scroll={{ x: 1200 }}
                />
              </Space>
            ),
          },
          {
            key: 'graph',
            label: '依赖图',
            children: (
              <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                <Row gutter={16}>
                  <Col xs={24} md={8}>
                    <Card>
                      <Statistic
                        title="需求"
                        value={(graphQuery.data?.nodes || []).filter((node) => node.node_type === 'requirement').length}
                      />
                    </Card>
                  </Col>
                  <Col xs={24} md={8}>
                    <Card>
                      <Statistic
                        title="项目任务"
                        value={(graphQuery.data?.nodes || []).filter((node) => node.node_type === 'work_item').length}
                      />
                    </Card>
                  </Col>
                  <Col xs={24} md={8}>
                    <Card>
                      <Statistic title="阻塞关系" value={blockingRelations.length} />
                    </Card>
                  </Col>
                </Row>
                <Table<GraphRelationRow>
                  rowKey="key"
                  size="small"
                  loading={graphQuery.isLoading}
                  dataSource={blockingRelations}
                  pagination={false}
                  title={() => '阻塞关系'}
                  locale={{ emptyText: '暂无阻塞关系' }}
                  columns={[
                    {
                      title: '前置项',
                      render: (_, record) => renderGraphNode(record.from, record.edge.from),
                    },
                    {
                      title: '被阻塞项',
                      render: (_, record) => renderGraphNode(record.to, record.edge.to),
                    },
                    {
                      title: '关系',
                      width: 120,
                      render: () => <Tag color="error">阻塞</Tag>,
                    },
                  ]}
                />
                <Table<GraphRelationRow>
                  rowKey="key"
                  size="small"
                  loading={graphQuery.isLoading}
                  dataSource={containsRelations}
                  pagination={{ pageSize: 8 }}
                  title={() => '需求拆分'}
                  locale={{ emptyText: '暂无项目任务' }}
                  columns={[
                    {
                      title: '需求',
                      render: (_, record) => renderGraphNode(record.from, record.edge.from),
                    },
                    {
                      title: '项目任务',
                      render: (_, record) => renderGraphNode(record.to, record.edge.to),
                    },
                    {
                      title: '关系',
                      width: 120,
                      render: () => <Tag color="blue">拆分</Tag>,
                    },
                  ]}
                />
                <Table<DependencyGraphNode>
                  rowKey="id"
                  size="small"
                  loading={graphQuery.isLoading}
                  dataSource={graphQuery.data?.nodes || []}
                  pagination={{ pageSize: 8 }}
                  title={() => '对象清单'}
                  columns={[
                    {
                      title: '对象',
                      render: (_, record) => renderGraphNode(record, record.id),
                    },
                    {
                      title: '状态',
                      width: 140,
                      render: (_, record) => graphStatusTag(record),
                    },
                  ]}
                />
              </Space>
            ),
          },
        ]}
      />

      <Modal
        title="新建需求"
        open={requirementModalOpen}
        onCancel={() => setRequirementModalOpen(false)}
        onOk={() => requirementForm.submit()}
        confirmLoading={createRequirementMutation.isPending}
        destroyOnClose
      >
        <Form<CreateRequirementPayload>
          form={requirementForm}
          layout="vertical"
          initialValues={{ status: 'draft', priority: 0 }}
          onFinish={(values) => createRequirementMutation.mutate(values)}
        >
          <Form.Item name="title" label="标题" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="summary" label="摘要">
            <Input.TextArea rows={3} />
          </Form.Item>
          <Form.Item name="detail" label="详情">
            <Input.TextArea rows={5} />
          </Form.Item>
          <Form.Item name="acceptance_criteria" label="验收标准">
            <Input.TextArea rows={4} />
          </Form.Item>
          <Form.Item name="status" label="状态">
            <Select options={requirementStatusOptions} />
          </Form.Item>
          <Form.Item name="priority" label="优先级">
            <InputNumber style={{ width: '100%' }} />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="设置需求前置关系"
        open={Boolean(requirementDepTarget)}
        onCancel={() => setRequirementDepTarget(null)}
        onOk={() => saveRequirementDepsMutation.mutate()}
        confirmLoading={saveRequirementDepsMutation.isPending}
        destroyOnClose
      >
        <Typography.Paragraph>{requirementDepTarget?.title}</Typography.Paragraph>
        <Select
          mode="multiple"
          style={{ width: '100%' }}
          loading={requirementDepsQuery.isLoading}
          value={requirementDepIds}
          onChange={setRequirementDepIds}
          options={requirements
            .filter((item) => item.id !== requirementDepTarget?.id)
            .map((item) => ({ value: item.id, label: item.title }))}
        />
      </Modal>

      <Modal
        title="实现技术总体文档"
        open={Boolean(docTarget)}
        onCancel={() => setDocTarget(null)}
        onOk={() => docForm.submit()}
        width={900}
        confirmLoading={saveDocMutation.isPending}
        destroyOnClose
      >
        <Form<DocFormValues> form={docForm} layout="vertical" onFinish={(values) => saveDocMutation.mutate(values)}>
          <Form.Item name="title" label="标题">
            <Input />
          </Form.Item>
          <Form.Item name="content" label="内容">
            <Input.TextArea rows={18} />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="新建项目任务"
        open={workItemModalOpen}
        onCancel={() => setWorkItemModalOpen(false)}
        onOk={() => workItemForm.submit()}
        confirmLoading={createWorkItemMutation.isPending}
        destroyOnClose
      >
        <Form<WorkItemFormValues>
          form={workItemForm}
          layout="vertical"
          initialValues={{ status: 'todo', priority: 0, sort_order: 0 }}
          onFinish={(values) => createWorkItemMutation.mutate(values)}
        >
          <Form.Item
            name="requirement_id"
            label="所属需求"
            rules={[
              { required: true },
              {
                validator: async (_, value?: string) => {
                  if (!value) {
                    return;
                  }
                  const doc = await api.getRequirementTechnicalOverview(value);
                  if (!doc.content?.trim()) {
                    throw new Error('创建项目任务前，请先填写该需求的实现技术总体文档内容');
                  }
                },
              },
            ]}
          >
            <Select options={requirements.map((item) => ({ value: item.id, label: item.title }))} />
          </Form.Item>
          <Form.Item name="title" label="标题" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input.TextArea rows={4} />
          </Form.Item>
          <Form.Item name="status" label="状态">
            <Select options={workItemStatusOptions} />
          </Form.Item>
          <Row gutter={12}>
            <Col span={12}>
              <Form.Item name="priority" label="优先级">
                <InputNumber style={{ width: '100%' }} />
              </Form.Item>
            </Col>
            <Col span={12}>
              <Form.Item name="estimate_points" label="估算点数">
                <InputNumber style={{ width: '100%' }} min={0} />
              </Form.Item>
            </Col>
          </Row>
          <Form.Item name="tags_text" label="标签">
            <Input placeholder="frontend,api" />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="设置项目任务前置关系"
        open={Boolean(workItemDepTarget)}
        onCancel={() => setWorkItemDepTarget(null)}
        onOk={() => saveWorkItemDepsMutation.mutate()}
        confirmLoading={saveWorkItemDepsMutation.isPending}
        destroyOnClose
      >
        <Typography.Paragraph>{workItemDepTarget?.title}</Typography.Paragraph>
        <Select
          mode="multiple"
          style={{ width: '100%' }}
          loading={workItemDepsQuery.isLoading}
          value={workItemDepIds}
          onChange={setWorkItemDepIds}
          options={workItems
            .filter((item) => item.id !== workItemDepTarget?.id)
            .map((item) => ({ value: item.id, label: item.title }))}
        />
      </Modal>

      <Drawer
        title="需求详情"
        open={Boolean(requirementDetailTarget)}
        onClose={() => setRequirementDetailTarget(null)}
        width="min(1120px, calc(100vw - 48px))"
        styles={{ body: { padding: 0, background: '#f6f7f9' } }}
        destroyOnClose
      >
        {requirementDetailTarget ? (
          <RequirementDetailPreview requirement={requirementDetailTarget} />
        ) : null}
      </Drawer>

      <Drawer
        title="项目任务详情"
        open={Boolean(workItemDetailTarget)}
        onClose={() => setWorkItemDetailTarget(null)}
        width="min(1120px, calc(100vw - 48px))"
        styles={{ body: { padding: 0, background: '#f6f7f9' } }}
        destroyOnClose
      >
        {workItemDetailTarget ? (
          <WorkItemDetailPreview
            workItem={workItemDetailTarget}
            requirementTitle={
              requirements.find((item) => item.id === workItemDetailTarget.requirement_id)?.title ||
              workItemDetailTarget.requirement_id
            }
          />
        ) : null}
      </Drawer>
    </div>
  );
}

function ProfileMarkdownField({
  title,
  name,
  value,
  editing,
  saving,
  onEdit,
  onCancel,
}: {
  title: string;
  name: ProfileMarkdownFieldName;
  value?: string;
  editing: boolean;
  saving: boolean;
  onEdit: () => void;
  onCancel: () => void;
}) {
  const hasContent = Boolean(value?.trim());

  return (
    <section style={profileMarkdownSectionStyle}>
      <div style={profileMarkdownSectionHeaderStyle}>
        <Space size={8} wrap>
          <Typography.Title level={4} style={{ margin: 0 }}>
            {title}
          </Typography.Title>
          <Tag color="blue">Markdown</Tag>
        </Space>
        {editing ? (
          <Space>
            <Button onClick={onCancel}>取消</Button>
            <Button type="primary" icon={<SaveOutlined />} htmlType="submit" loading={saving}>
              保存
            </Button>
          </Space>
        ) : (
          <Button icon={<EditOutlined />} onClick={onEdit}>
            编辑
          </Button>
        )}
      </div>
      {editing ? (
        <div style={profileEditorLayoutStyle}>
          <Form.Item name={name} style={{ marginBottom: 0 }}>
            <Input.TextArea
              autoSize={{ minRows: 20, maxRows: 36 }}
              style={profileTextAreaStyle}
              placeholder={`用 Markdown 编写${title}`}
            />
          </Form.Item>
        </div>
      ) : (
        <div style={hasContent ? profilePreviewOnlyStyle : profileEmptyPreviewStyle}>
          <MarkdownPreview value={value} />
        </div>
      )}
    </section>
  );
}

function RequirementDetailPreview({ requirement }: { requirement: RequirementRecord }) {
  return (
    <div style={detailPreviewShellStyle}>
      <section style={detailPreviewHeaderStyle}>
        <Space direction="vertical" size={8} style={{ width: '100%' }}>
          <Space size={8} wrap>
            {requirementStatusTag(requirement.status)}
            <Tag>优先级 {requirement.priority}</Tag>
            {requirement.source ? <Tag>{requirement.source}</Tag> : null}
          </Space>
          <Typography.Title level={3} style={detailPreviewTitleStyle}>
            {requirement.title}
          </Typography.Title>
        </Space>
      </section>

      <section style={detailPreviewMetaStyle}>
        <Descriptions bordered column={{ xs: 1, sm: 2, lg: 3 }} size="small">
          <Descriptions.Item label="负责人">{requirement.assignee_user_id || '-'}</Descriptions.Item>
          <Descriptions.Item label="创建时间">{formatDateTime(requirement.created_at)}</Descriptions.Item>
          <Descriptions.Item label="更新时间">{formatDateTime(requirement.updated_at)}</Descriptions.Item>
          <Descriptions.Item label="归档时间">{formatDateTime(requirement.archived_at)}</Descriptions.Item>
        </Descriptions>
      </section>

      <main style={markdownSectionsStyle}>
        <MarkdownPreviewSection title="摘要" value={requirement.summary} />
        <MarkdownPreviewSection title="需求详情" value={requirement.detail} />
        <MarkdownPreviewSection title="业务价值" value={requirement.business_value} />
        <MarkdownPreviewSection title="验收标准" value={requirement.acceptance_criteria} />
      </main>
    </div>
  );
}

function WorkItemDetailPreview({
  workItem,
  requirementTitle,
}: {
  workItem: ProjectWorkItemRecord;
  requirementTitle: string;
}) {
  return (
    <div style={detailPreviewShellStyle}>
      <section style={detailPreviewHeaderStyle}>
        <Space direction="vertical" size={8} style={{ width: '100%' }}>
          <Space size={8} wrap>
            {workItemStatusTag(workItem.status)}
            <Tag>优先级 {workItem.priority}</Tag>
            {workItem.tags.map((tag) => (
              <Tag key={tag}>{tag}</Tag>
            ))}
          </Space>
          <Typography.Title level={3} style={detailPreviewTitleStyle}>
            {workItem.title}
          </Typography.Title>
        </Space>
      </section>

      <section style={detailPreviewMetaStyle}>
        <Descriptions bordered column={{ xs: 1, sm: 2, lg: 3 }} size="small">
          <Descriptions.Item label="所属需求">{requirementTitle}</Descriptions.Item>
          <Descriptions.Item label="估算点数">{workItem.estimate_points ?? '-'}</Descriptions.Item>
          <Descriptions.Item label="计划完成">{formatDateTime(workItem.due_at)}</Descriptions.Item>
          <Descriptions.Item label="排序">{workItem.sort_order}</Descriptions.Item>
          <Descriptions.Item label="负责人">{workItem.assignee_user_id || '-'}</Descriptions.Item>
          <Descriptions.Item label="创建时间">{formatDateTime(workItem.created_at)}</Descriptions.Item>
          <Descriptions.Item label="更新时间">{formatDateTime(workItem.updated_at)}</Descriptions.Item>
          <Descriptions.Item label="归档时间">{formatDateTime(workItem.archived_at)}</Descriptions.Item>
        </Descriptions>
      </section>

      <main style={markdownSectionsStyle}>
        <MarkdownPreviewSection title="任务描述" value={workItem.description} />
      </main>
    </div>
  );
}

function projectStatusTag(status: 'active' | 'archived') {
  return <Tag color={status === 'active' ? 'success' : 'default'}>{status === 'active' ? '进行中' : '已归档'}</Tag>;
}

function renderGraphNode(node: DependencyGraphNode | undefined, fallback: string) {
  const label = node?.label?.trim() || fallback;
  const rawId = node?.raw_id || fallback;
  return (
    <Space direction="vertical" size={0}>
      <Space size={6} wrap>
        <Tag color={graphNodeTypeColor(node?.node_type)}>{graphNodeTypeLabel(node?.node_type)}</Tag>
        <Typography.Text strong>{label}</Typography.Text>
        {node ? graphStatusTag(node) : null}
      </Space>
      <Typography.Text type="secondary">#{shortGraphId(rawId)}</Typography.Text>
    </Space>
  );
}

function graphNodeTypeLabel(type?: string) {
  if (type === 'requirement') {
    return '需求';
  }
  if (type === 'work_item') {
    return '项目任务';
  }
  return '对象';
}

function graphNodeTypeColor(type?: string) {
  if (type === 'requirement') {
    return 'geekblue';
  }
  if (type === 'work_item') {
    return 'cyan';
  }
  return 'default';
}

function shortGraphId(value: string) {
  const raw = value.split(':').pop()?.trim() || value.trim();
  return raw.length > 8 ? raw.slice(0, 8) : raw;
}

function graphStatusTag(node: DependencyGraphNode) {
  if (node.node_type === 'requirement') {
    return requirementStatusTag(node.status as RequirementRecord['status']);
  }
  if (node.node_type === 'work_item') {
    return workItemStatusTag(node.status as ProjectWorkItemRecord['status']);
  }
  return <Tag>{node.status || '-'}</Tag>;
}

function requirementStatusTag(status: RequirementRecord['status']) {
  const item = requirementStatusDisplayOptions.find((option) => option.value === status);
  const color = status === 'done' ? 'success' : status === 'cancelled' || status === 'archived' ? 'default' : 'processing';
  return <Tag color={color}>{item?.label || status}</Tag>;
}

function workItemStatusTag(status: ProjectWorkItemRecord['status']) {
  const item = workItemStatusDisplayOptions.find((option) => option.value === status);
  const color =
    status === 'done'
      ? 'success'
      : status === 'blocked'
        ? 'error'
        : status === 'cancelled' || status === 'archived'
          ? 'default'
          : 'processing';
  return <Tag color={color}>{item?.label || status}</Tag>;
}

function MarkdownPreviewSection({ title, value }: { title: string; value?: string | null }) {
  return (
    <section style={markdownSectionStyle}>
      <div style={markdownSectionHeaderStyle}>
        <Typography.Title level={4} style={{ margin: 0 }}>
          {title}
        </Typography.Title>
        <Tag color="blue">Markdown</Tag>
      </div>
      <MarkdownPreview value={value} />
    </section>
  );
}

function MarkdownPreview({ value }: { value?: string | null }) {
  const text = value?.trim();
  if (!text) {
    return (
      <div style={markdownEmptyStyle}>
        <Typography.Text type="secondary">暂无内容</Typography.Text>
      </div>
    );
  }

  const blocks = parseMarkdownBlocks(text);
  return (
    <div style={markdownPreviewStyle}>
      {blocks.map((block, index) => renderMarkdownBlock(block, index))}
    </div>
  );
}

function parseMarkdownBlocks(text: string): MarkdownPreviewBlock[] {
  const blocks: MarkdownPreviewBlock[] = [];
  const lines = text.replace(/\r\n/g, '\n').split('\n');
  let paragraphLines: string[] = [];
  let listType: 'ul' | 'ol' | null = null;
  let listItems: string[] = [];
  let quoteLines: string[] = [];
  let inCode = false;
  let codeLanguage = '';
  let codeLines: string[] = [];

  const flushParagraph = () => {
    if (paragraphLines.length > 0) {
      blocks.push({ type: 'paragraph', text: paragraphLines.join('\n').trim() });
      paragraphLines = [];
    }
  };
  const flushList = () => {
    if (listType && listItems.length > 0) {
      blocks.push({ type: listType, items: listItems });
      listType = null;
      listItems = [];
    }
  };
  const flushQuote = () => {
    if (quoteLines.length > 0) {
      blocks.push({ type: 'blockquote', text: quoteLines.join('\n').trim() });
      quoteLines = [];
    }
  };
  const flushTextBlocks = () => {
    flushParagraph();
    flushList();
    flushQuote();
  };

  for (const line of lines) {
    const fenceMatch = line.match(/^\s*```(.*)$/);
    if (fenceMatch) {
      if (inCode) {
        blocks.push({ type: 'code', language: codeLanguage || undefined, text: codeLines.join('\n') });
        inCode = false;
        codeLanguage = '';
        codeLines = [];
      } else {
        flushTextBlocks();
        inCode = true;
        codeLanguage = fenceMatch[1].trim();
      }
      continue;
    }

    if (inCode) {
      codeLines.push(line);
      continue;
    }

    if (!line.trim()) {
      flushTextBlocks();
      continue;
    }

    const headingMatch = line.match(/^(#{1,4})\s+(.+)$/);
    if (headingMatch) {
      flushTextBlocks();
      blocks.push({
        type: 'heading',
        level: headingMatch[1].length as 1 | 2 | 3 | 4,
        text: headingMatch[2].trim(),
      });
      continue;
    }

    const unorderedMatch = line.match(/^\s*[-*+]\s+(.+)$/);
    if (unorderedMatch) {
      flushParagraph();
      flushQuote();
      if (listType !== 'ul') {
        flushList();
        listType = 'ul';
      }
      listItems.push(unorderedMatch[1].trim());
      continue;
    }

    const orderedMatch = line.match(/^\s*\d+[.)]\s+(.+)$/);
    if (orderedMatch) {
      flushParagraph();
      flushQuote();
      if (listType !== 'ol') {
        flushList();
        listType = 'ol';
      }
      listItems.push(orderedMatch[1].trim());
      continue;
    }

    const quoteMatch = line.match(/^\s*>\s?(.*)$/);
    if (quoteMatch) {
      flushParagraph();
      flushList();
      quoteLines.push(quoteMatch[1]);
      continue;
    }

    flushList();
    flushQuote();
    paragraphLines.push(line);
  }

  if (inCode) {
    blocks.push({ type: 'code', language: codeLanguage || undefined, text: codeLines.join('\n') });
  }
  flushTextBlocks();
  return blocks;
}

function renderMarkdownBlock(block: MarkdownPreviewBlock, index: number) {
  if (block.type === 'heading') {
    const level = Math.min(block.level + 2, 5) as 3 | 4 | 5;
    return (
      <Typography.Title key={index} level={level} style={markdownHeadingStyle}>
        {renderInlineMarkdown(block.text)}
      </Typography.Title>
    );
  }

  if (block.type === 'paragraph') {
    return (
      <Typography.Paragraph key={index} style={markdownParagraphStyle}>
        {renderInlineMarkdown(block.text)}
      </Typography.Paragraph>
    );
  }

  if (block.type === 'ul') {
    return (
      <ul key={index} style={markdownListStyle}>
        {block.items.map((item, itemIndex) => (
          <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
        ))}
      </ul>
    );
  }

  if (block.type === 'ol') {
    return (
      <ol key={index} style={markdownListStyle}>
        {block.items.map((item, itemIndex) => (
          <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
        ))}
      </ol>
    );
  }

  if (block.type === 'blockquote') {
    return (
      <blockquote key={index} style={markdownBlockquoteStyle}>
        {block.text.split('\n').map((line, lineIndex) => (
          <Fragment key={lineIndex}>
            {lineIndex > 0 ? <br /> : null}
            {renderInlineMarkdown(line)}
          </Fragment>
        ))}
      </blockquote>
    );
  }

  if (block.type === 'code') {
    return (
      <pre key={index} style={markdownCodeBlockStyle}>
        {block.language ? <div style={markdownCodeLanguageStyle}>{block.language}</div> : null}
        <code>{block.text}</code>
      </pre>
    );
  }

  return null;
}

function renderInlineMarkdown(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\(https?:\/\/[^)\s]+\))/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > lastIndex) {
      nodes.push(text.slice(lastIndex, match.index));
    }
    const token = match[0];
    const key = `${match.index}-${token.length}`;
    const linkMatch = token.match(/^\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)$/);
    if (token.startsWith('`')) {
      nodes.push(
        <Typography.Text key={key} code>
          {token.slice(1, -1)}
        </Typography.Text>,
      );
    } else if (token.startsWith('**')) {
      nodes.push(<strong key={key}>{token.slice(2, -2)}</strong>);
    } else if (linkMatch) {
      nodes.push(
        <Typography.Link key={key} href={linkMatch[2]} target="_blank" rel="noreferrer">
          {linkMatch[1]}
        </Typography.Link>,
      );
    }
    lastIndex = pattern.lastIndex;
  }

  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex));
  }
  return nodes;
}

function formatDateTime(value?: string | null) {
  return value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-';
}

const profileFormStyle: CSSProperties = {
  maxWidth: 1280,
};

const profileToolbarStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 16,
  marginBottom: 16,
};

const profileMarkdownSectionStyle: CSSProperties = {
  marginBottom: 18,
  background: '#fff',
  border: '1px solid #eceff3',
  borderRadius: 8,
  overflow: 'hidden',
};

const profileMarkdownSectionHeaderStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '14px 18px',
  borderBottom: '1px solid #eef0f3',
};

const profileEditorLayoutStyle: CSSProperties = {
  padding: 18,
};

const profileTextAreaStyle: CSSProperties = {
  minHeight: 520,
  fontFamily:
    'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
  lineHeight: 1.7,
  resize: 'vertical',
};

const profilePreviewOnlyStyle: CSSProperties = {
  minHeight: 220,
  maxHeight: 680,
  overflow: 'auto',
  background: '#fff',
};

const profileEmptyPreviewStyle: CSSProperties = {
  minHeight: 120,
  background: '#fff',
};

const detailPreviewShellStyle: CSSProperties = {
  minHeight: '100%',
};

const detailPreviewHeaderStyle: CSSProperties = {
  padding: '24px 32px 18px',
  background: '#fff',
  borderBottom: '1px solid #eef0f3',
};

const detailPreviewTitleStyle: CSSProperties = {
  margin: 0,
  lineHeight: 1.35,
  letterSpacing: 0,
};

const detailPreviewMetaStyle: CSSProperties = {
  padding: '16px 32px 0',
  background: '#f6f7f9',
};

const markdownSectionsStyle: CSSProperties = {
  display: 'grid',
  gap: 16,
  padding: '16px 32px 32px',
};

const markdownSectionStyle: CSSProperties = {
  background: '#fff',
  border: '1px solid #eceff3',
  borderRadius: 8,
  overflow: 'hidden',
};

const markdownSectionHeaderStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '14px 18px',
  borderBottom: '1px solid #eef0f3',
};

const markdownPreviewStyle: CSSProperties = {
  padding: '18px 22px',
  color: '#1f2328',
  fontSize: 14,
  lineHeight: 1.75,
  overflowX: 'auto',
};

const markdownEmptyStyle: CSSProperties = {
  padding: '24px 22px',
};

const markdownHeadingStyle: CSSProperties = {
  marginTop: 18,
  marginBottom: 8,
  lineHeight: 1.35,
  letterSpacing: 0,
};

const markdownParagraphStyle: CSSProperties = {
  marginBottom: 12,
  whiteSpace: 'pre-wrap',
};

const markdownListStyle: CSSProperties = {
  marginTop: 0,
  marginBottom: 12,
  paddingLeft: 24,
};

const markdownBlockquoteStyle: CSSProperties = {
  margin: '0 0 12px',
  padding: '10px 14px',
  borderLeft: '4px solid #d6e4ff',
  background: '#f5f8ff',
  color: '#475467',
};

const markdownCodeBlockStyle: CSSProperties = {
  margin: '0 0 12px',
  padding: '14px 16px',
  borderRadius: 6,
  background: '#111827',
  color: '#f9fafb',
  overflowX: 'auto',
  fontSize: 13,
  lineHeight: 1.65,
};

const markdownCodeLanguageStyle: CSSProperties = {
  marginBottom: 8,
  color: '#9ca3af',
  fontSize: 12,
};
