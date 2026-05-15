import { describe, expect, it, vi } from 'vitest';

import type {
  ChatStoreDraft,
  ChatStoreShape,
} from '../types';
import { createProjectActions } from './projects';

describe('loadProjects', () => {
  it('clears stale current project state when the selected project no longer exists', async () => {
    const state = {
      projects: [],
      currentProjectId: 'project_missing',
      currentProject: {
        id: 'project_missing',
        name: 'Missing Project',
        rootPath: '/tmp/missing-project',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      },
      activePanel: 'project',
      error: null,
    } as unknown as ChatStoreShape;

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    const actions = createProjectActions({
      set,
      get,
      client: {
        listProjects: vi.fn().mockResolvedValue([
          {
            id: 'project_kept',
            name: 'Kept Project',
            root_path: '/tmp/kept-project',
            created_at: '2026-01-01T00:00:00.000Z',
            updated_at: '2026-01-01T00:00:00.000Z',
          },
        ]),
      } as never,
      getUserIdParam: () => 'user_1',
    });

    await actions.loadProjects({ force: true });

    expect(state.currentProjectId).toBeNull();
    expect(state.currentProject).toBeNull();
    expect(state.activePanel).toBe('chat');
    expect(state.projects.map((project) => project.id)).toEqual(['project_kept']);
  });
});
