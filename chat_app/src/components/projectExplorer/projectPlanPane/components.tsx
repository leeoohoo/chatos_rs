import React from 'react';
import { ClipboardList, RefreshCw } from 'lucide-react';

import type { ProjectWorkItemResponse } from '../../../lib/api/client/types';
import { cn } from '../../../lib/utils';
import { LazyMarkdownRenderer } from '../../LazyMarkdownRenderer';
import {
  formatDateTime,
  priorityLabel,
  readText,
  statusClassName,
  statusLabel,
} from './model';

const DEPENDENCY_PILL_RENDER_LIMIT = 16;

export const PlanPaneHeader: React.FC<{
  loading: boolean;
  onRefresh: () => void;
  openItemCount: number;
  requirementCount: number;
  workItemCount: number;
}> = ({
  loading,
  onRefresh,
  openItemCount,
  requirementCount,
  workItemCount,
}) => (
  <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
    <div className="min-w-0">
      <h2 className="text-sm font-semibold text-foreground">Plan</h2>
      <p className="mt-0.5 truncate text-xs text-muted-foreground">
        {requirementCount} 个需求 · {workItemCount} 个项目任务 · {openItemCount} 个未完成
      </p>
    </div>
    <button
      type="button"
      className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-2.5 py-1.5 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
      disabled={loading}
      onClick={onRefresh}
    >
      <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} />
      刷新
    </button>
  </div>
);

export const PlanLoadingState: React.FC = () => (
  <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
    正在加载 Plan...
  </div>
);

export const PlanEmptyState: React.FC = () => (
  <div className="flex flex-1 items-center justify-center px-4 text-center">
    <div>
      <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
        <ClipboardList className="h-5 w-5" />
      </div>
      <div className="mt-3 text-sm font-medium text-foreground">暂无需求</div>
      <div className="mt-1 text-xs text-muted-foreground">
        规划任务写入 Project Management 后，需求和项目任务会显示在这里。
      </div>
    </div>
  </div>
);

export const PlanStatsBar: React.FC<{
  blockedWorkItemCount: number;
  doneWorkItemCount: number;
  requirementCount: number;
}> = ({ blockedWorkItemCount, doneWorkItemCount, requirementCount }) => (
  <div className="shrink-0 border-b border-border bg-background/95 px-3 py-2">
    <div className="grid grid-cols-3 gap-2">
      <div className="rounded-md border border-border bg-background px-2 py-1.5">
        <div className="text-[10px] text-muted-foreground">需求</div>
        <div className="text-sm font-semibold text-foreground">{requirementCount}</div>
      </div>
      <div className="rounded-md border border-border bg-background px-2 py-1.5">
        <div className="text-[10px] text-muted-foreground">完成</div>
        <div className="text-sm font-semibold text-foreground">{doneWorkItemCount}</div>
      </div>
      <div className="rounded-md border border-border bg-background px-2 py-1.5">
        <div className="text-[10px] text-muted-foreground">阻塞</div>
        <div className="text-sm font-semibold text-foreground">{blockedWorkItemCount}</div>
      </div>
    </div>
  </div>
);

export const PlanBannerMessages: React.FC<{
  error: string | null;
  executionMessage: string | null;
}> = ({ error, executionMessage }) => (
  <>
    {error ? (
      <div className="border-b border-destructive/30 bg-destructive/10 px-4 py-2 text-sm text-destructive">
        {error}
      </div>
    ) : null}
    {executionMessage ? (
      <div className="border-b border-emerald-200 bg-emerald-50 px-4 py-2 text-sm text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-300">
        {executionMessage}
      </div>
    ) : null}
  </>
);

export const RequirementContentSection: React.FC<{
  title: string;
  content?: string | null;
}> = ({ title, content }) => {
  const text = readText(content);
  if (!text) {
    return null;
  }
  return (
    <section className="border-t border-border/70 py-3">
      <h4 className="text-xs font-semibold text-muted-foreground">{title}</h4>
      <div className="mt-2 rounded-md border border-border/70 bg-muted/10 px-3 py-2">
        <LazyMarkdownRenderer content={text} className="text-sm" />
      </div>
    </section>
  );
};

