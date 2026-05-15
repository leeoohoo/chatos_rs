import { describe, expect, it, vi } from 'vitest';

import type { Terminal } from '../../types';
import {
  getProjectRunnerContactRowsSnapshot,
  RUNNER_START_COMMAND,
  RUNNER_SCRIPT_REL_PATH,
  buildProjectRunnerGenerationPrompt,
  buildProjectRunnerTarget,
  hasProjectRunnerScript,
  loadProjectRunnerContactRows,
  loadProjectRunnerMembers,
  markProjectRunnerScriptStateStale,
  normalizeProjectRunnerMembers,
  normalizeProjectRunnerRootPath,
  readProjectRunnerDispatchTarget,
  resolveProjectRuntimeTerminal,
  removeProjectRunnerContactRow,
  upsertProjectRunnerContactRow,
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

  it('dedupes concurrent project runner script checks per client and root path', async () => {
    let resolveBlocker!: () => void;
    const blocker = new Promise<void>((resolve) => {
      resolveBlocker = resolve;
    });
    const listFsEntries = vi.fn(async (path?: string) => {
      if (path === '/workspace') {
        await blocker;
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

    const client = { listFsEntries };
    const pendingA = hasProjectRunnerScript(client, '/workspace/');
    const pendingB = hasProjectRunnerScript(client, '/workspace/');
    resolveBlocker();

    await expect(Promise.all([pendingA, pendingB])).resolves.toEqual([true, true]);
    expect(listFsEntries).toHaveBeenCalledTimes(2);
  });

  it('reuses cached project runner script checks until explicitly invalidated', async () => {
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

    const client = { listFsEntries };

    await expect(hasProjectRunnerScript(client, '/workspace/')).resolves.toBe(true);
    await expect(hasProjectRunnerScript(client, '/workspace/')).resolves.toBe(true);
    expect(listFsEntries).toHaveBeenCalledTimes(2);

    markProjectRunnerScriptStateStale(client, '/workspace/');

    await expect(hasProjectRunnerScript(client, '/workspace/')).resolves.toBe(true);
    expect(listFsEntries).toHaveBeenCalledTimes(4);
  });

  it('caches missing project root failures until explicitly invalidated', async () => {
    const listFsEntries = vi.fn(async (path?: string) => {
      if (path === '/missing-workspace') {
        throw new Error('路径不存在');
      }
      return { entries: [] };
    });

    const client = { listFsEntries };

    await expect(hasProjectRunnerScript(client, '/missing-workspace/')).rejects.toThrow('路径不存在');
    await expect(hasProjectRunnerScript(client, '/missing-workspace/')).rejects.toThrow('路径不存在');
    expect(listFsEntries).toHaveBeenCalledTimes(1);

    markProjectRunnerScriptStateStale(client, '/missing-workspace/');

    await expect(hasProjectRunnerScript(client, '/missing-workspace/')).rejects.toThrow('路径不存在');
    expect(listFsEntries).toHaveBeenCalledTimes(2);
  });

  it('dedupes concurrent project runner member loads per client and project', async () => {
    let resolveBlocker!: () => void;
    const blocker = new Promise<void>((resolve) => {
      resolveBlocker = resolve;
    });
    const listProjectContacts = vi.fn(async () => {
      await blocker;
      return [
        {
          contact_id: 'contact_1',
          agent_id: 'agent_1',
          agent_name_snapshot: 'Agent One',
        },
      ];
    });

    const client = { listProjectContacts };
    const rowsA = loadProjectRunnerContactRows(client, 'project_1');
    const rowsB = loadProjectRunnerContactRows(client, 'project_1');
    const membersA = loadProjectRunnerMembers(client, 'project_1');
    resolveBlocker();

    await expect(Promise.all([rowsA, rowsB])).resolves.toEqual([
      [
        {
          contact_id: 'contact_1',
          agent_id: 'agent_1',
          agent_name_snapshot: 'Agent One',
        },
      ],
      [
        {
          contact_id: 'contact_1',
          agent_id: 'agent_1',
          agent_name_snapshot: 'Agent One',
        },
      ],
    ]);
    await expect(membersA).resolves.toEqual([
      {
        contactId: 'contact_1',
        agentId: 'agent_1',
        name: 'Agent One',
      },
    ]);
    expect(listProjectContacts).toHaveBeenCalledTimes(1);
  });

  it('supports local project runner member cache patching after snapshot load', async () => {
    const listProjectContacts = vi.fn(async () => [
      {
        contact_id: 'contact_1',
        agent_id: 'agent_1',
        agent_name_snapshot: 'Agent One',
      },
    ]);

    const client = { listProjectContacts };
    await expect(loadProjectRunnerContactRows(client, 'project_1')).resolves.toEqual([
      {
        contact_id: 'contact_1',
        agent_id: 'agent_1',
        agent_name_snapshot: 'Agent One',
      },
    ]);

    expect(upsertProjectRunnerContactRow(client, 'project_1', {
      contact_id: 'contact_2',
      agent_id: 'agent_2',
      agent_name_snapshot: 'Agent Two',
    })).toEqual([
      {
        contact_id: 'contact_2',
        agent_id: 'agent_2',
        agent_name_snapshot: 'Agent Two',
      },
      {
        contact_id: 'contact_1',
        agent_id: 'agent_1',
        agent_name_snapshot: 'Agent One',
      },
    ]);

    expect(getProjectRunnerContactRowsSnapshot(client, 'project_1')).toEqual([
      {
        contact_id: 'contact_2',
        agent_id: 'agent_2',
        agent_name_snapshot: 'Agent Two',
      },
      {
        contact_id: 'contact_1',
        agent_id: 'agent_1',
        agent_name_snapshot: 'Agent One',
      },
    ]);

    expect(removeProjectRunnerContactRow(client, 'project_1', 'contact_1')).toEqual([
      {
        contact_id: 'contact_2',
        agent_id: 'agent_2',
        agent_name_snapshot: 'Agent Two',
      },
    ]);

    expect(getProjectRunnerContactRowsSnapshot(client, 'project_1')).toEqual([
      {
        contact_id: 'contact_2',
        agent_id: 'agent_2',
        agent_name_snapshot: 'Agent Two',
      },
    ]);
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
