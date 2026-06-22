import { describe, expect, it } from 'vitest';

import {
  mergeProjectRunRefreshMode,
  resolveProjectRunnerRealtimeCatalogAction,
  resolveProjectRunnerStatus,
  resolveProjectRunTargetSelection,
  shouldApplyProjectRunnerRequest,
} from './projectRunnerCatalogState';

describe('projectRunnerCatalogState', () => {
  it('prioritizes analyze mode when merging queued refresh modes', () => {
    expect(mergeProjectRunRefreshMode(null, 'catalog')).toBe('catalog');
    expect(mergeProjectRunRefreshMode('catalog', 'analyze')).toBe('analyze');
    expect(mergeProjectRunRefreshMode('analyze', 'catalog')).toBe('analyze');
  });

  it('only applies responses for the latest request of the active project', () => {
    expect(shouldApplyProjectRunnerRequest({
      currentVersion: 3,
      requestVersion: 3,
      enabled: true,
      activeProjectId: 'project_1',
      requestProjectId: 'project_1',
    })).toBe(true);

    expect(shouldApplyProjectRunnerRequest({
      currentVersion: 4,
      requestVersion: 3,
      enabled: true,
      activeProjectId: 'project_1',
      requestProjectId: 'project_1',
    })).toBe(false);
  });

  it('keeps the current target when still present and otherwise falls back to default or first target', () => {
    const targets = [
      { id: 'target_1', command: 'npm run dev', cwd: '/workspace', kind: 'node', label: 'Dev', source: 'auto', confidence: 1, requiredToolchains: [] },
      { id: 'target_2', command: 'npm run test', cwd: '/workspace', kind: 'node', label: 'Test', source: 'auto', confidence: 1, requiredToolchains: [] },
    ];

    expect(resolveProjectRunTargetSelection({
      currentSelectedRunTargetId: 'target_2',
      targets,
      defaultTargetId: 'target_1',
    })).toBe('target_2');

    expect(resolveProjectRunTargetSelection({
      currentSelectedRunTargetId: 'missing',
      targets,
      defaultTargetId: 'target_1',
    })).toBe('target_1');

    expect(resolveProjectRunTargetSelection({
      currentSelectedRunTargetId: null,
      targets,
      defaultTargetId: null,
    })).toBe('target_1');
  });

  it('derives run status from enablement, loading, error, and target presence', () => {
    expect(resolveProjectRunnerStatus({
      enabled: false,
      projectId: 'project_1',
      loading: false,
      errorMessage: null,
      targetCount: 1,
    })).toBe('idle');

    expect(resolveProjectRunnerStatus({
      enabled: true,
      projectId: 'project_1',
      loading: true,
      errorMessage: null,
      targetCount: 0,
    })).toBe('loading');

    expect(resolveProjectRunnerStatus({
      enabled: true,
      projectId: 'project_1',
      loading: false,
      errorMessage: 'boom',
      targetCount: 0,
    })).toBe('error');

    expect(resolveProjectRunnerStatus({
      enabled: true,
      projectId: 'project_1',
      loading: false,
      errorMessage: null,
      targetCount: 2,
    })).toBe('ready');

    expect(resolveProjectRunnerStatus({
      enabled: true,
      projectId: 'project_1',
      loading: false,
      errorMessage: null,
      targetCount: 0,
    })).toBe('empty');
  });

  it('maps realtime catalog reasons to direct state actions', () => {
    expect(resolveProjectRunnerRealtimeCatalogAction({
      kind: 'project_run_catalog',
      project_id: 'project_1',
      reason: 'project_root_missing',
    })).toBe('reset');

    expect(resolveProjectRunnerRealtimeCatalogAction({
      kind: 'project_run_catalog',
      project_id: 'project_1',
      reason: 'project_run_environment_changed',
    })).toBe('reload_environment');

    expect(resolveProjectRunnerRealtimeCatalogAction({
      kind: 'project_run_catalog',
      project_id: 'project_1',
      reason: 'catalog_updated',
    })).toBe('reload_catalog');
  });

});
