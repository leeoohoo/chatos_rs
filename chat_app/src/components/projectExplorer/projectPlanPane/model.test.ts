import { describe, expect, it } from 'vitest';

import {
  buildDownstreamRequirementScope,
  buildDependencyMaps,
  buildVisiblePlanItems,
  canShowRequirementExecutionAction,
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

  it('builds downstream requirement scope without walking to parents or prerequisites', () => {
    const requirements = [
      { id: 'parent', title: 'Parent' },
      { id: 'child', title: 'Child', parent_requirement_id: 'parent' },
      { id: 'grandchild', title: 'Grandchild', parent_requirement_id: 'child' },
      { id: 'dependent', title: 'Dependent' },
      { id: 'dependent-child', title: 'Dependent Child', parent_requirement_id: 'dependent' },
      { id: 'after-dependent', title: 'After Dependent' },
      { id: 'sibling', title: 'Sibling', parent_requirement_id: 'parent' },
      { id: 'prerequisite', title: 'Prerequisite' },
    ];
    const dependencyMaps = buildDependencyMaps({
      dependencyGraph: {
        edges: [
          { from: 'requirement:prerequisite', to: 'requirement:child', edge_type: 'blocks' },
          { from: 'requirement:child', to: 'requirement:dependent', edge_type: 'blocks' },
          { from: 'requirement:dependent', to: 'requirement:after-dependent', edge_type: 'blocks' },
        ],
      },
    });

    const scope = buildDownstreamRequirementScope({
      dependencyMaps,
      requirements,
      rootId: 'child',
    });

    expect(scope).toEqual([
      'child',
      'grandchild',
      'dependent',
      'dependent-child',
      'after-dependent',
    ]);
  });

  it('hides requirement execution action for terminal statuses', () => {
    expect(canShowRequirementExecutionAction('done')).toBe(false);
    expect(canShowRequirementExecutionAction('cancelled')).toBe(false);
    expect(canShowRequirementExecutionAction('archived')).toBe(false);
    expect(canShowRequirementExecutionAction('in_progress')).toBe(true);
    expect(canShowRequirementExecutionAction('approved')).toBe(true);
  });
});
