// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { workspaceFilesystemFacade } from './filesystemFacade';

describe('workspaceFilesystemFacade local routing', () => {
  it('loads local project files directly through the desktop runtime', async () => {
    const listFsEntries = vi.fn().mockResolvedValue({ entries: [] });
    const readFsFile = vi.fn().mockResolvedValue({ content: 'hello' });
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud filesystem request must not run');
    });
    const context = {
      getLocalRuntimeClient: () => ({ listFsEntries, readFsFile }),
      getRequestFn: () => cloudRequest,
    };
    const root = 'local://connector/device/workspace/project';

    await workspaceFilesystemFacade.listFsEntries.call(context as never, root);
    await workspaceFilesystemFacade.readFsFile.call(context as never, `${root}/README.md`);

    expect(listFsEntries).toHaveBeenCalledWith(root);
    expect(readFsFile).toHaveBeenCalledWith(`${root}/README.md`);
    expect(cloudRequest).not.toHaveBeenCalled();
  });
});
