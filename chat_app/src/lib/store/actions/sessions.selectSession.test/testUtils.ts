import { afterEach, vi } from 'vitest';

import type { Message, Session } from '../../../../types';
import type {
  ChatStoreDraft,
  ChatStoreShape,
} from '../../types';
import {
  mergeLatestCompactHistorySnapshot as mergeLatestCompactHistorySnapshotImpl,
  readSessionMessagesCache as readSessionMessagesCacheImpl,
  readVisibleSessionMessagesSnapshot as readVisibleSessionMessagesSnapshotImpl,
  SESSION_MESSAGES_INITIAL_PAGE_SIZE as SESSION_MESSAGES_INITIAL_PAGE_SIZE_IMPL,
  writeSessionMessagesCache as writeSessionMessagesCacheImpl,
} from '../sessionsUtils';
import { setRealtimeConnectionStateSnapshot as setRealtimeConnectionStateSnapshotImpl } from '../../../realtime/state';

import { fetchSession as fetchSessionImpl } from '../../helpers/sessions';
import { fetchSessionMessages as fetchSessionMessagesImpl } from '../../helpers/messages';

const fetchSession = fetchSessionImpl;
const fetchSessionMessages = fetchSessionMessagesImpl;
const readSessionMessagesCache = readSessionMessagesCacheImpl;
const readVisibleSessionMessagesSnapshot = readVisibleSessionMessagesSnapshotImpl;
const SESSION_MESSAGES_INITIAL_PAGE_SIZE = SESSION_MESSAGES_INITIAL_PAGE_SIZE_IMPL;
const writeSessionMessagesCache = writeSessionMessagesCacheImpl;
const setRealtimeConnectionStateSnapshot = setRealtimeConnectionStateSnapshotImpl;

type FetchSessionMessagesResult = Awaited<ReturnType<typeof fetchSessionMessages>>;

afterEach(() => {
  setRealtimeConnectionStateSnapshot('idle');
  vi.clearAllMocks();
});

const createSession = (id: string): Session => ({
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
});

const createMessage = (
  sessionId: string,
  id: string,
  content: string,
  metadata: Message['metadata'] = {},
): Message => ({
  id,
  sessionId,
  role: 'assistant',
  content,
  status: 'completed',
  createdAt: new Date('2026-01-01T00:00:00.000Z'),
  metadata,
});

const installBackgroundSyncSpy = (state: ChatStoreShape) => {
  const syncSessionMessagesInBackground = vi.fn(async (sessionId: string) => {
    const result = await fetchSessionMessages({} as never, sessionId, { limit: 50, before: null });
    const preservedSnapshot = (
      readVisibleSessionMessagesSnapshot(state, sessionId)
      ?? readSessionMessagesCache(state, sessionId)
    );
    const mergedSnapshot = mergeLatestCompactHistorySnapshotImpl(
      result.messages,
      result.nextBefore,
      preservedSnapshot,
    );
    writeSessionMessagesCache(state, sessionId, {
      messages: mergedSnapshot.messages,
      nextBefore: mergedSnapshot.nextBefore,
      loaded: true,
    });
  });
  (state as ChatStoreShape & {
    syncSessionMessagesInBackground: typeof syncSessionMessagesInBackground;
  }).syncSessionMessagesInBackground = syncSessionMessagesInBackground;
  return syncSessionMessagesInBackground;
};

export {
  createMessage,
  createSession,
  fetchSession,
  fetchSessionMessages,
  installBackgroundSyncSpy,
  readSessionMessagesCache,
  SESSION_MESSAGES_INITIAL_PAGE_SIZE,
  setRealtimeConnectionStateSnapshot,
  writeSessionMessagesCache,
};
export type { ChatStoreDraft, ChatStoreShape, FetchSessionMessagesResult, Message };
