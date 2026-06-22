import type { MutableRefObject } from 'react';

import { shouldApplyProjectRunnerRequest } from './projectRunnerCatalogState';

export const createProjectRunnerRequestGuard = ({
  enabled,
  projectId,
  versionRef,
}: {
  enabled: boolean;
  projectId: string | null;
  versionRef: MutableRefObject<number>;
}): {
  requestVersion: number;
  shouldApply: () => boolean;
} | null => {
  if (!enabled || !projectId) {
    return null;
  }

  const requestVersion = ++versionRef.current;
  return {
    requestVersion,
    shouldApply: () => shouldApplyProjectRunnerRequest({
      currentVersion: versionRef.current,
      requestVersion,
      enabled,
      activeProjectId: projectId,
      requestProjectId: projectId,
    }),
  };
};
