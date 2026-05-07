import { describe, expect, it, vi } from 'vitest';

import { handleStreamEvent } from './streamEventHandler';
import type { Message } from '../../../../types';
import type { StreamEventPayload } from './types';
import type { StreamingMessageStateHelpers } from './streamingState';

const buildHelpers = () => ({
  flushPendingTextToStreamingMessage: vi.fn(),
  appendTextToStreamingMessage: vi.fn(),
  appendThinkingToStreamingMessage: vi.fn(),
  applyCompleteContent: vi.fn(),
  ensureStreamingMessage: vi.fn(),
  persistStreamingMessageDraft: vi.fn(),
  updateTurnHistoryProcess: vi.fn(),
}) satisfies StreamingMessageStateHelpers;

describe('handleStreamEvent', () => {
  it('queues runtime guidance state for the current session', () => {
    const helpers = buildHelpers();
    const state = {
      currentSessionId: 'session_1',
      messages: [],
      sessionRuntimeGuidanceState: {},
    } as {
      currentSessionId: string;
      messages: Message[];
      sessionRuntimeGuidanceState: Record<string, unknown>;
    };
    const set = vi.fn((updater) => {
      updater(state);
    });

    const result = handleStreamEvent({
      parsed: {
        type: 'runtime_guidance_queued',
        timestamp: '2026-04-23T10:00:00.000Z',
        data: {
          guidance_id: 'guidance_1',
          turn_id: 'turn_1',
          content: '请优先读取项目 README',
          pending_count: 3,
        },
      } as StreamEventPayload,
      set,
      currentSessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_1',
      streamedTextRef: { value: '' },
      helpers,
    });

    expect(result).toEqual({
      sawCancelled: false,
      sawDone: false,
      sawMeaningfulStreamData: false,
    });
    expect(state.sessionRuntimeGuidanceState.session_1).toMatchObject({
      pendingCount: 3,
      lastGuidanceAt: '2026-04-23T10:00:00.000Z',
      items: [
        {
          guidanceId: 'guidance_1',
          turnId: 'turn_1',
          content: '请优先读取项目 README',
          status: 'queued',
        },
      ],
    });
    expect(helpers.flushPendingTextToStreamingMessage).toHaveBeenCalledTimes(1);
  });

  it('applies runtime guidance state for the current session', () => {
    const helpers = buildHelpers();
    const state = {
      currentSessionId: 'session_1',
      messages: [],
      sessionRuntimeGuidanceState: {
        session_1: {
          pendingCount: 2,
          appliedCount: 0,
          lastGuidanceAt: '2026-04-23T10:00:00.000Z',
          lastAppliedAt: null,
          items: [
            {
              guidanceId: 'guidance_1',
              turnId: 'turn_1',
              content: '请优先读取项目 README',
              status: 'queued',
              createdAt: '2026-04-23T10:00:00.000Z',
              appliedAt: null,
            },
          ],
        },
      },
    } as {
      currentSessionId: string;
      messages: Message[];
      sessionRuntimeGuidanceState: Record<string, unknown>;
    };
    const set = vi.fn((updater) => {
      updater(state);
    });

    const result = handleStreamEvent({
      parsed: {
        type: 'runtime_guidance_applied',
        timestamp: '2026-04-23T10:00:05.000Z',
        data: {
          guidance_id: 'guidance_1',
          turn_id: 'turn_1',
          applied_at: '2026-04-23T10:00:05.000Z',
          pending_count: 1,
        },
      } as StreamEventPayload,
      set,
      currentSessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_1',
      streamedTextRef: { value: '' },
      helpers,
    });

    expect(result).toEqual({
      sawCancelled: false,
      sawDone: false,
      sawMeaningfulStreamData: false,
    });
    expect(state.sessionRuntimeGuidanceState.session_1).toMatchObject({
      pendingCount: 1,
      appliedCount: 1,
      lastAppliedAt: '2026-04-23T10:00:05.000Z',
      items: [
        {
          guidanceId: 'guidance_1',
          status: 'applied',
          appliedAt: '2026-04-23T10:00:05.000Z',
        },
      ],
    });
    expect(helpers.flushPendingTextToStreamingMessage).toHaveBeenCalledTimes(1);
  });

  it('throws formatted stream errors with code when present', () => {
    const helpers = buildHelpers();

    expect(() => handleStreamEvent({
      parsed: {
        type: 'error',
        code: 'RATE_LIMITED',
        data: {
          message: '请求过于频繁',
        },
      } as StreamEventPayload,
      set: vi.fn(),
      currentSessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_1',
      streamedTextRef: { value: '' },
      helpers,
    })).toThrow('[RATE_LIMITED] 请求过于频繁');

    expect(helpers.flushPendingTextToStreamingMessage).toHaveBeenCalledTimes(1);
  });

  it('throws fallback stream errors without code prefix when absent', () => {
    const helpers = buildHelpers();

    expect(() => handleStreamEvent({
      parsed: {
        type: 'error',
        data: {
          error: {
            message: '连接已断开',
          },
        },
      } as StreamEventPayload,
      set: vi.fn(),
      currentSessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_1',
      streamedTextRef: { value: '' },
      helpers,
    })).toThrow('连接已断开');

    expect(helpers.flushPendingTextToStreamingMessage).toHaveBeenCalledTimes(1);
  });

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
      helpers,
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
      helpers,
    });

    expect(result.sawDone).toBe(true);
    expect(helpers.flushPendingTextToStreamingMessage).toHaveBeenCalledTimes(2);
    expect(helpers.applyCompleteContent).toHaveBeenCalledWith('最终完整内容');
  });

  it('does not persist unavailable tools when payload adds no new entries', () => {
    const helpers = buildHelpers();
    const message: Message = {
      id: 'assistant_1',
      sessionId: 'session_1',
      role: 'assistant',
      content: '',
      status: 'streaming',
      createdAt: new Date('2026-04-23T00:00:00.000Z'),
      metadata: {
        unavailableTools: [
          {
            id: 'existing_1',
            serverName: 'alpha',
            toolName: 'search',
            reason: 'offline',
          },
        ],
      },
    };
    helpers.ensureStreamingMessage.mockReturnValue(message);

    const set = vi.fn((updater) => {
      updater({
        sessionChatState: {},
        taskReviewPanelsBySession: {},
        uiPromptPanelsBySession: {},
        currentSessionId: 'session_1',
      });
    });

    const result = handleStreamEvent({
      parsed: {
        type: 'tools_unavailable',
        data: {
          unavailable_tools: [
            {
              server_name: 'alpha',
              tool_name: 'search',
              reason: 'offline',
            },
          ],
        },
      } as StreamEventPayload,
      set,
      currentSessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_1',
      streamedTextRef: { value: '' },
      helpers,
    });

    expect(result.sawMeaningfulStreamData).toBe(true);
    expect(helpers.updateTurnHistoryProcess).not.toHaveBeenCalled();
    expect(helpers.persistStreamingMessageDraft).not.toHaveBeenCalled();
  });

  it('updates streaming preview text for chunk events', () => {
    const helpers = buildHelpers();
    const set = vi.fn((updater) => {
      updater({
        sessionChatState: {
          session_1: {
            streamingPreviewText: '',
          },
        },
      });
    });

    const result = handleStreamEvent({
      parsed: {
        type: 'chunk',
        content: 'hello world',
      } as StreamEventPayload,
      set,
      currentSessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_1',
      streamedTextRef: { value: '' },
      helpers,
    });

    expect(result.sawMeaningfulStreamData).toBe(true);
    expect(set).toHaveBeenCalled();
  });

  it('marks pending tool calls as cancelled on cancelled events', () => {
    const helpers = buildHelpers();
    const message = {
      id: 'assistant_1',
      sessionId: 'session_1',
      role: 'assistant',
      content: '',
      status: 'streaming',
      createdAt: new Date('2026-04-23T00:00:00.000Z'),
      metadata: {
        toolCalls: [
          {
            id: 'tool_1',
            messageId: 'assistant_1',
            name: 'workspace_search',
            arguments: '{}',
            result: '',
            completed: false,
          },
        ],
      },
    } as unknown as Message;
    helpers.ensureStreamingMessage.mockReturnValue(message);

    const set = vi.fn((updater) => {
      updater({});
    });

    const result = handleStreamEvent({
      parsed: {
        type: 'cancelled',
      } as StreamEventPayload,
      set,
      currentSessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_1',
      streamedTextRef: { value: '' },
      helpers,
    });

    expect(result.sawCancelled).toBe(true);
    expect(helpers.persistStreamingMessageDraft).toHaveBeenCalled();
    expect(message.metadata?.toolCalls?.[0]).toMatchObject({
      completed: true,
      error: '已取消',
    });
  });
});
