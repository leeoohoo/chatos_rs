// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { resolveProjectExecutionPlane } from './projectExecution';

describe('resolveProjectExecutionPlane', () => {
  it('routes explicit cloud projects to the cloud', () => {
    expect(resolveProjectExecutionPlane({
      executionPlane: 'cloud',
      sourceType: 'local_connector',
      rootPath: 'local://connector/device/workspace',
    })).toBe('cloud');
  });

  it('routes legacy cloud source types to the cloud', () => {
    expect(resolveProjectExecutionPlane({
      sourceType: 'cloud',
      rootPath: 'harness://project/project-1',
    })).toBe('cloud');
  });

  it('routes local project variants to the local connector', () => {
    expect(resolveProjectExecutionPlane({
      sourceType: 'local',
      rootPath: '/workspace/project',
    })).toBe('local_connector');
    expect(resolveProjectExecutionPlane({
      sourceType: 'local_connector',
      rootPath: 'local://connector/device/workspace/project',
    })).toBe('local_connector');
  });

  it('defaults unknown project types to the cloud', () => {
    expect(resolveProjectExecutionPlane({
      sourceType: null,
      rootPath: '/workspace/project',
    })).toBe('cloud');
  });
});
