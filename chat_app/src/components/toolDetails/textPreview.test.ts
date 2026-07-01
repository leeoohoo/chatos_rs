// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { formatToolDetailText, stringifyJsonPreview } from './textPreview';

describe('tool detail text preview', () => {
  it('clips long text blocks before rendering', () => {
    const preview = formatToolDetailText('x'.repeat(20_000), 128);

    expect(preview.meta).toBe('truncated');
    expect(preview.content.length).toBeLessThan(200);
    expect(preview.content).toContain('truncated');
  });

  it('builds bounded previews for large nested json values', () => {
    const payload = {
      rows: Array.from({ length: 120 }, (_, index) => ({
        index,
        content: 'y'.repeat(3_000),
      })),
    };

    const preview = stringifyJsonPreview(payload, 4_000);

    expect(preview.meta).toBe('truncated');
    expect(preview.content.length).toBeLessThanOrEqual(4_050);
    expect(preview.content).toContain('truncated');
  });
});
