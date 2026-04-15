import { describe, expect, it } from 'vitest';

import { buildMcpToolsetPresets } from './useMcpSelection';

describe('buildMcpToolsetPresets', () => {
  it('builds coding preset with available coding MCP ids', () => {
    const available = [
      'builtin_code_maintainer_read',
      'builtin_code_maintainer_write',
      'builtin_terminal_controller',
      'builtin_task_manager',
      'builtin_notepad',
    ];
    const selectable = [...available];
    const presets = buildMcpToolsetPresets(selectable, available);
    const coding = presets.find((item) => item.id === 'coding');

    expect(coding).toBeDefined();
    expect(coding?.disabled).toBe(false);
    expect(coding?.targetIds).toEqual([
      'builtin_code_maintainer_read',
      'builtin_code_maintainer_write',
      'builtin_terminal_controller',
      'builtin_task_manager',
      'builtin_notepad',
    ]);
  });

  it('filters remote preset by selectable ids when remote context is unavailable', () => {
    const available = [
      'builtin_remote_connection_controller',
      'builtin_terminal_controller',
      'builtin_task_manager',
      'builtin_code_maintainer_read',
    ];
    const selectable = [
      'builtin_terminal_controller',
      'builtin_task_manager',
      'builtin_code_maintainer_read',
    ];
    const presets = buildMcpToolsetPresets(selectable, available);
    const remoteOps = presets.find((item) => item.id === 'remote_ops');

    expect(remoteOps).toBeDefined();
    expect(remoteOps?.disabled).toBe(false);
    expect(remoteOps?.targetIds).toEqual([
      'builtin_terminal_controller',
      'builtin_task_manager',
      'builtin_code_maintainer_read',
    ]);
  });

  it('marks preset as disabled when no target ids are available', () => {
    const available = ['builtin_terminal_controller'];
    const selectable = ['builtin_terminal_controller'];
    const presets = buildMcpToolsetPresets(selectable, available);
    const webResearch = presets.find((item) => item.id === 'web_research');

    expect(webResearch).toBeDefined();
    expect(webResearch?.disabled).toBe(true);
    expect(webResearch?.targetIds).toEqual([]);
  });
});
