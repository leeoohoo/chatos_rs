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
  updatedAt: number;
}

export interface ProjectContactLink {
  contactId: string;
  agentId: string;
  name: string;
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
