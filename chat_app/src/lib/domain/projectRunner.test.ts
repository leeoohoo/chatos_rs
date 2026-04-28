import { describe, expect, it, vi } from 'vitest';

import type { Terminal } from '../../types';
import {
  RUNNER_START_COMMAND,
  RUNNER_SCRIPT_REL_PATH,
  buildProjectRunnerGenerationPrompt,
  buildProjectRunnerTarget,
  hasProjectRunnerScript,
  normalizeProjectRunnerMembers,
  normalizeProjectRunnerRootPath,
  readProjectRunnerDispatchTarget,
  resolveProjectRuntimeTerminal,
} from './projectRunner';

const buildTerminal = (overrides: Partial<Terminal>): Terminal => ({
  id: 'terminal_1',
  name: 'Runner',
  cwd: '/workspace',
  userId: null,
  projectId: 'project_1',
  status: 'running',
  busy: false,
  createdAt: new Date('2026-04-23T00:00:00.000Z'),
  updatedAt: new Date('2026-04-23T00:00:00.000Z'),
  lastActiveAt: new Date('2026-04-23T00:00:00.000Z'),
  ...overrides,
});

describe('domain/projectRunner', () => {
  it('dedupes and normalizes project runner members', () => {
    expect(normalizeProjectRunnerMembers([
      {
        contact_id: 'contact_1',
        agent_id: 'agent_1',
        agent_name_snapshot: 'Agent One',
      },
      {
        contactId: 'contact_1',
        agentId: 'agent_1',
        agentNameSnapshot: 'Agent One Newer',
      },
      {
        contact_id: 'broken',
      },
    ])).toEqual([
      {
        contactId: 'contact_1',
        agentId: 'agent_1',
        name: 'Agent One Newer',
      },
    ]);
  });

  it('detects the shared project runner script from filesystem listings', async () => {
    const listFsEntries = vi.fn(async (path?: string) => {
      if (path === '/workspace') {
        return {
          entries: [
            { name: '.chatos', path: '/workspace/.chatos', is_dir: true },
          ],
        };
      }
      if (path === '/workspace/.chatos') {
        return {
          entries: [
            { name: 'project_runner.sh', path: '/workspace/.chatos/project_runner.sh', is_dir: false },
          ],
        };
      }
      return { entries: [] };
    });

    await expect(hasProjectRunnerScript({ listFsEntries }, '/workspace/')).resolves.toBe(true);
    expect(listFsEntries).toHaveBeenNthCalledWith(1, '/workspace');
    expect(listFsEntries).toHaveBeenNthCalledWith(2, '/workspace/.chatos');
  });

  it('prefers busy runtime terminals and keeps the latest active fallback', () => {
    const older = buildTerminal({
      id: 'terminal_old',
      busy: false,
      lastActiveAt: new Date('2026-04-23T00:00:00.000Z'),
    });
    const newerBusy = buildTerminal({
      id: 'terminal_busy',
      busy: true,
      lastActiveAt: new Date('2026-04-23T00:00:10.000Z'),
    });

    expect(resolveProjectRuntimeTerminal([older, newerBusy], 'project_1')).toEqual({
      busyTerminal: newerBusy,
      activeTerminal: newerBusy,
    });
  });

  it('normalizes command targets and dispatch terminal metadata', () => {
    expect(normalizeProjectRunnerRootPath('/workspace///')).toBe('/workspace');
    expect(buildProjectRunnerTarget('/workspace')).toMatchObject({
      cwd: '/workspace',
      command: RUNNER_START_COMMAND,
      kind: 'script',
      source: 'script',
    });
    expect(readProjectRunnerDispatchTarget({
      terminal_id: 'terminal_1',
      terminal_name: 'Runner Terminal',
    })).toEqual({
      terminalId: 'terminal_1',
      terminalName: 'Runner Terminal',
    });
  });

  it('builds the shared runner generation prompt from project root', () => {
    const prompt = buildProjectRunnerGenerationPrompt('/workspace/demo');

    expect(prompt).toContain(`创建文件 ${RUNNER_SCRIPT_REL_PATH}`);
    expect(prompt).toContain('/workspace/demo/project_runner/logs/');
    expect(prompt).toContain('project_runner/runtime/ports.env');
    expect(prompt).toContain('stop 只能按本脚本维护的 pid 文件停止');
  });
});
