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
