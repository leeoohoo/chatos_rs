// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { resolveVisibleWorkspaceTabs } from './workspaceTabsModel';

describe('resolveVisibleWorkspaceTabs', () => {
  it('uses the sandbox runtime tab instead of project settings for cloud projects', () => {
    expect(resolveVisibleWorkspaceTabs(true)).toEqual([
      'files',
      'team',
      'plan',
      'sandbox',
    ]);
  });

  it('keeps sandbox runtime out of local project navigation', () => {
    expect(resolveVisibleWorkspaceTabs(false)).toEqual([
      'files',
      'team',
      'plan',
      'settings',
    ]);
  });

});
