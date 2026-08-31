// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, vi } from 'vitest';

import type { Message, Session } from '../../../../types';
import type {
  ChatStoreDraft,
  ChatStoreShape,
} from '../../types';
import { SESSION_MESSAGES_INITIAL_PAGE_SIZE as SESSION_MESSAGES_INITIAL_PAGE_SIZE_IMPL } from '../sessionsUtils';

import { fetchSession as fetchSessionImpl } from '../../helpers/sessions';
import { fetchSessionMessages as fetchSessionMessagesImpl } from '../../helpers/messages';

const fetchSession = fetchSessionImpl;
const fetchSessionMessages = fetchSessionMessagesImpl;
const SESSION_MESSAGES_INITIAL_PAGE_SIZE = SESSION_MESSAGES_INITIAL_PAGE_SIZE_IMPL;

type FetchSessionMessagesResult = Awaited<ReturnType<typeof fetchSessionMessages>>;

afterEach(() => {
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

export {
  createMessage,
  createSession,
  fetchSession,
  fetchSessionMessages,
  SESSION_MESSAGES_INITIAL_PAGE_SIZE,
};
export type { ChatStoreDraft, ChatStoreShape, FetchSessionMessagesResult, Message };
