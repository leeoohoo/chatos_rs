import type { Session } from '../../../types';
import type { TaskReviewPanelState, UiPromptPanelState } from '../../../lib/store/types';

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

export type TaskReviewPanelsBySessionMap = Record<string, TaskReviewPanelState[] | undefined>;

export type UiPromptPanelsBySessionMap = Record<string, UiPromptPanelState[] | undefined>;
