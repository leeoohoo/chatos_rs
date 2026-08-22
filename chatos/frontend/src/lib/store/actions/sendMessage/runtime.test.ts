// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { PUBLIC_PROJECT_ID } from '../../../domain/contactSessions';
import { resolveRuntimeConfig } from './runtime';

describe('resolveRuntimeConfig project isolation', () => {
  it('normalizes legacy public scope and clears inherited project roots', () => {
    const resolved = resolveRuntimeConfig({
      projectId: '0',
      projectRoot: '/workspace/should-not-leak',
    }, {});

    expect(resolved.effectiveProjectId).toBe(PUBLIC_PROJECT_ID);
    expect(resolved.effectiveProjectRoot).toBeNull();
    expect(resolved.effectiveExecutionRoot).toBeNull();
  });

  it('clears project roots for the canonical public scope', () => {
    const resolved = resolveRuntimeConfig({
      projectId: PUBLIC_PROJECT_ID,
      projectRoot: '/workspace/should-not-leak',
    }, {});

    expect(resolved.effectiveProjectId).toBe(PUBLIC_PROJECT_ID);
    expect(resolved.effectiveProjectRoot).toBeNull();
    expect(resolved.effectiveExecutionRoot).toBeNull();
  });

  it('keeps the project root for a concrete project', () => {
    const resolved = resolveRuntimeConfig({
      projectId: 'project-1',
      projectRoot: '/workspace/project-1',
    }, {});

    expect(resolved.effectiveProjectId).toBe('project-1');
    expect(resolved.effectiveProjectRoot).toBe('/workspace/project-1');
    expect(resolved.effectiveExecutionRoot).toBe('/workspace/project-1');
  });
});
