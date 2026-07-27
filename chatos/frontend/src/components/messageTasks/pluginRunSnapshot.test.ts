// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';
import { pluginRunSnapshotSummary } from './pluginRunSnapshot';

describe('pluginRunSnapshotSummary', () => {
  it('projects selection and immutable Release metadata without retaining Command arguments', () => {
    const secret = 'do-not-render-command-arguments';
    const summary = pluginRunSnapshotSummary({
      plugin_config: {
        device_id: 'device-1',
        workspace_id: 'workspace-1',
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
      plugin_snapshots: [{
        plugin_id: 'plugin-review',
        release_id: 'release-1',
        version: '1.2.3',
        component_snapshots: [
          { component_key: 'review', runtime: { arguments: secret } },
          { component_key: 'reviewer' },
        ],
      }],
    });

    expect(summary).toMatchObject({
      deviceId: 'device-1',
      workspaceId: 'workspace-1',
      plugins: [{
        pluginId: 'plugin-review',
        releaseId: 'release-1',
        version: '1.2.3',
        selectedSkillIds: ['review-skill'],
        selectedCommandIds: ['review'],
        selectedAgentIds: ['reviewer'],
        componentKeys: ['review', 'reviewer'],
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
        device_id: 'device-1',
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
