import { describe, expect, it, vi } from 'vitest';

import { handleStreamEvent } from './streamEventHandler';
import type { StreamEventPayload } from './types';

const buildHelpers = () => ({
  flushPendingTextToStreamingMessage: vi.fn(),
  appendTextToStreamingMessage: vi.fn(),
  appendThinkingToStreamingMessage: vi.fn(),
  applyCompleteContent: vi.fn(),
  ensureStreamingMessage: vi.fn(),
  persistStreamingMessageDraft: vi.fn(),
  updateTurnHistoryProcess: vi.fn(),
});

describe('handleStreamEvent', () => {
  it('ignores complete content when streamed text already exists', () => {
    const helpers = buildHelpers();
    const result = handleStreamEvent({
      parsed: {
        type: 'complete',
        result: { content: '最终完整内容' },
      } as StreamEventPayload,
      set: vi.fn(),
      currentSessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_1',
      streamedTextRef: { value: '已经收到的流式正文' },
      helpers: helpers as any,
    });

    expect(result.sawDone).toBe(true);
    expect(helpers.flushPendingTextToStreamingMessage).toHaveBeenCalledTimes(2);
    expect(helpers.applyCompleteContent).not.toHaveBeenCalled();
  });

  it('falls back to complete content only when no streamed text exists', () => {
    const helpers = buildHelpers();
    const result = handleStreamEvent({
      parsed: {
        type: 'complete',
        result: { content: '最终完整内容' },
      } as StreamEventPayload,
      set: vi.fn(),
      currentSessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_1',
      streamedTextRef: { value: '' },
      helpers: helpers as any,
    });

    expect(result.sawDone).toBe(true);
    expect(helpers.flushPendingTextToStreamingMessage).toHaveBeenCalledTimes(2);
    expect(helpers.applyCompleteContent).toHaveBeenCalledWith('最终完整内容');
  });
});
