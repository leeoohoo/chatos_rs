// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import {
  getProjectRunnerContactRowsSnapshot,
  loadProjectRunnerContactRows,
  loadProjectRunnerMembers,
  normalizeProjectRunnerMembers,
  normalizeProjectRunnerRootPath,
  readProjectRunnerBoundTerminal,
  readProjectRunnerDispatchTarget,
  removeProjectRunnerContactRow,
  subscribeProjectRunnerContactRows,
  syncProjectRunnerContactRows,
  upsertProjectRunnerContactRow,
} from './projectRunner';

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

  it('syncs project runner member cache from an authoritative row list', async () => {
    const listProjectContacts = vi.fn(async () => [
      {
        contact_id: 'contact_1',
        agent_id: 'agent_1',
        agent_name_snapshot: 'Agent One',
      },
    ]);

    const client = { listProjectContacts };
    await loadProjectRunnerContactRows(client, 'project_1');

    expect(syncProjectRunnerContactRows(client, 'project_1', [
      {
        contact_id: 'contact_2',
        agent_id: 'agent_2',
        agent_name_snapshot: 'Agent Two',
      },
    ])).toEqual([
      {
        contact_id: 'contact_2',
        agent_id: 'agent_2',
        agent_name_snapshot: 'Agent Two',
      },
    ]);

    await expect(loadProjectRunnerContactRows(client, 'project_1')).resolves.toEqual([
      {
        contact_id: 'contact_2',
        agent_id: 'agent_2',
        agent_name_snapshot: 'Agent Two',
      },
    ]);
    expect(listProjectContacts).toHaveBeenCalledTimes(1);
  });

  it('notifies mounted consumers when another project contact surface updates the cache', async () => {
    const listProjectContacts = vi.fn(async () => []);
    const client = { listProjectContacts };
    const listener = vi.fn();
    const unsubscribe = subscribeProjectRunnerContactRows(client, 'project_1', listener);

    syncProjectRunnerContactRows(client, 'project_1', [
      {
        contact_id: 'contact_1',
        agent_id: 'agent_1',
        agent_name_snapshot: 'Agent One',
      },
    ]);

    expect(listener).toHaveBeenCalledWith([
      {
        contact_id: 'contact_1',
        agent_id: 'agent_1',
        agent_name_snapshot: 'Agent One',
      },
    ]);

    unsubscribe();
    syncProjectRunnerContactRows(client, 'project_1', []);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('normalizes runtime root paths, bound terminal state and dispatch terminal metadata', () => {
    expect(normalizeProjectRunnerRootPath('/workspace///')).toBe('/workspace');
    expect(readProjectRunnerBoundTerminal({
      projectId: 'project_1',
      running: true,
      busy: false,
      status: 'running',
      terminalId: 'terminal_1',
      terminalName: 'Runner Terminal',
      cwd: '/workspace',
      terminal: null,
    })).toEqual({
      terminalId: 'terminal_1',
      terminalName: 'Runner Terminal',
      cwd: '/workspace',
      running: true,
      busy: false,
      status: 'running',
    });
    expect(readProjectRunnerDispatchTarget({
      terminal_id: 'terminal_1',
      terminal_name: 'Runner Terminal',
    })).toEqual({
      terminalId: 'terminal_1',
      terminalName: 'Runner Terminal',
    });
  });
});
