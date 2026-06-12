import type { ContactRecord, Session } from '../../../types';

export interface SessionAiSelection {
  selectedModelId: string | null;
  selectedAgentId: string | null;
}

export interface SessionCreatePayload {
  title?: string;
  contactAgentId?: string | null;
  contactId?: string | null;
  selectedModelId?: string | null;
  projectId?: string | null;
  projectRoot?: string | null;
}

export interface SessionSelectOptions {
  keepActivePanel?: boolean;
  initialPageSize?: number;
  skipBackgroundSync?: boolean;
}

export interface SessionCreateOptions {
  keepActivePanel?: boolean;
  activateSession?: boolean;
}

export interface SessionSliceState {
  sessions: Session[];
  currentSessionId: string | null;
  currentSession: Session | null;
  contacts: ContactRecord[];
  sessionAiSelectionBySession: Record<string, SessionAiSelection>;
}

export const sessionInitialState: SessionSliceState = {
  sessions: [],
  currentSessionId: null,
  currentSession: null,
  contacts: [],
  sessionAiSelectionBySession: {},
};

export interface SessionSliceActions {
  loadContacts: (options?: { force?: boolean }) => Promise<ContactRecord[]>;
  createContact: (agentId: string, agentNameSnapshot?: string) => Promise<ContactRecord>;
  updateContactTaskRunnerConfig: (
    contactId: string,
    config: {
      enabled: boolean;
      baseUrl: string;
      username: string;
      password?: string;
      clearPassword?: boolean;
    },
  ) => Promise<ContactRecord>;
  deleteContact: (contactId: string) => Promise<void>;
  getContactByAgentId: (agentId: string) => ContactRecord | null;
  markContactsStale: (userId?: string | null) => void;
  removeContactLocally: (contactId: string) => void;
  applyRealtimeContactSnapshot: (contact: ContactRecord | unknown) => ContactRecord | null;
  refreshContactById: (contactId: string) => Promise<ContactRecord | null>;

  loadSessions: (options?: { force?: boolean; limit?: number; offset?: number; append?: boolean; silent?: boolean }) => Promise<Session[]>;
  createSession: (
    payload?: string | SessionCreatePayload,
    options?: SessionCreateOptions,
  ) => Promise<string>;
  selectSession: (sessionId: string, options?: SessionSelectOptions) => Promise<void>;
  updateSession: (sessionId: string, updates: Partial<Session>) => Promise<void>;
  deleteSession: (sessionId: string) => Promise<void>;
  markSessionsStale: (options?: { userId?: string | null; sessionId?: string | null }) => void;
  removeSessionLocally: (sessionId: string) => void;
  applyRealtimeSessionSnapshot: (session: Session | unknown) => Session | null;
  refreshSessionById: (sessionId: string) => Promise<Session | null>;
}
