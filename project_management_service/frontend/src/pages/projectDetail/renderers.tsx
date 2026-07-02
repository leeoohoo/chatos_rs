// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { EditOutlined, SaveOutlined } from '@ant-design/icons';
import { Button, Descriptions, Form, Input, Space, Tag, Typography } from 'antd';
import dayjs from 'dayjs';

import { MarkdownPreview, MarkdownPreviewSection } from '../../components/MarkdownPreview';
import type { DependencyGraphNode, ProjectWorkItemRecord, RequirementRecord } from '../../types';
import {
  requirementStatusDisplayOptions,
  requirementTypeOptions,
  workItemStatusDisplayOptions,
} from './options';
import {
  detailPreviewHeaderStyle,
  detailPreviewMetaStyle,
  detailPreviewShellStyle,
  detailPreviewTitleStyle,
  markdownSectionsStyle,
  profileEditorLayoutStyle,
  profileEmptyPreviewStyle,
  profileMarkdownSectionHeaderStyle,
  profileMarkdownSectionStyle,
  profilePreviewOnlyStyle,
  profileTextAreaStyle,
} from './styles';
import type { ExecutionOptionLabelMap, ProfileMarkdownFieldName } from './types';

export function ProfileMarkdownField({
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

export function RequirementDetailPreview({ requirement }: { requirement: RequirementRecord }) {
  return (
    <div style={detailPreviewShellStyle}>
      <section style={detailPreviewHeaderStyle}>
        <Space direction="vertical" size={8} style={{ width: '100%' }}>
          <Space size={8} wrap>
            {requirementTypeTag(requirement.requirement_type)}
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

export function WorkItemDetailPreview({
  workItem,
  requirementTitle,
  modelLabelMap,
  toolLabelMap,
  skillLabelMap,
}: {
  workItem: ProjectWorkItemRecord;
  requirementTitle: string;
  modelLabelMap: ExecutionOptionLabelMap;
  toolLabelMap: ExecutionOptionLabelMap;
  skillLabelMap: ExecutionOptionLabelMap;
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
          <Descriptions.Item label="类型">
            {workItem.is_planning_task ? <Tag color="processing">规划</Tag> : <Tag>执行</Tag>}
          </Descriptions.Item>
          <Descriptions.Item label="估算点数">{workItem.estimate_points ?? '-'}</Descriptions.Item>
          <Descriptions.Item label="计划完成">{formatDateTime(workItem.due_at)}</Descriptions.Item>
          <Descriptions.Item label="排序">{workItem.sort_order}</Descriptions.Item>
          <Descriptions.Item label="负责人">{workItem.assignee_user_id || '-'}</Descriptions.Item>
          <Descriptions.Item label="执行模型">
            {resolveExecutionOptionLabel(workItem.task_runner_default_model_config_id, modelLabelMap)}
          </Descriptions.Item>
          <Descriptions.Item label="工具集">
            {renderExecutionToolTags(workItem.task_runner_enabled_tool_ids, toolLabelMap)}
          </Descriptions.Item>
          <Descriptions.Item label="Skills">
            {renderExecutionToolTags(workItem.task_runner_skill_ids, skillLabelMap)}
          </Descriptions.Item>
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

export function projectStatusTag(status: 'active' | 'archived') {
  return (
    <Tag color={status === 'active' ? 'success' : 'default'}>
      {status === 'active' ? '进行中' : '已归档'}
    </Tag>
  );
}

export function resolveExecutionOptionLabel(
  value: string | null | undefined,
  labelMap: ExecutionOptionLabelMap,
) {
  const id = value?.trim();
  return id ? labelMap.get(id) || id : '-';
}

export function renderExecutionToolTags(
  values: string[] | null | undefined,
  labelMap: ExecutionOptionLabelMap,
) {
  const ids = values?.filter((value) => value.trim()) || [];
  if (ids.length === 0) {
    return <Typography.Text type="secondary">-</Typography.Text>;
  }
  return (
    <Space size={[4, 4]} wrap>
      {ids.map((id) => (
        <Tag key={id}>{resolveExecutionOptionLabel(id, labelMap)}</Tag>
      ))}
    </Space>
  );
}

export function renderGraphNode(node: DependencyGraphNode | undefined, fallback: string) {
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

export function graphStatusTag(node: DependencyGraphNode) {
  if (node.node_type === 'requirement') {
    return requirementStatusTag(node.status as RequirementRecord['status']);
  }
  if (node.node_type === 'work_item') {
    return workItemStatusTag(node.status as ProjectWorkItemRecord['status']);
  }
  return <Tag>{node.status || '-'}</Tag>;
}

export function requirementStatusTag(status: RequirementRecord['status']) {
  const item = requirementStatusDisplayOptions.find((option) => option.value === status);
  const color =
    status === 'done'
      ? 'success'
      : status === 'cancelled' || status === 'archived'
        ? 'default'
        : 'processing';
  return <Tag color={color}>{item?.label || status}</Tag>;
}

export function requirementTypeTag(type?: RequirementRecord['requirement_type']) {
  const normalized = type || 'requirement';
  const item = requirementTypeOptions.find((option) => option.value === normalized);
  const color = normalized === 'bug_fix' ? 'red' : normalized === 'change' ? 'orange' : 'geekblue';
  return <Tag color={color}>{item?.label || normalized}</Tag>;
}

export function workItemStatusTag(status: ProjectWorkItemRecord['status']) {
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

function formatDateTime(value?: string | null) {
  return value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-';
}
