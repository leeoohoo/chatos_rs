import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

import type { Session } from '../../../types';
import type {
  ChatStoreDraft,
  ChatStoreShape,
} from '../types';
import { createSessionCreateActions } from './sessions/createSession';

const createSession = (id: string, overrides: Partial<Session> = {}): Session => ({
  id,
  title: id,
  userId: 'user_1',
  user_id: 'user_1',
  projectId: null,
  project_id: null,
  createdAt: new Date('2026-01-01T00:00:00.000Z'),
  updatedAt: new Date('2026-01-01T00:00:00.000Z'),
  messageCount: 0,
  tokenUsage: 0,
  pinned: false,
  archived: false,
  status: 'active',
  tags: null,
  metadata: null,
  ...overrides,
});

describe('createSession', () => {
  const localStorageMock = {
    getItem: vi.fn(),
    setItem: vi.fn(),
    removeItem: vi.fn(),
    clear: vi.fn(),
  };

  beforeEach(() => {
    vi.stubGlobal('localStorage', localStorageMock);
    localStorageMock.getItem.mockReset();
    localStorageMock.setItem.mockReset();
    localStorageMock.removeItem.mockReset();
    localStorageMock.clear.mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('can create a contact session without activating the current session', async () => {
    const currentSession = createSession('session_current', {
      title: 'Current',
      projectId: 'project_1',
      project_id: 'project_1',
    });
    const state = {
      contacts: [],
      sessions: [currentSession],
      currentSessionId: currentSession.id,
      currentSession,
      currentProjectId: 'project_1',
      currentProject: {
        id: 'project_1',
        name: 'Project 1',
        rootPath: '/tmp/project-1',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      },
      activePanel: 'chat',
      messages: [{
        id: 'msg_current',
        sessionId: currentSession.id,
        role: 'assistant',
        content: 'still current',
        status: 'completed',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        metadata: {},
      }],
      error: null,
      selectedModelId: 'model_current',
      selectedAgentId: null,
      sessionAiSelectionBySession: {},
      sessionStreamingMessageDrafts: {},
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;

    const set = (updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    };

    const createdRemote = {
      id: 'session_new',
      title: 'Teammate',
      user_id: 'user_1',
      project_id: 'project_1',
      created_at: '2026-01-01T00:00:00.000Z',
      updated_at: '2026-01-01T00:00:00.000Z',
      metadata: {
        ui_chat_selection: {
          selected_model_id: 'model_new',
          selected_agent_id: 'agent_1',
        },
        chat_runtime: {
          selected_model_id: 'model_new',
          contact_id: 'contact_1',
          contact_agent_id: 'agent_1',
          project_id: 'project_1',
        },
      },
    };

    const actions = createSessionCreateActions({
      set,
      get: (() => state) as never,
      client: {
        createSession: vi.fn().mockResolvedValue(createdRemote),
        getSessions: vi.fn().mockResolvedValue([]),
      } as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: 'project_1' }),
    });

    const createdId = await actions.createSession({
      title: 'Teammate',
      contactId: 'contact_1',
      contactAgentId: 'agent_1',
      projectId: 'project_1',
      selectedModelId: 'model_new',
    }, {
      activateSession: false,
      keepActivePanel: true,
    });

    expect(createdId).toBe('session_new');
    expect(state.currentSessionId).toBe(currentSession.id);
    expect(state.currentSession?.id).toBe(currentSession.id);
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0]?.sessionId).toBe(currentSession.id);
    expect(state.selectedModelId).toBe('model_current');
    expect(state.sessions.map((session) => session.id)).toContain('session_new');
    expect(state.sessionAiSelectionBySession.session_new).toEqual({
      selectedModelId: 'model_new',
      selectedAgentId: 'agent_1',
    });
    expect(localStorageMock.setItem).not.toHaveBeenCalled();
  });
});
