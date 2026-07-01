// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TranslateFn } from '../../../i18n/I18nProvider';
import type { ProjectRunValidationIssue } from '../../../types';

const formatValidationIssueLine = (
  issue: ProjectRunValidationIssue,
  t?: TranslateFn,
): string => {
  const base = issue.targetLabel ? `[${issue.targetLabel}] ${issue.message}` : issue.message;
  if (issue.hint) {
    return t ? t('runSettings.validationIssueHint', { base, hint: issue.hint }) : `${base}; suggestion: ${issue.hint}`;
  }
  return base;
};

export const formatProjectRunValidationIssues = (
  issues: ProjectRunValidationIssue[],
  fallback: string,
  t?: TranslateFn,
): string => {
  const normalized = issues
    .map((issue) => formatValidationIssueLine(issue, t))
    .filter(Boolean);
  if (normalized.length === 0) {
    return fallback;
  }
  return t
    ? t('runSettings.validationFailed', { issues: normalized.join('; ') })
    : `Preflight check failed: ${normalized.join('; ')}`;
};
