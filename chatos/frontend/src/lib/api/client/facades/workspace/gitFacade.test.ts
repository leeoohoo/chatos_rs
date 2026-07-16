// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { workspaceGitFacade } from './gitFacade';

describe('workspaceGitFacade local routing', () => {
  it('keeps local Git reads and writes inside the desktop runtime', async () => {
    const getGitDiff = vi.fn().mockResolvedValue({ patch: '' });
    const commitGit = vi.fn().mockResolvedValue({ success: true });
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud Git request must not run');
    });
    const context = {
      getLocalRuntimeClient: () => ({ getGitDiff, commitGit }),
      getRequestFn: () => cloudRequest,
    };
    const root = 'local://connector/device/workspace/project';

    await workspaceGitFacade.getGitDiff.call(context as never, {
      root,
      path: 'README.md',
    });
    await workspaceGitFacade.commitGit.call(context as never, {
      root,
      message: 'local commit',
    });

    expect(getGitDiff).toHaveBeenCalledWith({ root, path: 'README.md' });
    expect(commitGit).toHaveBeenCalledWith({ root, message: 'local commit' });
    expect(cloudRequest).not.toHaveBeenCalled();
  });
});
