import { describe, expect, it, vi } from 'vitest';

import { createConversationControlActions } from './conversationControl';
import { createDefaultSessionChatState } from './sendMessage/sessionState';

describe('createConversationControlActions.abortCurrentConversation', () => {
  const buildState = () => ({
    currentSessionId: 'session-1',
    selectedModelId: 'model-1',
    aiModelConfigs: [
      { id: 'model-1', supports_responses: true },
    ],
    sessionAiSelectionBySession: {},
    sessionChatState: {
      'session-1': {
        ...createDefaultSessionChatState(),
        isLoading: true,
        isStreaming: true,
        streamingMessageId: 'assistant-1',
      },
    },
    isLoading: true,
    isStreaming: true,
    streamingMessageId: 'assistant-1',
  });

  it('prefers session websocket abort when available', async () => {
    const state = buildState();
    const client = {
      stopChat: vi.fn(),
    } as any;
    const abortSessionChat = vi.fn(async () => true);
    const set = (updater: (draft: any) => void) => updater(state);
    const get = () => state as any;

    const actions = createConversationControlActions({
      set: set as any,
      get: get as any,
      client,
      abortSessionChat,
    });

    await actions.abortCurrentConversation();

    expect(abortSessionChat).toHaveBeenCalledWith('session-1');
    expect(client.stopChat).not.toHaveBeenCalled();
    expect(state.sessionChatState['session-1'].isStopping).toBe(true);
    expect(state.sessionChatState['session-1'].isStreaming).toBe(true);
  });

  it('falls back to HTTP stop when websocket abort is unavailable', async () => {
    const state = buildState();
    const client = {
      stopChat: vi.fn().mockResolvedValue({ success: true }),
    } as any;
    const abortSessionChat = vi.fn(async () => false);
    const set = (updater: (draft: any) => void) => updater(state);
    const get = () => state as any;

    const actions = createConversationControlActions({
      set: set as any,
      get: get as any,
      client,
      abortSessionChat,
    });

    await actions.abortCurrentConversation();

    expect(abortSessionChat).toHaveBeenCalledWith('session-1');
    expect(client.stopChat).toHaveBeenCalledWith('session-1', { useResponses: true });
  });
});
