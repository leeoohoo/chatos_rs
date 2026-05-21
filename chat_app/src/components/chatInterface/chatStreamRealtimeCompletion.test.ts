import { describe, expect, it, vi } from 'vitest';

import { handleChatStreamRealtimeCompletion } from './chatStreamRealtimeCompletion';

describe('chatStreamRealtimeCompletion', () => {
  it('ignores payloads without a conversation id', async () => {
    const apiClient = {};
    const chatStoreSet = vi.fn();

    await handleChatStreamRealtimeCompletion({
      payload: {
        kind: 'chat_stream',
        conversation_id: '',
        stream_type: 'message_delta',
        raw: {},
      } as never,
      storeGetState: () => ({}) as never,
      chatStoreSet: chatStoreSet as never,
      apiClient: apiClient as never,
      processedCompletionKeysRef: { current: new Set<string>() },
    });

    expect(chatStoreSet).not.toHaveBeenCalled();
  });
});
