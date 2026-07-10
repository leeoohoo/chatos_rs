// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { resolveVisibleWorkspaceTabs } from './workspaceTabsModel';

describe('resolveVisibleWorkspaceTabs', () => {
  it('uses the sandbox runtime tab instead of project settings for cloud projects', () => {
    expect(resolveVisibleWorkspaceTabs(true, true)).toEqual([
      'files',
      'team',
      'plan',
      'sandbox',
    ]);
  });

  it('adds sandbox runtime without removing local project settings when enabled', () => {
    expect(resolveVisibleWorkspaceTabs(false, true)).toEqual([
      'files',
      'team',
      'plan',
      'settings',
      'sandbox',
    ]);
  });

  it('keeps sandbox runtime hidden for local projects when sandbox is disabled', () => {
    expect(resolveVisibleWorkspaceTabs(false, false)).toEqual([
      'files',
      'team',
      'plan',
      'settings',
    ]);
  });
});
