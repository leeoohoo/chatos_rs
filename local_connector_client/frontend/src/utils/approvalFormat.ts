// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ApprovalMode } from '../api';

export function approvalModeLabel(mode: ApprovalMode) {
  const labels: Record<ApprovalMode, string> = {
    request_approval: '请求审批',
    auto_approval: '自动审批',
    full_control: '从不询问',
  };
  return labels[mode] || mode;
}

export function approvalModeDescription(mode: ApprovalMode) {
  const labels: Record<ApprovalMode, string> = {
    request_approval: '每条命令等待用户通过',
    auto_approval: '由本机 AI 审批命令',
    full_control: '命令直接执行，不再弹出审批',
  };
  return labels[mode] || mode;
}

export function approvalDecisionLabel(decision: string) {
  const labels: Record<string, string> = {
    approved: '通过',
    denied: '拒绝',
  };
  return labels[decision] || decision;
}

export function approvalDecisionClass(decision: string) {
  return decision === 'approved' ? 'status ok' : decision === 'denied' ? 'status bad' : 'status warn';
}

export function riskLabel(risk: string) {
  const labels: Record<string, string> = {
    low: '低风险',
    medium: '中风险',
    high: '高风险',
  };
  return labels[risk] || risk;
}

export function riskStatusClass(risk: string) {
  if (risk === 'low') {
    return 'status ok';
  }
  if (risk === 'high') {
    return 'status bad';
  }
  return 'status warn';
}

export function decisionSourceLabel(source: string) {
  const labels: Record<string, string> = {
    whitelist: '白名单',
    user: '用户',
    ai: 'AI',
    full_control: '从不询问',
    static_rule: '静态规则',
  };
  return labels[source] || source;
}

export function projectLabel(projectKey: {
  project_root_relative_path: string;
  project_anchor_relative_path?: string | null;
}) {
  return projectKey.project_anchor_relative_path || projectKey.project_root_relative_path || '.';
}
