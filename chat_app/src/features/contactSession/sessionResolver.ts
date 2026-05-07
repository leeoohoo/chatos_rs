export type { ContactSessionRef } from '../../lib/domain/contactSessions';
export {
  findLatestMatchedSession,
  isSessionActive,
  isSessionMatchedContactAndProject,
  normalizeProjectScopeId,
  resolveContactAgentIdFromSession,
  resolveContactIdFromSession,
  resolveSessionProjectScopeId,
  resolveSessionTimestamp,
} from '../../lib/domain/contactSessions';
