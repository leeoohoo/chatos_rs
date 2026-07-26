// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { normalizePluginCommandAuditEntries } from './pluginCommandAudit';

describe('normalizePluginCommandAuditEntries', () => {
  it('keeps immutable command identity and argument hash without raw arguments', () => {
    const entries = normalizePluginCommandAuditEntries([
      {
        plugin_id: ' plugin-a ',
        command_id: ' review ',
        arguments_present: true,
        arguments_sha256: 'A'.repeat(64),
        arguments: 'must not be exposed',
      },
      {
        plugin_id: 'plugin-a',
        command_id: 'review',
        arguments_present: true,
        arguments_sha256: 'b'.repeat(64),
      },
    ]);

    expect(entries).toEqual([{
      plugin_id: 'plugin-a',
      command_id: 'review',
      arguments_present: true,
      arguments_sha256: 'a'.repeat(64),
    }]);
    expect(entries[0]).not.toHaveProperty('arguments');
  });

  it('drops malformed identities and invalid hashes', () => {
    expect(normalizePluginCommandAuditEntries([
      { plugin_id: '', command_id: 'review' },
      { plugin_id: 'plugin-a', command_id: '' },
      {
        plugin_id: 'plugin-b',
        command_id: 'run',
        arguments_present: true,
        arguments_sha256: 'not-a-sha256',
      },
    ])).toEqual([{
      plugin_id: 'plugin-b',
      command_id: 'run',
      arguments_present: true,
      arguments_sha256: null,
    }]);
  });
});
