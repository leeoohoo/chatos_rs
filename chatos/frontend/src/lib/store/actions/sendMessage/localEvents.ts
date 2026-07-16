// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type ApiClient from '../../../api/client';
import type { LocalRuntimeEventRecord } from '../../../api/localRuntime/types';
import type { ChatStoreSet } from '../../types';
import { createDefaultSessionChatState } from './sessionState';

const LOCAL_EVENT_POLL_INTERVAL_MS = 180;
const LOCAL_EVENT_PAGE_SIZE = 200;

interface LocalEventContext {
  set: ChatStoreSet;
  sessionId: string;
  turnId: string;
}

export const applyLocalRuntimeEvents = (
  context: LocalEventContext,
  events: LocalRuntimeEventRecord[],
): void => {
  const ordered = [...events].sort((left, right) => left.event_seq - right.event_seq);
  ordered.forEach((event) => applyLocalRuntimeEvent(context, event));
};

export const startLocalRuntimeEventPolling = ({
  client,
  set,
  sessionId,
  turnId,
}: LocalEventContext & { client: ApiClient }) => {
  let afterSequence = 0;
  let stopping = false;
  let wakeDelay: (() => void) | null = null;
  let reportedPollingError = false;
  const localClient = client.getLocalRuntimeClient();

  const drain = async (): Promise<void> => {
    while (true) {
      const events = await localClient.getRuntimeEvents(sessionId, {
        turnId,
        after: afterSequence,
        limit: LOCAL_EVENT_PAGE_SIZE,
      });
      const nextEvents = events
        .filter((event) => event.event_seq > afterSequence)
        .sort((left, right) => left.event_seq - right.event_seq);
      if (nextEvents.length === 0) {
        return;
      }
      afterSequence = nextEvents[nextEvents.length - 1].event_seq;
      applyLocalRuntimeEvents({ set, sessionId, turnId }, nextEvents);
      if (events.length < LOCAL_EVENT_PAGE_SIZE) {
        return;
      }
    }
  };

  const drainSafely = async (): Promise<void> => {
    try {
      await drain();
    } catch (error) {
      if (!reportedPollingError) {
        reportedPollingError = true;
        console.warn('本地运行时事件轮询暂时不可用:', error);
      }
    }
  };

  const waitForNextPoll = (): Promise<void> => new Promise((resolve) => {
    const timer = globalThis.setTimeout(() => {
      wakeDelay = null;
      resolve();
    }, LOCAL_EVENT_POLL_INTERVAL_MS);
    wakeDelay = () => {
      globalThis.clearTimeout(timer);
      wakeDelay = null;
      resolve();
    };
  });

  const loop = (async () => {
    while (!stopping) {
      await drainSafely();
      if (!stopping) {
        await waitForNextPoll();
      }
    }
  })();

  return {
    stop: async (): Promise<void> => {
      stopping = true;
      wakeDelay?.();
      await loop;
      await drainSafely();
    },
  };
};

const applyLocalRuntimeEvent = (
  { set, sessionId, turnId }: LocalEventContext,
  event: LocalRuntimeEventRecord,
): void => {
  const payload = asRecord(event.payload);
  set((state) => {
    const previous = state.sessionChatState[sessionId] || createDefaultSessionChatState();
    if (previous.activeTurnId !== turnId) {
      return;
    }
    const next = {
      ...previous,
      streamingTransport: 'local' as const,
    };

    switch (event.event_name) {
      case 'chat.chunk': {
        const text = payloadText(payload);
        if (!text) {
          return;
        }
        next.isLoading = false;
        next.isStreaming = true;
        next.streamingPhase = null;
        next.streamingPreviewText = `${previous.streamingPreviewText || ''}${text}`;
        break;
      }
      case 'chat.thinking':
        next.isLoading = false;
        next.isStreaming = true;
        next.streamingPhase = 'thinking';
        break;
      case 'chat.tools.start':
      case 'chat.tools.stream':
      case 'chat.tools.end':
        next.isLoading = false;
        next.isStreaming = true;
        next.streamingPhase = 'reviewing';
        break;
      case 'chat.phase':
        next.isLoading = false;
        next.isStreaming = true;
        next.streamingPhase = payload.phase === 'continue' ? 'thinking' : null;
        break;
      case 'chat.completed':
        next.streamingPhase = null;
        break;
      case 'chat.cancelled':
      case 'chat.failed':
        next.isLoading = false;
        next.isStreaming = false;
        next.streamingPhase = null;
        break;
      default:
        return;
    }

    state.sessionChatState[sessionId] = next;
    if (state.currentSessionId === sessionId) {
      state.isLoading = next.isLoading;
      state.isStreaming = next.isStreaming;
    }
  });
};

const asRecord = (value: unknown): Record<string, unknown> => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {}
);

const payloadText = (payload: Record<string, unknown>): string => (
  typeof payload.text === 'string' ? payload.text : ''
);
