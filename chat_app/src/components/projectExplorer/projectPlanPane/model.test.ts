import { describe, expect, it } from 'vitest';

import {
  buildDependencyMaps,
  buildVisiblePlanItems,
  mergeDependencyMaps,
} from './model';

describe('projectPlanPane model', () => {
  it('limits visible plan items and reports hidden count', () => {
    const result = buildVisiblePlanItems([1, 2, 3, 4], 2);

    expect(result.items).toEqual([1, 2]);
    expect(result.hiddenCount).toBe(2);
    expect(result.totalCount).toBe(4);
    expect(result.hasMore).toBe(true);
  });

  it('normalizes invalid visible limits to at least one item', () => {
    const result = buildVisiblePlanItems(['a', 'b'], 0);

    expect(result.items).toEqual(['a']);
    expect(result.hiddenCount).toBe(1);
  });

  it('merges requirement and work item dependency maps', () => {
    const requirementMaps = buildDependencyMaps({
      dependencyGraph: {
        edges: [
          { from: 'requirement:req-a', to: 'requirement:req-b', edge_type: 'blocks' },
        ],
      },
    });
    const workItemMaps = buildDependencyMaps({
      dependencyGraph: {
        edges: [
          { from: 'work_item:item-a', to: 'work_item:item-b', edge_type: 'blocks' },
        ],
      },
    });

    const merged = mergeDependencyMaps(requirementMaps, workItemMaps);

    expect(merged.requirementPrerequisites.get('req-b')).toEqual(['req-a']);
    expect(merged.workItemPrerequisites.get('item-b')).toEqual(['item-a']);
  });
});
