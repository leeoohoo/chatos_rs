import { DeleteOutlined, EyeOutlined, FileTextOutlined, LinkOutlined } from '@ant-design/icons';
import { Button, Popconfirm, Space, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import type { ProjectWorkItemRecord, RequirementRecord } from '../../types';
import {
  renderExecutionToolTags,
  requirementStatusTag,
  requirementTypeTag,
  resolveExecutionOptionLabel,
  workItemStatusTag,
} from './renderers';
import type { ExecutionOptionLabelMap, RequirementTableRecord } from './types';

interface ProjectDetailColumnsArgs {
  requirements: RequirementRecord[];
  taskRunnerModelLabelMap: ExecutionOptionLabelMap;
  taskRunnerToolLabelMap: ExecutionOptionLabelMap;
  taskRunnerSkillLabelMap: ExecutionOptionLabelMap;
  onShowRequirementDetail: (record: RequirementRecord) => void;
  onShowRequirementDeps: (record: RequirementRecord) => void;
  onShowRequirementDoc: (record: RequirementRecord) => void;
  onArchiveRequirement: (id: string) => void;
  onShowWorkItemDetail: (record: ProjectWorkItemRecord) => void;
  onShowWorkItemDeps: (record: ProjectWorkItemRecord) => void;
  onArchiveWorkItem: (id: string) => void;
}

export function buildProjectDetailColumns({
  requirements,
  taskRunnerModelLabelMap,
  taskRunnerToolLabelMap,
  taskRunnerSkillLabelMap,
  onShowRequirementDetail,
  onShowRequirementDeps,
  onShowRequirementDoc,
  onArchiveRequirement,
  onShowWorkItemDetail,
  onShowWorkItemDeps,
  onArchiveWorkItem,
}: ProjectDetailColumnsArgs) {
  const requirementColumns: ColumnsType<RequirementTableRecord> = [
    {
      title: '需求',
      dataIndex: 'title',
      render: (_, record) => (
        <div
          className="requirement-title-cell"
          style={{ paddingInlineStart: (record.tree_level || 0) * 28 }}
        >
          {(record.tree_level || 0) > 0 ? <span className="requirement-title-branch" aria-hidden /> : null}
          <Space direction="vertical" size={2} className="requirement-title-content">
            <Typography.Text strong>{record.title}</Typography.Text>
            {record.summary ? <Typography.Text type="secondary">{record.summary}</Typography.Text> : null}
          </Space>
        </div>
      ),
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 120,
      render: (status: RequirementRecord['status']) => requirementStatusTag(status),
    },
    {
      title: '类型',
      dataIndex: 'requirement_type',
      width: 120,
      render: (type: RequirementRecord['requirement_type']) => requirementTypeTag(type),
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
          <Button size="small" icon={<EyeOutlined />} onClick={() => onShowRequirementDetail(record)}>
            详情
          </Button>
          <Button size="small" icon={<LinkOutlined />} onClick={() => onShowRequirementDeps(record)}>
            前置
          </Button>
          <Button size="small" icon={<FileTextOutlined />} onClick={() => onShowRequirementDoc(record)}>
            文档
          </Button>
          <Popconfirm title="归档需求" onConfirm={() => onArchiveRequirement(record.id)}>
            <Button size="small" danger icon={<DeleteOutlined />} disabled={record.status === 'archived'} />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const workItemColumns: ColumnsType<ProjectWorkItemRecord> = [
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
      title: '执行模型',
      dataIndex: 'task_runner_default_model_config_id',
      width: 200,
      render: (modelConfigId: string) => (
        <Typography.Text
          ellipsis={{ tooltip: resolveExecutionOptionLabel(modelConfigId, taskRunnerModelLabelMap) }}
          style={{ maxWidth: 176 }}
        >
          {resolveExecutionOptionLabel(modelConfigId, taskRunnerModelLabelMap)}
        </Typography.Text>
      ),
    },
    {
      title: '工具集',
      dataIndex: 'task_runner_enabled_tool_ids',
      width: 240,
      render: (toolIds: string[]) => renderExecutionToolTags(toolIds, taskRunnerToolLabelMap),
    },
    {
      title: 'Skills',
      dataIndex: 'task_runner_skill_ids',
      width: 220,
      render: (skillIds: string[]) => renderExecutionToolTags(skillIds, taskRunnerSkillLabelMap),
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
          <Button size="small" icon={<EyeOutlined />} onClick={() => onShowWorkItemDetail(record)}>
            详情
          </Button>
          <Button size="small" icon={<LinkOutlined />} onClick={() => onShowWorkItemDeps(record)}>
            前置
          </Button>
          <Popconfirm title="归档项目任务" onConfirm={() => onArchiveWorkItem(record.id)}>
            <Button size="small" danger icon={<DeleteOutlined />} disabled={record.status === 'archived'} />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return { requirementColumns, workItemColumns };
}
