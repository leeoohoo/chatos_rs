import type { ProjectRunValidationIssue } from '../../../types';

const formatValidationIssueLine = (issue: ProjectRunValidationIssue): string => {
  const base = issue.targetLabel ? `[${issue.targetLabel}] ${issue.message}` : issue.message;
  if (issue.hint) {
    return `${base}；建议：${issue.hint}`;
  }
  return base;
};

export const formatProjectRunValidationIssues = (
  issues: ProjectRunValidationIssue[],
  fallback: string,
): string => {
  const normalized = issues
    .map(formatValidationIssueLine)
    .filter(Boolean);
  if (normalized.length === 0) {
    return fallback;
  }
  return `启动前检查未通过：${normalized.join('；')}`;
};
