// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { resolveVisibleWorkspaceTabs } from './workspaceTabsModel';

describe('resolveVisibleWorkspaceTabs', () => {
  it('uses one workspace tab set for every project', () => {
    expect(resolveVisibleWorkspaceTabs()).toEqual([
      'files',
      'team',
      'plan',
      'settings',
    ]);
  });
});
