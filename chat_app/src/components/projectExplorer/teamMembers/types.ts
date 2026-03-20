import type { Session } from '../../../types';

export interface ContactItem {
  id: string;
  agentId: string;
  name: string;
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
  } | undefined
>;
