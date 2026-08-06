// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { resolveGitSummaryViewState } from './GitBranchButtonViews';

describe('resolveGitSummaryViewState', () => {
  it('does not report a missing summary request as a non-Git repository', () => {
    expect(resolveGitSummaryViewState(null, 'Tool not found: list_branches')).toBe('error');
  });

  it('reports non-repository state only from a successful summary', () => {
    expect(resolveGitSummaryViewState({ isRepo: false }, null)).toBe('not-repo');
  });

  it('keeps the initial request in a loading state', () => {
    expect(resolveGitSummaryViewState(null, null)).toBe('loading');
  });
});
