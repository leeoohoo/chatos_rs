// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { sanitizeUserVisibleAiError } from './errorParsing';

describe('sanitizeUserVisibleAiError', () => {
  it('localizes cancellation', () => {
    expect(sanitizeUserVisibleAiError('Chat turn cancelled')).toBe('已停止生成');
  });

  it('keeps retry count but removes parser internals', () => {
    expect(sanitizeUserVisibleAiError(
      'AI 请求失败：响应解析异常，已重试 5 次，最后错误：stream response parse failed: no valid SSE events parsed from provider',
    )).toBe('模型服务响应异常，已自动重试 5 次，请稍后重试或切换模型。');
  });

  it('redacts provider credentials and traces', () => {
    const result = sanitizeUserVisibleAiError(
      'status 500 Internal Server Error: {"api_key":"secret","internal_trace":"trace-1"}',
    );

    expect(result).toBe('模型服务调用失败，请稍后重试或检查模型配置。');
    expect(result).not.toContain('secret');
    expect(result).not.toContain('internal_trace');
  });
});
