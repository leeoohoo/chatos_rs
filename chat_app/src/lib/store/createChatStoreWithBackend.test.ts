import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createChatStoreWithBackend } from './createChatStoreWithBackend';
import {
  LEGACY_CHAT_STORE_PERSIST_KEY,
  resolveChatStorePersistKey,
} from './persistence';

type PersistCapableStore = ReturnType<typeof createChatStoreWithBackend> & {
  persist?: {
    rehydrate?: () => Promise<void> | void;
  };
};

const buildMemoryStorage = () => {
  const values = new Map<string, string>();
  return {
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      values.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      values.delete(key);
    }),
    clear: vi.fn(() => {
      values.clear();
    }),
  };
};

describe('createChatStoreWithBackend persistence', () => {
  let localStorageMock: ReturnType<typeof buildMemoryStorage>;

  beforeEach(() => {
    localStorageMock = buildMemoryStorage();
    vi.stubGlobal('localStorage', localStorageMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('does not persist streaming runtime state into localStorage', () => {
    const persistKey = resolveChatStorePersistKey('user_1');
    const store = createChatStoreWithBackend({} as never, {
      userId: 'user_1',
    }) as PersistCapableStore;

    store.setState({
      theme: 'dark',
      sidebarOpen: false,
      sessionAiSelectionBySession: {
        session_1: {
          selectedModelId: 'model_1',
          selectedAgentId: 'agent_1',
        },
      },
      sessionChatState: {
        session_1: {
          isLoading: true,
          isStreaming: true,
          isStopping: false,
          streamingPhase: 'thinking',
          streamingMessageId: 'assistant_temp_1',
          activeTurnId: 'turn_1',
          streamingPreviewText: 'partial answer',
          streamingTransport: 'realtime',
          runtimeContextRefreshNonce: 3,
        },
      },
      sessionStreamingMessageDrafts: {
        session_1: {
          id: 'assistant_temp_1',
          sessionId: 'session_1',
          role: 'assistant',
          content: 'partial answer',
          status: 'streaming',
          createdAt: new Date('2026-05-25T10:00:00.000Z'),
          metadata: {
            conversation_turn_id: 'turn_1',
            toolCalls: [{
              id: 'tool_1',
              messageId: 'assistant_temp_1',
              name: 'browser_inspect',
              arguments: '{}',
              result: { huge: 'payload' },
              createdAt: new Date('2026-05-25T10:00:01.000Z'),
            }],
            contentSegments: [{ type: 'text', content: 'partial answer' }],
          },
        },
      },
    });

    const persistedCalls = localStorageMock.setItem.mock.calls
      .filter(([key]) => key === persistKey);
    const persistedRaw = persistedCalls[persistedCalls.length - 1]?.[1];
    expect(typeof persistedRaw).toBe('string');

    const persisted = JSON.parse(String(persistedRaw));
    expect(persisted.version).toBe(2);
    expect(persisted.state).toMatchObject({
      theme: 'dark',
      sidebarOpen: false,
      sessionAiSelectionBySession: {
        session_1: {
          selectedModelId: 'model_1',
          selectedAgentId: 'agent_1',
        },
      },
    });
    expect(persisted.state.sessionChatState).toBeUndefined();
    expect(persisted.state.sessionStreamingMessageDrafts).toBeUndefined();
  });

  it('migrates legacy persisted runtime state into the user-scoped store during rehydrate', async () => {
    const persistKey = resolveChatStorePersistKey('user_1');
    localStorageMock.setItem(
      LEGACY_CHAT_STORE_PERSIST_KEY,
      JSON.stringify({
        state: {
          theme: 'dark',
          sidebarOpen: false,
          sessionAiSelectionBySession: {
            session_1: {
              selectedModelId: 'model_1',
              selectedAgentId: 'agent_1',
            },
          },
          sessionChatState: {
            session_1: {
              isLoading: true,
              isStreaming: true,
              isStopping: false,
              streamingPhase: 'thinking',
              streamingMessageId: 'assistant_temp_1',
              activeTurnId: 'turn_1',
              streamingPreviewText: 'partial answer',
              streamingTransport: 'realtime',
              runtimeContextRefreshNonce: 2,
            },
          },
          sessionStreamingMessageDrafts: {
            session_1: {
              id: 'assistant_temp_1',
              sessionId: 'session_1',
              role: 'assistant',
              content: 'partial answer',
              status: 'streaming',
              createdAt: '2026-05-25T10:00:00.000Z',
              metadata: {
                conversation_turn_id: 'turn_1',
              },
            },
          },
        },
        version: 1,
      }),
    );

    const store = createChatStoreWithBackend({} as never, {
      userId: 'user_1',
    }) as PersistCapableStore;
    await store.persist?.rehydrate?.();

    expect(store.getState().theme).toBe('dark');
    expect(store.getState().sidebarOpen).toBe(false);
    expect(store.getState().sessionAiSelectionBySession).toEqual({
      session_1: {
        selectedModelId: 'model_1',
        selectedAgentId: 'agent_1',
      },
    });
    expect(store.getState().sessionChatState).toEqual({});
    expect(store.getState().sessionStreamingMessageDrafts).toEqual({});

    const persisted = JSON.parse(
      String(localStorageMock.getItem(persistKey)),
    );
    expect(persisted.version).toBe(2);
    expect(persisted.state.sessionChatState).toBeUndefined();
    expect(persisted.state.sessionStreamingMessageDrafts).toBeUndefined();
    expect(localStorageMock.getItem(LEGACY_CHAT_STORE_PERSIST_KEY)).toBeNull();
  });
});
