// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { ApiRequestError } from '../../../lib/api/client/shared';
import { extractProjectRunnerValidationMessage } from './projectRunnerCommandErrors';

describe('projectRunnerCommandErrors', () => {
  it('formats validation issues from API request errors', () => {
    const error = new ApiRequestError('bad request', {
      status: 400,
      payload: {
        validation_issues: [
          { kind: 'toolchain', message: '缺少 JDK', target_label: 'Java App', hint: '请选择 JDK 21' },
        ],
      },
    });

    expect(extractProjectRunnerValidationMessage(error, 'fallback')).toContain('缺少 JDK');
  });

  it('falls back when the error is not a validation payload', () => {
    expect(extractProjectRunnerValidationMessage(new Error('boom'), 'fallback')).toBe('fallback');
  });
});
