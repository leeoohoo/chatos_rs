// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type { ContactSessionRef } from '../../lib/domain/contactSessions';
export {
  findBestMatchedSession,
  findLatestMatchedSession,
  hasSessionMessages,
  isSessionActive,
  isSessionMatchedContactAndProject,
  normalizeProjectScopeId,
  PUBLIC_PROJECT_ID,
  resolveContactAgentIdFromSession,
  resolveContactIdFromSession,
  resolveSessionMessageCount,
  resolveSessionProjectScopeId,
  resolveSessionTimestamp,
} from '../../lib/domain/contactSessions';
