// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  AlertCircle,
  ArrowRight,
  CheckCircle2,
  ClipboardList,
  Eye,
  FileText,
  Link2,
  ListChecks,
  Play,
  Square,
} from 'lucide-react';

import type {
  ProjectRequirementDocumentResponse,
  ProjectRequirementResponse,
  ProjectWorkItemResponse,
} from '../../../lib/api/client/types';
import { cn } from '../../../lib/utils';
import {
  DependencyLine,
  RequirementContentSection,
  TechnicalDocumentsSection,
  WorkItemRow,
} from './components';
import {
  type DependencyMaps,
  type VisiblePlanItems,
  SELECTED_WORK_ITEM_RENDER_INCREMENT,
  countOpenItems,
  formatDateTime,
  getUpdatedAt,
  priorityLabel,
  readText,
  requirementTypeLabel,
  statusClassName,
  statusLabel,
} from './model';

export type DetailTabId = 'requirement' | 'documents' | 'tasks';

export const PlanRequirementDetail: React.FC<{
  activeDetailTab: DetailTabId;
  actionDisabled: boolean;
  dependencyMaps: DependencyMaps;
  onActiveDetailTabChange: (tab: DetailTabId) => void;
  onLoadMoreWorkItems: () => void;
  onPreviewRequirement: (requirement: ProjectRequirementResponse, canConfirm: boolean) => void;
  onStopRequirementExecution: (requirement: ProjectRequirementResponse) => void;
  resolveRequirementTitle: (id: string) => string;
  resolveWorkItemTitle: (id: string) => string;
  selectedDocumentsLoading: boolean;
  selectedExecutionScopeRelatedIds: string[];
  selectedRequirement: ProjectRequirementResponse | null;
  selectedRequirementActionBusy: boolean;
  selectedRequirementCanShowAction: boolean;
  selectedRequirementChildren: ProjectRequirementResponse[];
  selectedRequirementDependents: string[];
  selectedRequirementDocuments: ProjectRequirementDocumentResponse[];
  selectedRequirementIsExecuting: boolean;
  selectedRequirementPrerequisites: string[];
  selectedWorkItems: ProjectWorkItemResponse[];
  selectedWorkItemsLoading: boolean;
  visibleSelectedWorkItems: VisiblePlanItems<ProjectWorkItemResponse>;
}> = ({
  activeDetailTab,
  actionDisabled,
  dependencyMaps,
  onActiveDetailTabChange,
  onLoadMoreWorkItems,
  onPreviewRequirement,
  onStopRequirementExecution,
  resolveRequirementTitle,
  resolveWorkItemTitle,
  selectedDocumentsLoading,
  selectedExecutionScopeRelatedIds,
  selectedRequirement,
  selectedRequirementActionBusy,
  selectedRequirementCanShowAction,
  selectedRequirementChildren,
  selectedRequirementDependents,
  selectedRequirementDocuments,
  selectedRequirementIsExecuting,
  selectedRequirementPrerequisites,
  selectedWorkItems,
  selectedWorkItemsLoading,
  visibleSelectedWorkItems,
}) => (
  <main className="min-h-0 overflow-y-auto px-5 py-4">
    {selectedRequirement ? (
      <div className="mx-auto max-w-5xl">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2 text-xs">
              <span className={cn(
                'rounded-full border px-2 py-0.5 font-medium',
                statusClassName(selectedRequirement.status),
              )}
              >
                {statusLabel(selectedRequirement.status)}
              </span>
              <span className="rounded-full border border-border bg-muted/20 px-2 py-0.5 text-muted-foreground">
                {requirementTypeLabel(selectedRequirement.requirement_type || selectedRequirement.requirementType)}
              </span>
              <span className="rounded-full border border-border bg-muted/20 px-2 py-0.5 text-muted-foreground">
                {priorityLabel(selectedRequirement.priority)}
              </span>
            </div>
            <h3 className="mt-2 break-words text-lg font-semibold leading-7 text-foreground">
              {selectedRequirement.title || selectedRequirement.id}
            </h3>
            <div className="mt-1 text-xs text-muted-foreground">
              更新于 {formatDateTime(getUpdatedAt(selectedRequirement))}
            </div>
          </div>
          <div className="flex shrink-0 flex-wrap items-center gap-2">
            <button
              type="button"
              className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-xs font-medium text-muted-foreground shadow-sm hover:bg-accent hover:text-foreground"
              onClick={() => onPreviewRequirement(selectedRequirement, false)}
            >
              <Eye className="h-3.5 w-3.5" />
              预览流程
            </button>
            {selectedRequirementCanShowAction ? (
              <button
                type="button"
                className={cn(
                  'inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs font-medium shadow-sm disabled:cursor-not-allowed disabled:border-border disabled:bg-muted disabled:text-muted-foreground disabled:shadow-none',
                  selectedRequirementIsExecuting
                    ? 'border-destructive/40 bg-destructive text-destructive-foreground hover:bg-destructive/90'
                    : 'border-primary/40 bg-primary text-primary-foreground hover:bg-primary/90',
                )}
                disabled={actionDisabled}
                onClick={() => {
                  if (selectedRequirementIsExecuting) {
                    onStopRequirementExecution(selectedRequirement);
                  } else {
                    onPreviewRequirement(selectedRequirement, true);
                  }
                }}
              >
                {selectedRequirementIsExecuting ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
                {selectedRequirementActionBusy
                  ? (selectedRequirementIsExecuting ? '停止中' : '执行中')
                  : (selectedRequirementIsExecuting ? '停止' : '执行关联任务')}
              </button>
            ) : null}
          </div>
        </div>

        <div className="mt-4 border-b border-border">
          <div className="flex gap-1 overflow-x-auto" role="tablist" aria-label="需求详情">
            <button
              type="button"
              role="tab"
              aria-selected={activeDetailTab === 'requirement'}
              className={cn(
                'inline-flex h-9 shrink-0 items-center gap-1.5 border-b-2 px-3 text-xs font-medium transition-colors',
                activeDetailTab === 'requirement'
                  ? 'border-primary text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground',
              )}
              onClick={() => onActiveDetailTabChange('requirement')}
            >
              <ClipboardList className="h-3.5 w-3.5" />
              需求
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={activeDetailTab === 'documents'}
              className={cn(
                'inline-flex h-9 shrink-0 items-center gap-1.5 border-b-2 px-3 text-xs font-medium transition-colors',
                activeDetailTab === 'documents'
                  ? 'border-primary text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground',
              )}
              onClick={() => onActiveDetailTabChange('documents')}
            >
              <FileText className="h-3.5 w-3.5" />
              技术文档
              <span className="rounded-full border border-border bg-muted/20 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {selectedDocumentsLoading ? '...' : selectedRequirementDocuments.length}
              </span>
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={activeDetailTab === 'tasks'}
              className={cn(
                'inline-flex h-9 shrink-0 items-center gap-1.5 border-b-2 px-3 text-xs font-medium transition-colors',
                activeDetailTab === 'tasks'
                  ? 'border-primary text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground',
              )}
              onClick={() => onActiveDetailTabChange('tasks')}
            >
              <ListChecks className="h-3.5 w-3.5" />
              任务
              <span className="rounded-full border border-border bg-muted/20 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {selectedWorkItemsLoading ? '...' : selectedWorkItems.length}
              </span>
            </button>
          </div>
        </div>

        <div className="pt-4">
          {activeDetailTab === 'requirement' ? (
            <div role="tabpanel">
              <div className="rounded-md border border-border bg-muted/10 px-3 py-3">
                <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-foreground">
                  <Link2 className="h-3.5 w-3.5 text-muted-foreground" />
                  需求关系
                </div>
                <div className="space-y-1.5">
                  <DependencyLine
                    ids={selectedRequirementPrerequisites}
                    label="前置需求"
                    resolveLabel={resolveRequirementTitle}
                  />
                  {selectedRequirementDependents.length > 0 ? (
                    <DependencyLine
                      ids={selectedRequirementDependents}
                      label="后续需求"
                      resolveLabel={resolveRequirementTitle}
                      tone="dependent"
                    />
                  ) : null}
                  {selectedRequirementChildren.length > 0 ? (
                    <DependencyLine
                      ids={selectedRequirementChildren.map((requirement) => requirement.id)}
                      label="子需求"
                      resolveLabel={resolveRequirementTitle}
                      tone="dependent"
                    />
                  ) : null}
                  <DependencyLine
                    emptyLabel="仅当前需求"
                    ids={selectedExecutionScopeRelatedIds}
                    label="执行会包含"
                    resolveLabel={resolveRequirementTitle}
                    tone="dependent"
                  />
                </div>
              </div>

              <div className="mt-4">
                <RequirementContentSection title="摘要" content={selectedRequirement.summary} />
                <RequirementContentSection title="详细说明" content={selectedRequirement.detail} />
                <RequirementContentSection title="业务价值" content={selectedRequirement.business_value || selectedRequirement.businessValue} />
                <RequirementContentSection title="验收标准" content={selectedRequirement.acceptance_criteria || selectedRequirement.acceptanceCriteria} />
                {!readText(selectedRequirement.summary)
                  && !readText(selectedRequirement.detail)
                  && !readText(selectedRequirement.business_value || selectedRequirement.businessValue)
                  && !readText(selectedRequirement.acceptance_criteria || selectedRequirement.acceptanceCriteria) ? (
                    <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
                      这个需求还没有补充内容。
                    </div>
                  ) : null}
              </div>
            </div>
          ) : null}

          {activeDetailTab === 'documents' ? (
            <div role="tabpanel">
              <TechnicalDocumentsSection
                className="mt-0 border-t-0 pt-0"
                documents={selectedRequirementDocuments}
                loading={selectedDocumentsLoading}
              />
            </div>
          ) : null}

          {activeDetailTab === 'tasks' ? (
            <section role="tabpanel">
              <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                <div>
                  <h4 className="text-sm font-semibold text-foreground">项目任务</h4>
                  <div className="mt-0.5 text-xs text-muted-foreground">
                    {selectedWorkItemsLoading
                      ? '正在加载项目任务...'
                      : `${selectedWorkItems.length} 个项目任务 · ${countOpenItems(selectedWorkItems)} 个未完成`}
                  </div>
                </div>
                {selectedWorkItems.length > 0 && countOpenItems(selectedWorkItems) === 0 ? (
                  <span className="inline-flex items-center gap-1 rounded-full border border-emerald-200 bg-emerald-50 px-2 py-0.5 text-xs font-medium text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-300">
                    <CheckCircle2 className="h-3.5 w-3.5" />
                    已全部完成
                  </span>
                ) : null}
              </div>
              {selectedWorkItems.length > 0 ? (
                <div className="mb-3 flex items-start gap-2 rounded-md border border-border bg-muted/10 px-3 py-2 text-xs text-muted-foreground">
                  <ArrowRight className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                  <span>项目任务已按前置关系尽量排序；“前置项目任务”是当前项目任务开始前需要先完成的任务。</span>
                </div>
              ) : null}
              {selectedWorkItemsLoading ? (
                <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
                  正在加载项目任务...
                </div>
              ) : selectedWorkItems.length === 0 ? (
                <div className="rounded-md border border-amber-200 bg-amber-50 px-3 py-3 text-sm text-amber-800 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200">
                  <div className="flex items-center gap-2 font-medium">
                    <AlertCircle className="h-4 w-4" />
                    这个需求下面还没有任务
                  </div>
                </div>
              ) : (
                <div className="space-y-2">
                  {visibleSelectedWorkItems.items.map((item) => (
                    <WorkItemRow
                      key={item.id}
                      item={item}
                      prerequisites={dependencyMaps.workItemPrerequisites.get(item.id) || []}
                      dependents={dependencyMaps.workItemDependents.get(item.id) || []}
                      resolveWorkItemTitle={resolveWorkItemTitle}
                    />
                  ))}
                  {visibleSelectedWorkItems.hasMore ? (
                    <button
                      type="button"
                      className="w-full rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-foreground"
                      onClick={onLoadMoreWorkItems}
                    >
                      加载更多 {Math.min(
                        SELECTED_WORK_ITEM_RENDER_INCREMENT,
                        visibleSelectedWorkItems.hiddenCount,
                      )} / {visibleSelectedWorkItems.hiddenCount}
                    </button>
                  ) : null}
                </div>
              )}
            </section>
          ) : null}
        </div>
      </div>
    ) : null}
  </main>
);
