// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import {
  isTransientServiceAppError,
  sanitizeUserVisibleAppError,
} from './userVisibleError';

describe('sanitizeUserVisibleAppError', () => {
  it('hides internal service transport details', () => {
    expect(sanitizeUserVisibleAppError(
      'user_service 鉴权失败: error sending request for url (http://127.0.0.1:39190/api/auth/verify)',
    )).toBe('服务暂时不可用，请稍后重试。');
    expect(isTransientServiceAppError('connection refused')).toBe(true);
  });

  it('keeps concise product validation messages', () => {
    expect(sanitizeUserVisibleAppError('项目名称不能为空')).toBe('项目名称不能为空');
  });

  it('hides raw credentials and protocol payloads', () => {
    expect(sanitizeUserVisibleAppError(
      'status 500 {"access_token":"secret","internal_trace":"trace-1"}',
    )).toBe('服务暂时不可用，请稍后重试。');
  });
});
