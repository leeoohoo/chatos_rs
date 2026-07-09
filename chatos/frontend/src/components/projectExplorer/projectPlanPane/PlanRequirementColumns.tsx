// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { ChevronRight, GitBranch } from 'lucide-react';

import type {
  ProjectRequirementResponse,
  ProjectWorkItemResponse,
} from '../../../lib/api/client/types';
import { cn } from '../../../lib/utils';
import { PlanStatsBar } from './components';
import {
  type DependencyMaps,
  type RequirementColumn,
  REQUIREMENT_COLUMN_WIDTH,
  priorityLabel,
  readText,
  requirementTypeLabel,
  statusClassName,
  statusLabel,
} from './model';

const relationPreviewText = (
  ids: string[],
  resolveLabel: (id: string) => string,
  limit = 2,
): string => {
  const visible = ids.slice(0, limit).map(resolveLabel);
  const hiddenCount = ids.length - visible.length;
  return hiddenCount > 0
    ? `${visible.join('、')} 等 ${ids.length} 个`
    : visible.join('、');
};

export const PlanRequirementColumns: React.FC<{
  blockedWorkItemCount: number;
  dependencyMaps: DependencyMaps;
  doneWorkItemCount: number;
  onSelectRequirement: (requirementId: string) => void;
  requirementChildrenMap: Map<string, ProjectRequirementResponse[]>;
  requirementColumns: RequirementColumn[];
  requirementCount: number;
  requirementPath: string[];
  resolveRequirementTitle: (id: string) => string;
  selectedRequirementId: string | null;
  workItemsByRequirement: Map<string, ProjectWorkItemResponse[]>;
}> = ({
  blockedWorkItemCount,
  dependencyMaps,
  doneWorkItemCount,
  onSelectRequirement,
  requirementChildrenMap,
  requirementColumns,
  requirementCount,
  requirementPath,
  resolveRequirementTitle,
  selectedRequirementId,
  workItemsByRequirement,
}) => (
  <aside className="flex min-h-0 flex-col border-r border-border bg-muted/10">
    <PlanStatsBar
      blockedWorkItemCount={blockedWorkItemCount}
      doneWorkItemCount={doneWorkItemCount}
      requirementCount={requirementCount}
    />
    <div className="min-h-0 flex-1 overflow-x-auto overflow-y-hidden">
      <div className="flex h-full min-w-max">
        {requirementColumns.map((column, columnIndex) => (
          <section
            key={column.id}
            className="flex h-full shrink-0 flex-col border-r border-border/80 bg-background"
            style={{ width: REQUIREMENT_COLUMN_WIDTH }}
          >
            <div className="flex h-10 shrink-0 items-center gap-1.5 border-b border-border/80 px-3">
              {columnIndex === 0 ? (
                <GitBranch className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              ) : null}
              <span className="min-w-0 truncate text-xs font-semibold text-foreground">
                {column.title}
              </span>
              <span className="ml-auto rounded-full border border-border bg-muted/20 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {column.items.length}
              </span>
            </div>
            <div className="min-h-0 flex-1 space-y-1 overflow-y-auto p-2">
              {column.items.map((requirement) => {
                const tasks = workItemsByRequirement.get(requirement.id);
                const prerequisiteIds = dependencyMaps.requirementPrerequisites.get(requirement.id) || [];
                const dependentIds = dependencyMaps.requirementDependents.get(requirement.id) || [];
                const children = requirementChildrenMap.get(requirement.id) || [];
                const active = requirement.id === selectedRequirementId;
                const inPath = requirementPath.includes(requirement.id);
                return (
                  <button
                    key={requirement.id}
                    type="button"
                    className={cn(
                      'w-full rounded-md border px-2.5 py-2 text-left transition-colors',
                      active
                        ? 'border-primary/40 bg-primary/10 shadow-sm'
                        : inPath
                          ? 'border-primary/20 bg-accent/70'
                          : 'border-transparent hover:border-border hover:bg-accent/50',
                    )}
                    onClick={() => onSelectRequirement(requirement.id)}
                  >
                    <div className="flex items-center gap-1.5">
                      <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
                        {requirement.title || requirement.id}
                      </span>
                      <span className="shrink-0 rounded-full border border-border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground">
                        {tasks ? tasks.length : '-'}
                      </span>
                      {children.length > 0 ? (
                        <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                      ) : null}
                    </div>
                    <div className="mt-1.5 flex flex-wrap gap-1.5">
                      <span className={cn(
                        'rounded-full border px-1.5 py-0.5 text-[10px] font-medium',
                        statusClassName(requirement.status),
                      )}
                      >
                        {statusLabel(requirement.status)}
                      </span>
                      <span className="rounded-full border border-border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground">
                        {requirementTypeLabel(requirement.requirement_type || requirement.requirementType)}
                      </span>
                      <span className="rounded-full border border-border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground">
                        {priorityLabel(requirement.priority)}
                      </span>
                      {prerequisiteIds.length > 0 ? (
                        <span className="rounded-full border border-amber-200 bg-amber-50 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300">
                          前置 {prerequisiteIds.length}
                        </span>
                      ) : null}
                      {dependentIds.length > 0 ? (
                        <span className="rounded-full border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-700 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-300">
                          后续 {dependentIds.length}
                        </span>
                      ) : null}
                      {children.length > 0 ? (
                        <span className="rounded-full border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-700 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-300">
                          子需求 {children.length}
                        </span>
                      ) : null}
                    </div>
                    {prerequisiteIds.length > 0 ? (
                      <div className="mt-1.5 min-w-0 text-[11px] leading-4 text-amber-700 dark:text-amber-300">
                        <span className="font-medium">前置：</span>
                        <span className="break-words">
                          {relationPreviewText(prerequisiteIds, resolveRequirementTitle)}
                        </span>
                      </div>
                    ) : null}
                    {dependentIds.length > 0 ? (
                      <div className="mt-1 min-w-0 text-[11px] leading-4 text-blue-700 dark:text-blue-300">
                        <span className="font-medium">后续：</span>
                        <span className="break-words">
                          {relationPreviewText(dependentIds, resolveRequirementTitle)}
                        </span>
                      </div>
                    ) : null}
                    {readText(requirement.summary) ? (
                      <div className="mt-1 line-clamp-2 text-xs leading-4 text-muted-foreground">
                        {requirement.summary}
                      </div>
                    ) : null}
                  </button>
                );
              })}
            </div>
          </section>
        ))}
      </div>
    </div>
  </aside>
);
