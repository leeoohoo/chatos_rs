// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';
import {
  filterPluginMentionOptions,
  findPluginMentionAtCursor,
  replacePluginMention,
} from './pluginMentions';

const plugin = (
  overrides: Partial<TaskRunnerSelectablePluginResponse> = {},
): TaskRunnerSelectablePluginResponse => ({
  id: 'plugin-browser',
  plugin_key: 'browser',
  display_name: 'Browser',
  description: 'Control a managed browser session',
  version: '1.8.0',
  release_id: 'release-browser',
  artifact_sha256: 'a'.repeat(64),
  device_id: 'device-1',
  requires_device: true,
  component_keys: ['browser.tools/v1'],
  components: [],
  commands: [],
  ...overrides,
});

describe('Plugin mentions', () => {
  it('finds a bounded mention at the cursor without treating email text as a Plugin mention', () => {
    expect(findPluginMentionAtCursor('Use @brow now', 9)).toEqual({
      start: 4,
      end: 9,
      query: 'brow',
    });
    expect(findPluginMentionAtCursor('mail@example.com', 8)).toBeNull();
    expect(findPluginMentionAtCursor('@'.repeat(130), 130)).toBeNull();
  });

  it('replaces the complete token and returns the next caret position', () => {
    const draft = findPluginMentionAtCursor('Use @brower now', 9);
    expect(draft).not.toBeNull();
    expect(replacePluginMention('Use @brower now', draft!, 'browser')).toEqual({
      message: 'Use @browser now',
      cursor: 12,
    });
  });

  it('filters by canonical key, display name, description, version, and component', () => {
    const plugins = [
      plugin(),
      plugin({
        id: 'plugin-documents',
        plugin_key: 'documents',
        display_name: 'Documents',
        description: 'DOCX editing',
        component_keys: ['documents.skill/v1'],
      }),
    ];
    expect(filterPluginMentionOptions(plugins, 'docx').map((item) => item.id)).toEqual([
      'plugin-documents',
    ]);
    expect(filterPluginMentionOptions(plugins, 'browser.tools').map((item) => item.id)).toEqual([
      'plugin-browser',
    ]);
  });
});
