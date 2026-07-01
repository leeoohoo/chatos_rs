// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Session } from '../../../types';

export interface ContactItem {
  id: string;
  agentId: string;
  name: string;
  taskRunner?: {
    enabled: boolean;
    baseUrl: string;
    username: string;
    hasPassword: boolean;
  };
}

export interface ProjectContactRow {
  contact: ContactItem;
  session: Session | null;
  latestSessionId: string | null;
  lastMessageAt: string | null;
  updatedAt: number;
}

export interface EnsureProjectContactSessionOptions {
  createIfMissing?: boolean;
}

export interface ProjectContactLink {
  contactId: string;
  agentId: string;
  name: string;
  latestSessionId: string | null;
  lastMessageAt: string | null;
  updatedAt: number;
}

export type SessionChatStateMap = Record<
  string,
  {
    isLoading?: boolean;
    isStreaming?: boolean;
    isStopping?: boolean;
    streamingPhase?: 'thinking' | 'reviewing' | null;
  } | undefined
>;
