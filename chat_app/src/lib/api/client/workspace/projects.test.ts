import { describe, expect, it, vi } from 'vitest';

import {
  getProjectContactLock,
  getProjectPlan,
  listProjectRequirementWorkItems,
} from './projects';

describe('workspace project api helpers', () => {
  it('loads the project contact lock state from the Chatos project endpoint', async () => {
    const request = vi.fn().mockResolvedValue({ locked: false });

    await getProjectContactLock(request as never, 'project with spaces');

    expect(request).toHaveBeenCalledTimes(1);
    expect(request).toHaveBeenCalledWith('/projects/project%20with%20spaces/contacts/lock');
  });

  it('passes lightweight plan options as query parameters', async () => {
    const request = vi.fn().mockResolvedValue({});

    await getProjectPlan(request as never, 'project 1', { includeWorkItems: false });

    expect(request).toHaveBeenCalledWith('/projects/project%201/plan?include_work_items=false');
  });

  it('loads requirement work items through the project-scoped endpoint', async () => {
    const request = vi.fn().mockResolvedValue({});

    await listProjectRequirementWorkItems(request as never, 'project 1', 'req/1', {
      includeDependencyGraph: true,
    });

    expect(request).toHaveBeenCalledWith(
      '/projects/project%201/requirements/req%2F1/work-items?include_dependency_graph=true',
    );
  });
});
