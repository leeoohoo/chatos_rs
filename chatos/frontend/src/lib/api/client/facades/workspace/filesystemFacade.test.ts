// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { workspaceFilesystemFacade } from './filesystemFacade';

describe('workspaceFilesystemFacade local routing', () => {
  it('loads local connector project files through the cloud filesystem gateway', async () => {
    const cloudRequest = vi.fn()
      .mockResolvedValueOnce({ entries: [] })
      .mockResolvedValueOnce({ content: 'hello' });
    const context = {
      getRequestFn: () => cloudRequest,
    };
    const root = 'local://connector/device/workspace/project';

    await workspaceFilesystemFacade.listFsEntries.call(context as never, root);
    await workspaceFilesystemFacade.readFsFile.call(context as never, `${root}/README.md`);

    expect(cloudRequest).toHaveBeenNthCalledWith(
      1,
      '/fs/entries?path=local%3A%2F%2Fconnector%2Fdevice%2Fworkspace%2Fproject',
    );
    expect(cloudRequest).toHaveBeenNthCalledWith(
      2,
      '/fs/read?path=local%3A%2F%2Fconnector%2Fdevice%2Fworkspace%2Fproject%2FREADME.md',
    );
  });
});
