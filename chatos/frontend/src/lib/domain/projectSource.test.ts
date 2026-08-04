// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { resolveProjectSourceKind } from './projectSource';

describe('resolveProjectSourceKind', () => {
  it('keeps project source independent from cloud orchestration', () => {
    const project = {
      sourceType: 'local_connector',
      executionPlane: 'cloud',
      rootPath: 'local://connector/device/workspace/project',
    };
    expect(resolveProjectSourceKind(project)).toBe('local_connector');
  });

  it('recognizes legacy local projects and connector roots', () => {
    expect(resolveProjectSourceKind({
      sourceType: 'local',
      rootPath: '/workspace/project',
    })).toBe('local_connector');
    expect(resolveProjectSourceKind({
      sourceType: null,
      rootPath: 'local://connector/device/workspace/project',
    })).toBe('local_connector');
  });

  it('recognizes cloud and unknown project sources', () => {
    expect(resolveProjectSourceKind({
      sourceType: 'cloud',
      rootPath: 'harness://project/project-1',
    })).toBe('cloud');
    expect(resolveProjectSourceKind({
      sourceType: null,
      rootPath: '',
    })).toBe('cloud');
  });
});
