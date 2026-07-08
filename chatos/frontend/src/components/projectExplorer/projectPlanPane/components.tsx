// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useEffect, useMemo, useState } from 'react';
import { ClipboardList, FileText, GitBranch, Play, RefreshCw, X } from 'lucide-react';

import type {
  ProjectRequirementDocumentResponse,
  ProjectRequirementResponse,
  ProjectWorkItemResponse,
} from '../../../lib/api/client/types';
import { cn } from '../../../lib/utils';
import { LazyMarkdownRenderer } from '../../LazyMarkdownRenderer';
import {
  type DependencyMaps,
  buildRequirementDownstreamScope,
  buildRequirementExecutionScope,
  formatDateTime,
  priorityLabel,
  readText,
  requirementParentId,
  requirementDocumentTypeLabel,
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

export const TechnicalDocumentsSection: React.FC<{
  className?: string;
  documents: ProjectRequirementDocumentResponse[];
  loading: boolean;
}> = ({ className, documents, loading }) => {
  const [selectedDocumentId, setSelectedDocumentId] = useState<string | null>(null);
  const selectedDocument = useMemo(
    () => documents.find((document) => document.id === selectedDocumentId) || documents[0] || null,
    [documents, selectedDocumentId],
  );

  useEffect(() => {
    if (documents.length === 0) {
      setSelectedDocumentId(null);
      return;
    }
    if (selectedDocumentId && documents.some((document) => document.id === selectedDocumentId)) {
      return;
    }
    setSelectedDocumentId(documents[0].id);
  }, [documents, selectedDocumentId]);

  const selectedDocType = selectedDocument
    ? readText(selectedDocument.doc_type) || readText(selectedDocument.docType)
    : '';
  const selectedUpdatedAt = selectedDocument
    ? readText(selectedDocument.updated_at) || readText(selectedDocument.updatedAt)
    : '';
  const selectedContent = selectedDocument ? readText(selectedDocument.content) : '';

  return (
    <section className={cn('mt-5 border-t border-border pt-4', className)}>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div>
          <h4 className="text-sm font-semibold text-foreground">技术文档</h4>
          <div className="mt-0.5 text-xs text-muted-foreground">
            {loading ? '正在加载技术文档...' : `${documents.length} 份文档`}
          </div>
        </div>
        {documents.length > 0 ? (
          <span className="inline-flex items-center gap-1 rounded-full border border-blue-200 bg-blue-50 px-2 py-0.5 text-xs font-medium text-blue-700 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-300">
            <FileText className="h-3.5 w-3.5" />
            可查看
          </span>
        ) : null}
      </div>
      {loading ? (
        <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
          正在加载技术文档...
        </div>
      ) : documents.length === 0 ? (
        <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
          这个需求还没有技术文档。
        </div>
      ) : (
        <div className="grid gap-3 lg:grid-cols-[260px_minmax(0,1fr)]">
          <div className="space-y-2">
            {documents.map((document, index) => {
              const docType = readText(document.doc_type) || readText(document.docType);
              const updatedAt = readText(document.updated_at) || readText(document.updatedAt);
              const active = document.id === selectedDocument?.id;
              return (
                <button
                  key={document.id}
                  type="button"
                  className={cn(
                    'w-full rounded-md border px-3 py-2 text-left transition-colors',
                    active
                      ? 'border-primary/40 bg-primary/10 shadow-sm'
                      : 'border-border bg-background hover:bg-accent/50',
                  )}
                  onClick={() => setSelectedDocumentId(document.id)}
                >
                  <div className="flex items-center gap-2">
                    <span className="rounded-full border border-border bg-muted/20 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                      {index + 1}
                    </span>
                    <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
                      {readText(document.title) || requirementDocumentTypeLabel(docType)}
                    </span>
                  </div>
                  <div className="mt-1 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">
                    <span className="rounded border border-border bg-background px-1.5 py-0.5">
                      {requirementDocumentTypeLabel(docType)}
                    </span>
                    {typeof document.version === 'number' ? (
                      <span className="rounded border border-border bg-background px-1.5 py-0.5">
                        v{document.version}
                      </span>
                    ) : null}
                  </div>
                  {updatedAt ? (
                    <div className="mt-1 truncate text-[11px] text-muted-foreground">
                      更新于 {formatDateTime(updatedAt)}
                    </div>
                  ) : null}
                </button>
              );
            })}
          </div>

          {selectedDocument ? (
            <article className="overflow-hidden rounded-md border border-border bg-background">
              <div className="flex flex-wrap items-start justify-between gap-2 border-b border-border bg-muted/20 px-3 py-2">
                <div className="min-w-0">
                  <div className="break-words text-sm font-medium text-foreground">
                    {readText(selectedDocument.title) || requirementDocumentTypeLabel(selectedDocType)}
                  </div>
                  <div className="mt-1 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">
                    <span className="rounded border border-border bg-background px-1.5 py-0.5">
                      {requirementDocumentTypeLabel(selectedDocType)}
                    </span>
                    <span className="rounded border border-border bg-background px-1.5 py-0.5">
                      {readText(selectedDocument.format) || 'markdown'}
                    </span>
                    {typeof selectedDocument.version === 'number' ? (
                      <span className="rounded border border-border bg-background px-1.5 py-0.5">
                        v{selectedDocument.version}
                      </span>
                    ) : null}
                    {selectedUpdatedAt ? (
                      <span className="rounded border border-border bg-background px-1.5 py-0.5">
                        更新于 {formatDateTime(selectedUpdatedAt)}
                      </span>
                    ) : null}
                  </div>
                </div>
              </div>
              <div className="px-3 py-2">
                {selectedContent ? (
                  <LazyMarkdownRenderer content={selectedContent} className="text-sm" />
                ) : (
                  <div className="text-sm text-muted-foreground">暂无内容</div>
                )}
              </div>
            </article>
          ) : null}
        </div>
      )}
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

type PreviewNodeKind = 'root' | 'main' | 'prerequisite' | 'optional';

type PreviewGraphNode = {
  id: string;
  kind: PreviewNodeKind;
  requirement: ProjectRequirementResponse;
  x: number;
  y: number;
};

type PreviewGraphEdge = {
  id: string;
  kind: 'dependency' | 'child';
  source: string;
  target: string;
};

const PREVIEW_NODE_WIDTH = 224;
const PREVIEW_NODE_HEIGHT = 86;
const PREVIEW_COLUMN_GAP = 96;
const PREVIEW_ROW_GAP = 22;
const PREVIEW_PADDING = 28;

const previewKindLabel = (kind: PreviewNodeKind): string => {
  switch (kind) {
    case 'root':
      return '当前';
    case 'prerequisite':
      return '补齐前置';
    case 'optional':
      return '额外后续';
    case 'main':
    default:
      return '主线';
  }
};

const previewKindClassName = (kind: PreviewNodeKind): string => {
  switch (kind) {
    case 'root':
      return 'border-blue-300 bg-blue-50 text-blue-700 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-200';
    case 'prerequisite':
      return 'border-amber-300 bg-amber-50 text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200';
    case 'optional':
      return 'border-emerald-300 bg-emerald-50 text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-200';
    case 'main':
    default:
      return 'border-border bg-muted/30 text-muted-foreground';
  }
};

const buildPreviewEdges = (
  dependencyMaps: DependencyMaps,
  requirements: ProjectRequirementResponse[],
  scopeIds: string[],
): PreviewGraphEdge[] => {
  const idSet = new Set(scopeIds);
  const edgeById = new Map<string, PreviewGraphEdge>();
  dependencyMaps.requirementPrerequisites.forEach((prerequisiteIds, target) => {
    if (!idSet.has(target)) {
      return;
    }
    prerequisiteIds.forEach((source) => {
      if (!idSet.has(source)) {
        return;
      }
      const id = `${source}->${target}`;
      edgeById.set(id, { id, source, target, kind: 'dependency' });
    });
  });
  requirements.forEach((requirement) => {
    if (!idSet.has(requirement.id)) {
      return;
    }
    const parentId = requirementParentId(requirement);
    if (!parentId || !idSet.has(parentId)) {
      return;
    }
    const id = `${parentId}->${requirement.id}`;
    if (!edgeById.has(id)) {
      edgeById.set(id, { id, source: parentId, target: requirement.id, kind: 'child' });
    }
  });
  return Array.from(edgeById.values());
};

const buildPreviewGraph = ({
  baseIds,
  dependencyMaps,
  downstreamIds,
  expandedIds,
  includePrerequisiteDependents,
  requirement,
  requirements,
}: {
  baseIds: string[];
  dependencyMaps: DependencyMaps;
  downstreamIds: string[];
  expandedIds: string[];
  includePrerequisiteDependents: boolean;
  requirement: ProjectRequirementResponse;
  requirements: ProjectRequirementResponse[];
}) => {
  const scopeIds = includePrerequisiteDependents ? expandedIds : baseIds;
  const requirementById = new Map(requirements.map((item) => [item.id, item]));
  const orderById = new Map(scopeIds.map((id, index) => [id, index]));
  const baseIdSet = new Set(baseIds);
  const downstreamIdSet = new Set(downstreamIds);
  const edges = buildPreviewEdges(dependencyMaps, requirements, scopeIds);
  const indegree = new Map(scopeIds.map((id) => [id, 0]));
  const outgoing = new Map<string, string[]>();
  edges.forEach((edge) => {
    indegree.set(edge.target, (indegree.get(edge.target) || 0) + 1);
    outgoing.set(edge.source, [...(outgoing.get(edge.source) || []), edge.target]);
  });

  const levelById = new Map(scopeIds.map((id) => [id, 0]));
  const emitted = new Set<string>();
  const queue = scopeIds
    .filter((id) => (indegree.get(id) || 0) === 0)
    .sort((left, right) => (orderById.get(left) || 0) - (orderById.get(right) || 0));
  while (queue.length > 0) {
    const current = queue.shift();
    if (!current || emitted.has(current)) {
      continue;
    }
    emitted.add(current);
    (outgoing.get(current) || []).forEach((target) => {
      levelById.set(target, Math.max(levelById.get(target) || 0, (levelById.get(current) || 0) + 1));
      const nextIndegree = (indegree.get(target) || 0) - 1;
      indegree.set(target, nextIndegree);
      if (nextIndegree === 0) {
        queue.push(target);
        queue.sort((left, right) => (orderById.get(left) || 0) - (orderById.get(right) || 0));
      }
    });
  }
  scopeIds.forEach((id) => {
    if (!emitted.has(id)) {
      levelById.set(id, Math.max(0, levelById.get(id) || 0));
    }
  });

  const idsByLevel = new Map<number, string[]>();
  scopeIds.forEach((id) => {
    const level = levelById.get(id) || 0;
    idsByLevel.set(level, [...(idsByLevel.get(level) || []), id]);
  });

  const nodes: PreviewGraphNode[] = [];
  idsByLevel.forEach((ids, level) => {
    ids.forEach((id, rowIndex) => {
      const item = requirementById.get(id);
      if (!item) {
        return;
      }
      const kind: PreviewNodeKind = id === requirement.id
        ? 'root'
        : !baseIdSet.has(id)
          ? 'optional'
          : downstreamIdSet.has(id)
            ? 'main'
            : 'prerequisite';
      nodes.push({
        id,
        kind,
        requirement: item,
        x: PREVIEW_PADDING + level * (PREVIEW_NODE_WIDTH + PREVIEW_COLUMN_GAP),
        y: PREVIEW_PADDING + rowIndex * (PREVIEW_NODE_HEIGHT + PREVIEW_ROW_GAP),
      });
    });
  });

  const maxLevel = Math.max(0, ...Array.from(idsByLevel.keys()));
  const maxRows = Math.max(1, ...Array.from(idsByLevel.values()).map((ids) => ids.length));
  return {
    edges,
    height: PREVIEW_PADDING * 2 + maxRows * PREVIEW_NODE_HEIGHT + (maxRows - 1) * PREVIEW_ROW_GAP,
    nodes,
    optionalCount: expandedIds.filter((id) => !baseIdSet.has(id)).length,
    scopeIds,
    width: PREVIEW_PADDING * 2 + (maxLevel + 1) * PREVIEW_NODE_WIDTH + maxLevel * PREVIEW_COLUMN_GAP,
  };
};

const previewEdgePath = (
  edge: PreviewGraphEdge,
  nodeById: Map<string, PreviewGraphNode>,
): string => {
  const source = nodeById.get(edge.source);
  const target = nodeById.get(edge.target);
  if (!source || !target) {
    return '';
  }
  const sx = source.x + PREVIEW_NODE_WIDTH;
  const sy = source.y + PREVIEW_NODE_HEIGHT / 2;
  const tx = target.x;
  const ty = target.y + PREVIEW_NODE_HEIGHT / 2;
  const bend = Math.max(42, Math.abs(tx - sx) / 2);
  return `M ${sx} ${sy} C ${sx + bend} ${sy}, ${tx - bend} ${ty}, ${tx} ${ty}`;
};

export const RequirementExecutionPreviewModal: React.FC<{
  dependencyMaps: DependencyMaps;
  requirement: ProjectRequirementResponse;
  requirements: ProjectRequirementResponse[];
  running: boolean;
  onClose: () => void;
  onConfirm?: (includePrerequisiteDependents: boolean) => void;
}> = ({
  dependencyMaps,
  requirement,
  requirements,
  running,
  onClose,
  onConfirm,
}) => {
  const [includePrerequisiteDependents, setIncludePrerequisiteDependents] = useState(false);
  const downstreamIds = useMemo(() => buildRequirementDownstreamScope({
    dependencyMaps,
    requirements,
    rootId: requirement.id,
  }), [dependencyMaps, requirement.id, requirements]);
  const baseIds = useMemo(() => buildRequirementExecutionScope({
    dependencyMaps,
    requirements,
    rootId: requirement.id,
  }), [dependencyMaps, requirement.id, requirements]);
  const expandedIds = useMemo(() => buildRequirementExecutionScope({
    dependencyMaps,
    includePrerequisiteDependents: true,
    requirements,
    rootId: requirement.id,
  }), [dependencyMaps, requirement.id, requirements]);
  const graph = useMemo(() => buildPreviewGraph({
    baseIds,
    dependencyMaps,
    downstreamIds,
    expandedIds,
    includePrerequisiteDependents,
    requirement,
    requirements,
  }), [
    baseIds,
    dependencyMaps,
    downstreamIds,
    expandedIds,
    includePrerequisiteDependents,
    requirement,
    requirements,
  ]);
  const nodeById = useMemo(() => new Map(graph.nodes.map((node) => [node.id, node])), [graph.nodes]);
  const optionalAvailable = expandedIds.some((id) => !baseIds.includes(id));

  return (
    <div className="fixed inset-0 z-[70]">
      <button
        type="button"
        aria-label="关闭执行预览"
        className="absolute inset-0 bg-black/45"
        onClick={onClose}
      />
      <section className="absolute left-1/2 top-1/2 flex max-h-[94vh] w-[calc(100vw-16px)] max-w-[1500px] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xl">
        <header className="flex shrink-0 items-start justify-between gap-4 border-b border-border px-4 py-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <GitBranch className="h-3.5 w-3.5" />
              <span>{onConfirm ? '执行预览' : '流程预览'}</span>
            </div>
            <h2 className="mt-1 break-words text-sm font-semibold text-foreground">
              {requirement.title || requirement.id}
            </h2>
          </div>
          <button
            type="button"
            className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
            onClick={onClose}
            aria-label="关闭"
          >
            <X className="h-4 w-4" />
          </button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto p-4">
          <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
            <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
              {(['root', 'main', 'prerequisite', 'optional'] as PreviewNodeKind[]).map((kind) => (
                <span
                  key={kind}
                  className={cn('rounded-full border px-2 py-0.5 font-medium', previewKindClassName(kind))}
                >
                  {previewKindLabel(kind)}
                </span>
              ))}
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={includePrerequisiteDependents}
              className={cn(
                'inline-flex items-center gap-2 rounded-md border border-border bg-background px-3 py-2 text-xs transition-colors',
                'hover:bg-accent hover:text-foreground',
              )}
              onClick={() => setIncludePrerequisiteDependents((current) => !current)}
            >
              <span
                aria-hidden
                className={cn(
                  'relative h-4 w-7 rounded-full border transition-colors',
                  includePrerequisiteDependents
                    ? 'border-primary bg-primary'
                    : 'border-border bg-muted',
                )}
              >
                <span
                  className={cn(
                    'absolute top-1/2 h-3 w-3 -translate-y-1/2 rounded-full bg-background shadow-sm transition-transform',
                    includePrerequisiteDependents ? 'translate-x-3.5' : 'translate-x-0.5',
                  )}
                />
              </span>
              <span>include_prerequisite_dependents</span>
              <span className="text-muted-foreground">
                {includePrerequisiteDependents ? '开' : '关'}
              </span>
            </button>
          </div>

          <div className="mb-3 grid grid-cols-3 gap-2 text-xs">
            <div className="rounded-md border border-border bg-muted/20 px-3 py-2">
              <div className="text-muted-foreground">当前范围</div>
              <div className="mt-1 text-base font-semibold text-foreground">{graph.scopeIds.length}</div>
            </div>
            <div className="rounded-md border border-border bg-muted/20 px-3 py-2">
              <div className="text-muted-foreground">默认范围</div>
              <div className="mt-1 text-base font-semibold text-foreground">{baseIds.length}</div>
            </div>
            <div className="rounded-md border border-border bg-muted/20 px-3 py-2">
              <div className="text-muted-foreground">额外后续</div>
              <div className="mt-1 text-base font-semibold text-foreground">{graph.optionalCount}</div>
            </div>
          </div>

          <div className="h-[66vh] min-h-[460px] overflow-auto rounded-md border border-border bg-muted/10">
            <div
              className="relative"
              style={{ width: graph.width, height: graph.height }}
            >
              <svg
                className="pointer-events-none absolute inset-0"
                width={graph.width}
                height={graph.height}
                viewBox={`0 0 ${graph.width} ${graph.height}`}
                aria-hidden
              >
                <defs>
                  <marker id="requirement-preview-arrow" markerWidth="10" markerHeight="10" refX="8" refY="3" orient="auto" markerUnits="strokeWidth">
                    <path d="M0,0 L0,6 L9,3 z" fill="rgba(100,116,139,0.82)" />
                  </marker>
                </defs>
                {graph.edges.map((edge) => (
                  <path
                    key={edge.id}
                    d={previewEdgePath(edge, nodeById)}
                    fill="none"
                    stroke={edge.kind === 'child' ? 'rgba(148,163,184,0.55)' : 'rgba(100,116,139,0.82)'}
                    strokeDasharray={edge.kind === 'child' ? '5 5' : undefined}
                    strokeWidth={edge.kind === 'child' ? 1.4 : 1.8}
                    markerEnd="url(#requirement-preview-arrow)"
                  />
                ))}
              </svg>
              {graph.nodes.map((node) => (
                <article
                  key={node.id}
                  className="absolute overflow-hidden rounded-md border bg-background px-3 py-2 shadow-sm"
                  style={{
                    left: node.x,
                    top: node.y,
                    width: PREVIEW_NODE_WIDTH,
                    height: PREVIEW_NODE_HEIGHT,
                  }}
                >
                  <div className="mb-1 flex items-center gap-1.5">
                    <span className={cn('rounded-full border px-1.5 py-0.5 text-[10px] font-medium', previewKindClassName(node.kind))}>
                      {previewKindLabel(node.kind)}
                    </span>
                    <span className={cn('rounded-full border px-1.5 py-0.5 text-[10px]', statusClassName(node.requirement.status))}>
                      {statusLabel(node.requirement.status)}
                    </span>
                  </div>
                  <div className="line-clamp-2 break-words text-xs font-semibold leading-4 text-foreground">
                    {node.requirement.title || node.requirement.id}
                  </div>
                  {readText(node.requirement.summary) ? (
                    <div className="mt-1 line-clamp-1 text-[11px] text-muted-foreground">
                      {node.requirement.summary}
                    </div>
                  ) : null}
                </article>
              ))}
            </div>
          </div>
        </div>

        <footer className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-t border-border px-4 py-3">
          <div className="text-xs text-muted-foreground">
            {optionalAvailable
              ? '切换开关可预览是否纳入前置解锁出的额外后续需求。'
              : '当前没有前置解锁出的额外后续需求。'}
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="rounded-md border border-border bg-background px-3 py-1.5 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-foreground"
              onClick={onClose}
            >
              取消
            </button>
            {onConfirm ? (
              <button
                type="button"
                className="inline-flex items-center gap-1.5 rounded-md border border-primary/40 bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:cursor-wait disabled:opacity-60"
                disabled={running}
                onClick={() => onConfirm(includePrerequisiteDependents)}
              >
                <Play className="h-3.5 w-3.5" />
                {running ? '执行中' : '按当前范围执行'}
              </button>
            ) : null}
          </div>
        </footer>
      </section>
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
