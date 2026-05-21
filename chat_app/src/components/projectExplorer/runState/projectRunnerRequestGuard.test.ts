import { describe, expect, it } from 'vitest';

import { createProjectRunnerRequestGuard } from './projectRunnerRequestGuard';

describe('projectRunnerRequestGuard', () => {
  it('creates a guard for the active project request', () => {
    const versionRef = { current: 0 };
    const guard = createProjectRunnerRequestGuard({
      enabled: true,
      projectId: 'project_1',
      versionRef,
    });

    expect(guard).not.toBeNull();
    expect(versionRef.current).toBe(1);
    expect(guard?.shouldApply()).toBe(true);
  });

  it('returns null when disabled or missing project id', () => {
    expect(createProjectRunnerRequestGuard({
      enabled: false,
      projectId: 'project_1',
      versionRef: { current: 0 },
    })).toBeNull();
  });
});
