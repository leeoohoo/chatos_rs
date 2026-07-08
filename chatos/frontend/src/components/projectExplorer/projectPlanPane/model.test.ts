// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import {
  buildDependencyMaps,
  buildRequirementExecutionScope,
  buildVisiblePlanItems,
  canShowRequirementExecutionAction,
  mergeDependencyMaps,
  statusClassName,
  statusLabel,
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

  it('builds execution scope with downstream requirements and required prerequisites', () => {
    const requirements = [
      { id: 'parent', title: 'Parent' },
      { id: 'child', title: 'Child', parent_requirement_id: 'parent' },
      { id: 'grandchild', title: 'Grandchild', parent_requirement_id: 'child' },
      { id: 'dependent', title: 'Dependent' },
      { id: 'dependent-child', title: 'Dependent Child', parent_requirement_id: 'dependent' },
      { id: 'after-dependent', title: 'After Dependent' },
      { id: 'sibling', title: 'Sibling', parent_requirement_id: 'parent' },
      { id: 'prerequisite', title: 'Prerequisite' },
      { id: 'unrelated-dependent', title: 'Unrelated Dependent' },
    ];
    const dependencyMaps = buildDependencyMaps({
      dependencyGraph: {
        edges: [
          { from: 'requirement:child', to: 'requirement:dependent', edge_type: 'blocks' },
          { from: 'requirement:prerequisite', to: 'requirement:dependent', edge_type: 'blocks' },
          { from: 'requirement:dependent', to: 'requirement:after-dependent', edge_type: 'blocks' },
          { from: 'requirement:prerequisite', to: 'requirement:unrelated-dependent', edge_type: 'blocks' },
        ],
      },
    });

    const scope = buildRequirementExecutionScope({
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
      'prerequisite',
    ]);
  });

  it('keeps completed external prerequisites out of execution scope', () => {
    const requirements = [
      { id: 'root', title: 'Root' },
      { id: 'dependent', title: 'Dependent' },
      { id: 'completed-prerequisite', title: 'Completed Prerequisite', status: 'done' as const },
    ];
    const dependencyMaps = buildDependencyMaps({
      dependencyGraph: {
        edges: [
          { from: 'requirement:root', to: 'requirement:dependent', edge_type: 'blocks' },
          { from: 'requirement:completed-prerequisite', to: 'requirement:dependent', edge_type: 'blocks' },
        ],
      },
    });

    const scope = buildRequirementExecutionScope({
      dependencyMaps,
      requirements,
      rootId: 'root',
    });

    expect(scope).toEqual(['root', 'dependent']);
  });

  it('can include dependents unlocked by required prerequisites', () => {
    const requirements = [
      { id: 'root', title: 'Root' },
      { id: 'dependent', title: 'Dependent' },
      { id: 'prerequisite', title: 'Prerequisite' },
      { id: 'prerequisite-dependent', title: 'Prerequisite Dependent' },
    ];
    const dependencyMaps = buildDependencyMaps({
      dependencyGraph: {
        edges: [
          { from: 'requirement:root', to: 'requirement:dependent', edge_type: 'blocks' },
          { from: 'requirement:prerequisite', to: 'requirement:dependent', edge_type: 'blocks' },
          { from: 'requirement:prerequisite', to: 'requirement:prerequisite-dependent', edge_type: 'blocks' },
        ],
      },
    });

    const scope = buildRequirementExecutionScope({
      dependencyMaps,
      includePrerequisiteDependents: true,
      requirements,
      rootId: 'root',
    });

    expect(scope).toEqual(['root', 'dependent', 'prerequisite', 'prerequisite-dependent']);
  });

  it('hides requirement execution action for terminal statuses', () => {
    expect(canShowRequirementExecutionAction('done')).toBe(false);
    expect(canShowRequirementExecutionAction('cancelled')).toBe(false);
    expect(canShowRequirementExecutionAction('archived')).toBe(false);
    expect(canShowRequirementExecutionAction('in_progress')).toBe(true);
    expect(canShowRequirementExecutionAction('approved')).toBe(true);
  });

  it('renders failed statuses without falling back to draft', () => {
    expect(statusLabel('failed')).toBe('失败');
    expect(statusLabel('FAILED')).toBe('失败');
    expect(statusLabel('unexpected_status')).toBe('unexpected_status');
    expect(statusClassName('failed')).toContain('text-destructive');
  });
});
