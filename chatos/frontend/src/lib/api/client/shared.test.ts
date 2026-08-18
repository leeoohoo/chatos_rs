// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { getErrorMessageFromPayload } from './shared';

describe('getErrorMessageFromPayload', () => {
  it('includes nested JSON-RPC error details', () => {
    expect(getErrorMessageFromPayload({
      error: 'Local Connector MCP 调用失败',
      detail: {
        code: -32000,
        message: 'local execution scope is missing project identity',
      },
    }, 'request failed')).toBe(
      'Local Connector MCP 调用失败: local execution scope is missing project identity',
    );
  });
});