const DependencyPill: React.FC<{
  children: React.ReactNode;
  tone?: 'dependency' | 'dependent';
}> = ({ children, tone = 'dependency' }) => (
  <span className={cn(
    'inline-flex min-w-0 items-center rounded border px-1.5 py-0.5',
    tone === 'dependent'
      ? 'border-blue-200 bg-blue-50 text-blue-700 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-300'
      : 'border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300',
  )}
  >
    <span className="truncate">{children}</span>
  </span>
);

export const DependencyLine: React.FC<{
  emptyLabel?: string;
  ids: string[];
  label: string;
  resolveLabel: (id: string) => string;
  tone?: 'dependency' | 'dependent';
}> = ({
  emptyLabel = '无',
  ids,
  label,
  resolveLabel,
  tone = 'dependency',
}) => {
  const visibleIds = ids.length > DEPENDENCY_PILL_RENDER_LIMIT
    ? ids.slice(0, DEPENDENCY_PILL_RENDER_LIMIT)
    : ids;
  const hiddenCount = ids.length - visibleIds.length;

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-[11px]">
      <span className="shrink-0 font-medium text-muted-foreground">{label}</span>
      {ids.length === 0 ? (
        <span className="rounded border border-border/70 bg-muted/20 px-1.5 py-0.5 text-muted-foreground">
          {emptyLabel}
        </span>
      ) : (
        <>
          {visibleIds.map((id) => (
            <DependencyPill key={id} tone={tone}>
              {resolveLabel(id)}
            </DependencyPill>
          ))}
          {hiddenCount > 0 ? (
            <DependencyPill tone={tone}>
              +{hiddenCount}
            </DependencyPill>
          ) : null}
        </>
      )}
    </div>
  );
};

export const WorkItemRow: React.FC<{
  dependents: string[];
  item: ProjectWorkItemResponse;
  prerequisites: string[];
  resolveWorkItemTitle: (id: string) => string;
}> = ({
  dependents,
  item,
  prerequisites,
  resolveWorkItemTitle,
}) => (
  <article className="rounded-md border border-border bg-background px-3 py-2">
    <div className="flex flex-wrap items-start justify-between gap-2">
      <div className="min-w-0">
        <div className="break-words text-sm font-medium text-foreground">
          {item.title || item.id}
        </div>
        {readText(item.description) ? (
          <div className="mt-1 line-clamp-3 text-xs leading-5 text-muted-foreground">
            {item.description}
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <span className={cn(
          'rounded-full border px-2 py-0.5 text-[11px] font-medium',
          statusClassName(item.status),
        )}
        >
          {statusLabel(item.status)}
        </span>
        <span className="rounded-full border border-border bg-muted/30 px-2 py-0.5 text-[11px] text-muted-foreground">
          {priorityLabel(item.priority)}
        </span>
      </div>
    </div>
    <div className="mt-2 space-y-1.5 rounded-md border border-border/70 bg-muted/10 px-2 py-2">
      <DependencyLine
        ids={prerequisites}
        label="前置项目任务"
        resolveLabel={resolveWorkItemTitle}
      />
      {dependents.length > 0 ? (
        <DependencyLine
          ids={dependents}
          label="后续项目任务"
          resolveLabel={resolveWorkItemTitle}
          tone="dependent"
        />
      ) : null}
    </div>
    {(item.tags || []).length > 0 || item.due_at || item.dueAt ? (
      <div className="mt-2 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">
        {(item.tags || []).map((tag) => (
          <span key={tag} className="rounded border border-border/70 bg-muted/20 px-1.5 py-0.5">
            {tag}
          </span>
        ))}
        {item.due_at || item.dueAt ? (
          <span className="rounded border border-border/70 bg-muted/20 px-1.5 py-0.5">
            截止 {formatDateTime(readText(item.due_at) || readText(item.dueAt))}
          </span>
        ) : null}
      </div>
    ) : null}
  </article>
);
