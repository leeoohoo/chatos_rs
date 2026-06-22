import type { RealtimeProjectRunCatalogPayloadWrapper } from '../../../lib/realtime/types';
import type { ProjectRunTarget } from '../../../types';

export type ProjectRunRefreshMode = 'catalog' | 'analyze';
export type ProjectRunStatus = 'idle' | 'loading' | 'error' | 'ready' | 'empty';
export type ProjectRunRealtimeCatalogAction = 'reset' | 'reload_environment' | 'reload_catalog';

export const mergeProjectRunRefreshMode = (
  current: ProjectRunRefreshMode | null,
  next: ProjectRunRefreshMode,
): ProjectRunRefreshMode => {
  if (current === 'analyze' || next === 'analyze') {
    return 'analyze';
  }
  return 'catalog';
};

export const shouldApplyProjectRunnerRequest = ({
  currentVersion,
  requestVersion,
  enabled,
  activeProjectId,
  requestProjectId,
}: {
  currentVersion: number;
  requestVersion: number;
  enabled: boolean;
  activeProjectId: string | null;
  requestProjectId: string;
}): boolean => (
  currentVersion === requestVersion
  && enabled
  && activeProjectId === requestProjectId
);

export const resolveProjectRunTargetSelection = ({
  currentSelectedRunTargetId,
  targets,
  defaultTargetId,
}: {
  currentSelectedRunTargetId: string | null;
  targets: ProjectRunTarget[];
  defaultTargetId?: string | null;
}): string | null => {
  if (currentSelectedRunTargetId && targets.some((item) => item.id === currentSelectedRunTargetId)) {
    return currentSelectedRunTargetId;
  }

  const normalizedDefaultTargetId = typeof defaultTargetId === 'string'
    ? defaultTargetId.trim()
    : '';
  return normalizedDefaultTargetId || targets[0]?.id || null;
};

export const resolveProjectRunnerStatus = ({
  enabled,
  projectId,
  loading,
  errorMessage,
  targetCount,
}: {
  enabled: boolean;
  projectId: string | null;
  loading: boolean;
  errorMessage: string | null;
  targetCount: number;
}): ProjectRunStatus => {
  if (!enabled || !projectId) {
    return 'idle';
  }
  if (loading) {
    return 'loading';
  }
  if (errorMessage) {
    return 'error';
  }
  if (targetCount > 0) {
    return 'ready';
  }
  return 'empty';
};

export const resolveProjectRunnerRealtimeCatalogAction = (
  payload: RealtimeProjectRunCatalogPayloadWrapper,
): ProjectRunRealtimeCatalogAction => {
  const reason = String(payload.reason || '').trim();
  if (reason === 'project_root_missing') {
    return 'reset';
  }
  if (reason === 'project_run_environment_changed') {
    return 'reload_environment';
  }
  return 'reload_catalog';
};
