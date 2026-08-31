// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Session } from '../../../types';
import {
  isSessionActive as isSessionActiveDomain,
  matchSessionContactProjectScope as matchSessionContactProjectScopeDomain,
  normalizeContactSessions as normalizeContactSessionsDomain,
  normalizeMemoryContact,
  PUBLIC_PROJECT_ID,
  normalizeProjectScopeId as normalizeProjectScopeIdDomain,
  resolveSessionContactIdentity as resolveSessionContactIdentityDomain,
  resolveSessionProjectScopeId as resolveSessionProjectScopeIdDomain,
  resolveSessionTimestamp as resolveSessionTimestampDomain,
  splitSessionsByMappedContacts as splitSessionsByMappedContactsDomain,
} from '../../domain/contactSessions';
import {
  normalizeDate as normalizeUnknownDate,
} from '../helpers/normalizerUtils';
import type {
  ChatState,
} from '../types';

export const SESSION_MESSAGES_INITIAL_PAGE_SIZE = 5;

export const createPerfMeasureStopper = (measureName: string): (() => number | null) => {
  if (typeof performance === 'undefined' || typeof performance.mark !== 'function' || typeof performance.measure !== 'function') {
    return () => null;
  }

  const startMark = `${measureName}:start`;
  const endMark = `${measureName}:end`;
  performance.mark(startMark);

  return () => {
    performance.mark(endMark);
    performance.measure(measureName, startMark, endMark);
    const entries = performance.getEntriesByName(measureName);
    const duration = entries.length > 0 ? entries[entries.length - 1].duration : null;
    performance.clearMarks(startMark);
    performance.clearMarks(endMark);
    performance.clearMeasures(measureName);
    return duration;
  };
};

type CurrentSessionViewState = Pick<
  ChatState,
  'currentSessionId' | 'currentSession' | 'messages' | 'selectedModelId' | 'selectedAgentId' | 'isLoading' | 'isStreaming' | 'streamingMessageId' | 'hasMoreMessages'
>;

export const resetCurrentSessionViewState = (state: CurrentSessionViewState) => {
  state.currentSessionId = null;
  state.currentSession = null;
  state.selectedModelId = null;
  state.selectedAgentId = null;
  state.messages = [];
  state.isLoading = false;
  state.isStreaming = false;
  state.streamingMessageId = null;
  state.hasMoreMessages = false;
};

type SessionProjectSyncState = Pick<ChatState, 'projects' | 'currentProjectId' | 'currentProject'>;

export const normalizeDate = normalizeUnknownDate;

export const resolveSessionTimestamp = resolveSessionTimestampDomain;

export const normalizeProjectScopeId = normalizeProjectScopeIdDomain;

export const resolveSessionProjectScopeId = resolveSessionProjectScopeIdDomain;

export const resolveSessionContactIdentity = resolveSessionContactIdentityDomain;

export const isSessionActive = isSessionActiveDomain;

export const matchSessionContactProjectScope = matchSessionContactProjectScopeDomain;

export const splitSessionsByMappedContacts = splitSessionsByMappedContactsDomain;

export const normalizeContactSessions = normalizeContactSessionsDomain;

export const syncCurrentProjectFromSession = (
  state: SessionProjectSyncState,
  session: Session | null | undefined,
) => {
  const projectId = resolveSessionProjectScopeId(session);
  if (!projectId || projectId === PUBLIC_PROJECT_ID) {
    state.currentProjectId = null;
    state.currentProject = null;
    return;
  }

  state.currentProjectId = projectId;
  state.currentProject = (state.projects || []).find((project) => project.id === projectId) || null;
};

export type { MemoryContact } from '../../domain/contactSessions';

export const normalizeContact = normalizeMemoryContact;
