// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ProjectWorkItemRecord, RequirementStatus, RequirementType } from '../../types';

export const requirementStatusDisplayOptions = [
  { value: 'draft', label: '草稿' },
  { value: 'reviewing', label: '评审中' },
  { value: 'approved', label: '已确认' },
  { value: 'in_progress', label: '实现中' },
  { value: 'blocked', label: '阻塞' },
  { value: 'done', label: '已完成' },
  { value: 'cancelled', label: '已取消' },
  { value: 'archived', label: '已归档' },
] satisfies Array<{ value: RequirementStatus; label: string }>;

export const requirementStatusOptions = requirementStatusDisplayOptions.filter(
  (option) => option.value !== 'archived',
);

export const requirementTypeOptions = [
  { value: 'requirement', label: '需求' },
  { value: 'change', label: '变更' },
  { value: 'bug_fix', label: 'Bug 修复' },
] satisfies Array<{ value: RequirementType; label: string }>;

export const workItemStatusDisplayOptions = [
  { value: 'todo', label: '待处理' },
  { value: 'ready', label: '已就绪' },
  { value: 'in_progress', label: '进行中' },
  { value: 'blocked', label: '阻塞' },
  { value: 'done', label: '完成' },
  { value: 'cancelled', label: '取消' },
  { value: 'archived', label: '已归档' },
] satisfies Array<{ value: ProjectWorkItemRecord['status']; label: string }>;

export const workItemStatusOptions = workItemStatusDisplayOptions.filter(
  (option) => option.value !== 'archived',
);

export const requirementDocumentTypeOptions = [
  { value: 'technical_overview', label: '技术概要' },
  { value: 'implementation_plan', label: '实现方案' },
  { value: 'ui_svg_preview', label: '前端 SVG 预览图' },
  { value: 'architecture_diagram', label: '架构图' },
  { value: 'flowchart', label: '流程图' },
  { value: 'sequence_diagram', label: '时序图' },
  { value: 'api_design', label: '接口设计' },
  { value: 'data_model', label: '数据模型' },
  { value: 'risk_notes', label: '风险说明' },
  { value: 'other', label: '其他' },
] satisfies Array<{ value: string; label: string }>;

export function requirementDocumentTypeLabel(value?: string | null): string {
  return (
    requirementDocumentTypeOptions.find((option) => option.value === value)?.label ||
    value ||
    '技术文档'
  );
}
