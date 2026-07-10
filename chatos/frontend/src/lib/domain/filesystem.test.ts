// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import {
  deriveParentPath,
  getUserVisiblePath,
  normalizeFsEntry,
  resolveUserVisiblePathInput,
} from './filesystem';

describe('domain/filesystem', () => {
  it('derives parent paths for unix and windows directories', () => {
    expect(deriveParentPath('/srv/app/src')).toBe('/srv/app');
    expect(deriveParentPath('/Users/lilei/project/my_project')).toBe('/Users/lilei/project');
    expect(deriveParentPath('/')).toBeNull();
    expect(deriveParentPath('C:\\workspace\\demo')).toBe('C:\\workspace');
    expect(deriveParentPath('C:\\')).toBe('C:\\');
  });

  it('formats user-scoped workspace paths as paths under the user root', () => {
    const root = '/opt/chatos/backend/data/workspace/users/user-123/workspaces';
    expect(getUserVisiblePath(root)).toBe('/');
    expect(getUserVisiblePath(`${root}/demo/src`)).toBe('/demo/src');
    expect(getUserVisiblePath(`${root}/demo/src`, `${root}/demo`)).toBe('/src');
  });

  it('hides the internal Harness project URI', () => {
    const root = 'harness://project/project-1';
    expect(getUserVisiblePath(root)).toBe('/');
    expect(getUserVisiblePath(`${root}/src/main.rs`)).toBe('/src/main.rs');
    expect(getUserVisiblePath(`${root}/src/main.rs`, root)).toBe('/src/main.rs');
  });

  it('converts edited user-visible paths back to the current scoped root', () => {
    const current = '/opt/chatos/backend/data/workspace/users/user-123/workspaces/demo';
    expect(resolveUserVisiblePathInput('/next', current)).toBe(
      '/opt/chatos/backend/data/workspace/users/user-123/workspaces/next',
    );
    expect(resolveUserVisiblePathInput('nested/app', current)).toBe(
      '/opt/chatos/backend/data/workspace/users/user-123/workspaces/nested/app',
    );
  });

  it('keeps backend-provided display paths on filesystem entries', () => {
    expect(normalizeFsEntry({
      name: 'demo',
      path: '/opt/chatos/backend/data/workspace/users/user-123/workspaces/demo',
      display_path: '/demo',
      is_dir: true,
    })).toMatchObject({
      name: 'demo',
      path: '/opt/chatos/backend/data/workspace/users/user-123/workspaces/demo',
      displayPath: '/demo',
      isDir: true,
    });
  });
});
