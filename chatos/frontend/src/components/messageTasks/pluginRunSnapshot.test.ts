// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';
import { pluginRunSnapshotSummary } from './pluginRunSnapshot';

describe('pluginRunSnapshotSummary', () => {
  it('projects selection and immutable Release metadata without retaining Command arguments', () => {
    const secret = 'do-not-render-command-arguments';
    const summary = pluginRunSnapshotSummary({
      plugin_config: {
        selected_plugins: [{
          plugin_id: 'plugin-review',
          selected_skill_ids: ['review-skill'],
          selected_command_ids: ['review'],
          selected_agent_ids: ['reviewer'],
        }],
        command_invocations: [{
          plugin_id: 'plugin-review',
          command_id: 'review',
          arguments: secret,
          arguments_present: true,
          arguments_sha256: 'a'.repeat(64),
        }],
      },
    });

    expect(summary).toMatchObject({
      plugins: [{
        pluginId: 'plugin-review',
        selectedSkillIds: ['review-skill'],
        selectedCommandIds: ['review'],
        selectedAgentIds: ['reviewer'],
      }],
      commands: [{
        pluginId: 'plugin-review',
        commandId: 'review',
        argumentsPresent: true,
        argumentsSha256: 'a'.repeat(64),
      }],
    });
    expect(JSON.stringify(summary)).not.toContain(secret);
  });

  it('rejects incomplete or unselected Plugin audit entries', () => {
    expect(pluginRunSnapshotSummary({ plugin_config: {} })).toBeNull();
    expect(pluginRunSnapshotSummary({
      plugin_config: {
        selected_plugins: [{ plugin_id: 'plugin-a' }],
        command_invocations: [{
          plugin_id: 'plugin-b',
          command_id: 'hidden',
          arguments_present: true,
          arguments_sha256: 'b'.repeat(64),
        }],
      },
    })?.commands).toEqual([]);
  });
});
