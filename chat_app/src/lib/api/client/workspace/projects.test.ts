import { describe, expect, it, vi } from 'vitest';

import { getProjectContactLock } from './projects';

describe('workspace project api helpers', () => {
  it('loads the project contact lock state from the Chatos project endpoint', async () => {
    const request = vi.fn().mockResolvedValue({ locked: false });

    await getProjectContactLock(request as never, 'project with spaces');

    expect(request).toHaveBeenCalledTimes(1);
    expect(request).toHaveBeenCalledWith('/projects/project%20with%20spaces/contacts/lock');
  });
});
