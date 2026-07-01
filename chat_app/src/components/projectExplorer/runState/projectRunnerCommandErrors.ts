// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TranslateFn } from '../../../i18n/I18nProvider';
import { ApiRequestError } from '../../../lib/api/client/shared';
import { normalizeProjectRunValidationIssue } from '../../../lib/domain/projectExplorer';
import { formatProjectRunValidationIssues } from './projectRunnerValidationIssues';

export const extractProjectRunnerValidationMessage = (
  error: unknown,
  fallback: string,
  t?: TranslateFn,
): string => {
  if (!(error instanceof ApiRequestError) || !error.payload || typeof error.payload !== 'object') {
    return fallback;
  }
  const payload = error.payload as Record<string, unknown>;
  const rawIssues = payload.validation_issues;
  if (!Array.isArray(rawIssues)) {
    return fallback;
  }
  const issues = rawIssues
    .map(normalizeProjectRunValidationIssue)
    .filter((item) => item.kind && item.message);
  return formatProjectRunValidationIssues(issues, fallback, t);
};
