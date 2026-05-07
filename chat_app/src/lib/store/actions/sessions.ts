import { createSessionCreateActions } from './sessions/createSession';
import { createLoadSessionActions } from './sessions/loadSessions';
import { createSessionMutationActions } from './sessions/mutations';
import { createSelectSessionActions } from './sessions/selectSession';
import type { SessionActionDeps } from './sessions/types';

export function createSessionActions(deps: SessionActionDeps) {
  return {
    ...createLoadSessionActions(deps),
    ...createSessionCreateActions(deps),
    ...createSelectSessionActions(deps),
    ...createSessionMutationActions(deps),
  };
}
