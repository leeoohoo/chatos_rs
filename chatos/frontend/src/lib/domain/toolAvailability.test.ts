// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import {
  extractUnavailableToolsFromPayload,
  mergeUnavailableToolEntries,
  normalizeUnavailableToolEntry,
  unavailableToolEntryKey,
} from './toolAvailability';

describe('domain/toolAvailability', () => {
  it('extracts unavailable tool payloads from envelope and singleton forms', () => {
    expect(extractUnavailableToolsFromPayload({
      unavailable_tools: [{ server_name: 'alpha', tool_name: 'search' }],
    })).toEqual([{ server_name: 'alpha', tool_name: 'search' }]);

    expect(extractUnavailableToolsFromPayload({
      serverName: 'beta',
      toolName: 'read',
    })).toEqual([{ serverName: 'beta', toolName: 'read' }]);
  });

  it('normalizes fallback fields and dedupes merged entries', () => {
    const merged = mergeUnavailableToolEntries([
      {
        id: 'entry_1',
        serverName: 'alpha',
        toolName: 'search',
        reason: 'offline',
      },
    ], {
      unavailable_tools: [
        { server_name: 'alpha', tool_name: 'search', reason: 'offline' },
        { serverName: 'beta', toolName: 'read' },
      ],
    });

    expect(merged.addedCount).toBe(1);
    expect(merged.items).toHaveLength(2);
    expect(merged.items[1]).toMatchObject({
      serverName: 'beta',
      toolName: 'read',
      reason: '工具当前不可用',
    });
    expect(unavailableToolEntryKey(merged.items[1]!)).toBe('beta::read::工具当前不可用');
  });

  it('normalizes unavailable entries with stable shape', () => {
    expect(normalizeUnavailableToolEntry({}, 0)).toMatchObject({
      id: expect.stringContaining('unavailable_tool_'),
      serverName: 'unknown_server',
      toolName: 'unknown_tool',
      reason: '工具当前不可用',
      createdAt: expect.any(String),
    });
  });
});
