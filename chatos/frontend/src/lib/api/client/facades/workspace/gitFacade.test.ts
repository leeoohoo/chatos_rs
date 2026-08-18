// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { workspaceGitFacade } from './gitFacade';

describe('workspaceGitFacade local routing', () => {
  it('routes local connector Git reads and writes through the cloud gateway', async () => {
    const cloudRequest = vi.fn()
      .mockResolvedValueOnce({ patch: '' })
      .mockResolvedValueOnce({ success: true });
    const context = {
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

    expect(cloudRequest).toHaveBeenNthCalledWith(
      1,
      '/git/diff?root=local%3A%2F%2Fconnector%2Fdevice%2Fworkspace%2Fproject&path=README.md',
    );
    expect(cloudRequest).toHaveBeenNthCalledWith(2, '/git/commit', {
      method: 'POST',
      body: JSON.stringify({
        root,
        message: 'local commit',
        paths: undefined,
      }),
    });
  });
});
