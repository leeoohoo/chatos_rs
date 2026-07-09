// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo, useState } from 'react';
import { DeleteOutlined, EditOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Button,
  Form,
  Input,
  Modal,
  Popconfirm,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';
import { useNavigate } from 'react-router-dom';

import { api } from '../api/client';
import type { CreateProjectPayload, ProjectRecord } from '../types';

export function ProjectsPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [messageApi, contextHolder] = message.useMessage();
  const [modalOpen, setModalOpen] = useState(false);
  const [editingProject, setEditingProject] = useState<ProjectRecord | null>(null);
  const [form] = Form.useForm<CreateProjectPayload>();

  const projectsQuery = useQuery({
    queryKey: ['projects'],
    queryFn: () => api.listProjects(),
  });

  const saveMutation = useMutation({
    mutationFn: async (values: CreateProjectPayload) => {
      if (editingProject) {
        return api.updateProject(editingProject.id, values);
      }
      return api.createProject(values);
    },
    onSuccess: () => {
      messageApi.success('已保存项目');
      setModalOpen(false);
      setEditingProject(null);
      form.resetFields();
      queryClient.invalidateQueries({ queryKey: ['projects'] });
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const archiveMutation = useMutation({
    mutationFn: (id: string) => api.archiveProject(id),
    onSuccess: () => {
      messageApi.success('项目已归档');
      queryClient.invalidateQueries({ queryKey: ['projects'] });
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const columns = useMemo<ColumnsType<ProjectRecord>>(
    () => [
      {
        title: '项目',
        dataIndex: 'name',
        width: 260,
        render: (_, record) => (
          <Space direction="vertical" size={2}>
            <Button type="link" style={{ padding: 0 }} onClick={() => navigate(`/projects/${record.id}`)}>
              {record.name}
            </Button>
            <Typography.Text type="secondary" copyable>
              {record.id}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: '状态',
        dataIndex: 'status',
        width: 120,
        render: (status: ProjectRecord['status']) => (
          <Tag color={status === 'active' ? 'success' : 'default'}>
            {status === 'active' ? '进行中' : '已归档'}
          </Tag>
        ),
      },
      {
        title: '根目录',
        dataIndex: 'root_path',
        ellipsis: true,
        render: (value?: string | null) => value || '-',
      },
      {
        title: 'Git',
        dataIndex: 'git_url',
        ellipsis: true,
        render: (value?: string | null) => value || '-',
      },
      {
        title: 'Owner',
        width: 180,
        render: (_, record) =>
          record.owner_display_name || record.owner_username || record.owner_user_id || '-',
      },
      {
        title: '更新时间',
        dataIndex: 'updated_at',
        width: 180,
        render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
      },
      {
        title: '操作',
        width: 160,
        render: (_, record) => (
          <Space>
            <Button
              size="small"
              icon={<EditOutlined />}
              onClick={() => {
                setEditingProject(record);
                form.resetFields();
                form.setFieldsValue({
                  name: record.name,
                  root_path: record.root_path || undefined,
                  git_url: record.git_url || undefined,
                  description: record.description || undefined,
                });
                setModalOpen(true);
              }}
            />
            <Popconfirm
              title="归档项目"
              description="归档后项目下内容将变为只读。"
              onConfirm={() => archiveMutation.mutate(record.id)}
            >
              <Button size="small" danger icon={<DeleteOutlined />} disabled={record.status === 'archived'} />
            </Popconfirm>
          </Space>
        ),
      },
    ],
    [archiveMutation, form, navigate],
  );

  return (
    <div className="page">
      {contextHolder}
      <div className="page-header">
        <div>
          <Typography.Title level={3} style={{ margin: 0 }}>
            项目
          </Typography.Title>
          <Typography.Text type="secondary">项目基础信息、项目背景、需求和项目任务的入口。</Typography.Text>
        </div>
        <Space>
          <Button icon={<ReloadOutlined />} onClick={() => projectsQuery.refetch()}>
            刷新
          </Button>
          <Button
            type="primary"
            icon={<PlusOutlined />}
            onClick={() => {
              setEditingProject(null);
              form.resetFields();
              form.setFieldsValue({ sandbox_enabled: true });
              setModalOpen(true);
            }}
          >
            新建项目
          </Button>
        </Space>
      </div>

      <Table<ProjectRecord>
        rowKey="id"
        loading={projectsQuery.isLoading}
        columns={columns}
        dataSource={projectsQuery.data || []}
        pagination={{ pageSize: 10, showSizeChanger: true }}
        scroll={{ x: 1200 }}
      />

      <Modal
        title={editingProject ? '编辑项目' : '新建项目'}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        onOk={() => form.submit()}
        confirmLoading={saveMutation.isPending}
        destroyOnClose
      >
        <Form<CreateProjectPayload>
          form={form}
          layout="vertical"
          initialValues={{ sandbox_enabled: true }}
          onFinish={(values) => saveMutation.mutate(values)}
        >
          <Form.Item name="name" label="项目名" rules={[{ required: true, message: '请输入项目名' }]}>
            <Input />
          </Form.Item>
          <Form.Item name="root_path" label="根目录">
            <Input placeholder="/path/to/project" />
          </Form.Item>
          <Form.Item name="git_url" label="Git 地址">
            <Input placeholder="git@github.com:org/repo.git" />
          </Form.Item>
          <Form.Item name="description" label="短描述">
            <Input.TextArea rows={3} />
          </Form.Item>
          {!editingProject ? (
            <Form.Item
              name="sandbox_enabled"
              label="使用沙箱初始化"
              valuePropName="checked"
              preserve={false}
            >
              <Switch checkedChildren="是" unCheckedChildren="否" />
            </Form.Item>
          ) : null}
        </Form>
      </Modal>
    </div>
  );
}
