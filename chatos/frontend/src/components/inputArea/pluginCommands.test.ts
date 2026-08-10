// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';
import {
  filterPluginCommandOptions,
  parseLeadingPluginCommand,
  pluginCommandOptions,
  replaceLeadingPluginCommand,
  utf8ByteLength,
} from './pluginCommands';

const plugins: TaskRunnerSelectablePluginResponse[] = [{
  id: 'plugin-review',
  plugin_key: 'reviewer',
  display_name: 'Code Reviewer',
  description: 'Reviews source code',
  version: '1.0.0',
  release_id: 'release-1',
  artifact_sha256: 'a'.repeat(64),
  device_id: 'device-1',
  execution_type: 'local',
  requires_device: true,
  component_hosts: { review: 'local' },
  component_keys: ['review'],
  components: [],
  commands: [{
    command_id: 'review',
    display_name: 'Review changes',
    description: 'Review a path',
    argument_hint: '[path]',
    requires_confirmation: true,
  }],
}];

describe('Plugin Command composer helpers', () => {
  it('parses only a leading slash command and preserves its argument draft', () => {
    expect(parseLeadingPluginCommand('/review src/lib.rs')).toEqual({
      query: 'review',
      arguments: 'src/lib.rs',
    });
    expect(parseLeadingPluginCommand('please /review src/lib.rs')).toBeNull();
    expect(parseLeadingPluginCommand('//review')).toBeNull();
  });

  it('replaces a partial slash token without dropping arguments', () => {
    expect(replaceLeadingPluginCommand('/rev src/lib.rs', 'review', true))
      .toBe('/review src/lib.rs');
    expect(replaceLeadingPluginCommand('/rev', 'review', true)).toBe('/review ');
  });

  it('discovers commands by command and Plugin metadata', () => {
    const options = pluginCommandOptions(plugins);
    expect(filterPluginCommandOptions(options, 'review')).toHaveLength(1);
    expect(filterPluginCommandOptions(options, 'code reviewer')).toHaveLength(1);
    expect(filterPluginCommandOptions(options, 'missing')).toHaveLength(0);
  });

  it('counts UTF-8 bytes instead of JavaScript code units', () => {
    expect(utf8ByteLength('插件')).toBe(6);
  });
});
