import React, { useMemo } from 'react';

import type { FsEntry, Project, ProjectChangeSummary, ProjectSearchHit } from '../../types';
import { cn, formatFileSize } from '../../lib/utils';
import {
  CHANGE_KIND_COLOR_CLASS,
  CHANGE_KIND_LABEL,
  CHANGE_KIND_ROW_CLASS,
  CHANGE_KIND_TEXT_CLASS,
  splitTextByQuery,
} from './utils';
import type { ChangeKind } from './utils';

interface ProjectTreePaneProps {
  project: Project;
  treeWidth: number;
  treeScrollRef: React.MutableRefObject<HTMLDivElement | null>;
  entriesMap: Record<string, FsEntry[]>;
  expandedPaths: Set<string>;
  loadingPaths: Set<string>;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  draggingEntryPath: string | null;
  dropTargetDirPath: string | null;
  actionLoading: boolean;
  actionReloadPath: string | null;
  canConfirmCurrent: boolean;
  showOnlyChanged: boolean;
  changeSummary: ProjectChangeSummary;
  loadingSummary: boolean;
  summaryError: string | null;
  actionMessage: string | null;
  actionError: string | null;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  searchLoading: boolean;
  searchError: string | null;
  searchTruncated: boolean;
  activeSearchHitId: string | null;
  activeSearchHitIndex: number;
  totalSearchHits: number;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
  aggregatedChangeKindByPath: Map<string, ChangeKind>;
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  onSelectProjectRoot: () => void;
  onToggleShowOnlyChanged: () => void;
  onCreateDirectoryAtRoot: () => void;
  onCreateFileAtRoot: () => void;
  onRefresh: () => void;
  onConfirmCurrent: () => void;
  onConfirmAll: () => void;
  onSearchQueryChange: (value: string) => void;
  onToggleSearchCaseSensitive: () => void;
  onToggleSearchWholeWord: () => void;
  onClearSearch: () => void;
  onOpenPreviousSearchHit: () => void;
  onOpenNextSearchHit: () => void;
  onOpenContextMenu: (event: React.MouseEvent, entry: FsEntry) => void;
  onOpenSearchHit: (hit: ProjectSearchHit) => void;
  onSelectDeletedPath: (path: string) => void;
  onSelectMarkedPath: (path: string) => void;
  onToggleDir: (entry: FsEntry) => void;
  onOpenFile: (entry: FsEntry) => void;
  onDragStart: (event: React.DragEvent, entry: FsEntry) => void;
  onDragEnd: () => void;
  onSetDropTargetDirPath: React.Dispatch<React.SetStateAction<string | null>>;
  onSetDraggingEntryPath: React.Dispatch<React.SetStateAction<string | null>>;
  onMoveEntryByDrop: (sourcePath: string, targetDirPath: string) => void;
  onScheduleDragExpand: (path: string) => void;
  onCancelDragExpandIfMatches: (path: string) => void;
  onClearDragExpandTimer: () => void;
  onStartDragAutoScroll: (velocity: number) => void;
  onClearDragAutoScroll: () => void;
}

export const ProjectTreePane: React.FC<ProjectTreePaneProps> = ({
  project,
  treeWidth,
  treeScrollRef,
  entriesMap,
  expandedPaths,
  loadingPaths,
  selectedPath,
  selectedEntry,
  draggingEntryPath,
  dropTargetDirPath,
  actionLoading,
  actionReloadPath,
  canConfirmCurrent,
  showOnlyChanged,
  changeSummary,
  loadingSummary,
  summaryError,
  actionMessage,
  actionError,
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  searchLoading,
  searchError,
  searchTruncated,
  activeSearchHitId,
  activeSearchHitIndex,
  totalSearchHits,
  canOpenPreviousSearchHit,
  canOpenNextSearchHit,
  aggregatedChangeKindByPath,
  normalizePath,
  toExpandedKey,
  canDropToDirectory,
  onSelectProjectRoot,
  onToggleShowOnlyChanged,
  onCreateDirectoryAtRoot,
  onCreateFileAtRoot,
  onRefresh,
  onConfirmCurrent,
  onConfirmAll,
  onSearchQueryChange,
  onToggleSearchCaseSensitive,
  onToggleSearchWholeWord,
  onClearSearch,
  onOpenPreviousSearchHit,
  onOpenNextSearchHit,
  onOpenContextMenu,
  onOpenSearchHit,
  onSelectDeletedPath,
  onSelectMarkedPath,
  onToggleDir,
  onOpenFile,
  onDragStart,
  onDragEnd,
  onSetDropTargetDirPath,
  onSetDraggingEntryPath,
  onMoveEntryByDrop,
  onScheduleDragExpand,
  onCancelDragExpandIfMatches,
  onClearDragExpandTimer,
  onStartDragAutoScroll,
  onClearDragAutoScroll,
}) => {
  const renderHighlightedText = (text: string, query: string): React.ReactNode => (
    splitTextByQuery(text, query, {
      caseSensitive: searchCaseSensitive,
      wholeWord: searchWholeWord,
    }).map((segment, index) => (
      segment.matched ? (
        <mark
          key={`${segment.text}-${index}`}
          className="rounded bg-amber-300/60 px-0.5 text-inherit"
        >
          {segment.text}
        </mark>
      ) : (
        <React.Fragment key={`${segment.text}-${index}`}>
          {segment.text}
        </React.Fragment>
      )
    ))
  );

  const activeSearchOptionLabels = useMemo(
    () => [
      searchCaseSensitive ? '区分大小写' : null,
      searchWholeWord ? '全词匹配' : null,
    ].filter((value): value is string => Boolean(value)),
    [searchCaseSensitive, searchWholeWord]
  );
  const activeSearchPositionLabel = useMemo(() => {
    if (totalSearchHits <= 0) {
      return null;
    }
    const currentIndex = activeSearchHitIndex >= 0 ? activeSearchHitIndex + 1 : 0;
    return `${currentIndex} / ${totalSearchHits}`;
  }, [activeSearchHitIndex, totalSearchHits]);
  const handleSearchInputKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) {
      return;
    }
    if (event.key !== 'Enter' || searchQuery.trim().length === 0 || totalSearchHits <= 0) {
      return;
    }
    event.preventDefault();
    if (event.shiftKey) {
      onOpenPreviousSearchHit();
      return;
    }
    onOpenNextSearchHit();
  };

  const projectRootEntry = useMemo<FsEntry>(() => ({
    name: project.name || project.rootPath,
    path: project.rootPath,
    isDir: true,
    size: null,
    modifiedAt: null,
  }), [project.name, project.rootPath]);

  const isEntryVisible = (entryPath: string): boolean => {
    if (!showOnlyChanged) return true;
    return aggregatedChangeKindByPath.has(normalizePath(entryPath));
  };

  const visibleRootEntryCount = useMemo(() => {
    const rootEntries = entriesMap[project.rootPath] || [];
    return rootEntries.filter((entry) => isEntryVisible(entry.path)).length;
  }, [entriesMap, project.rootPath, showOnlyChanged, aggregatedChangeKindByPath, normalizePath]);

  const loadedEntryPathSet = useMemo(() => {
    const out = new Set<string>();
    Object.values(entriesMap).forEach((entries) => {
      entries.forEach((entry) => {
        const normalized = normalizePath(entry.path);
        if (normalized) {
          out.add(normalized);
        }
      });
    });
    return out;
  }, [entriesMap, normalizePath]);

  const hiddenFileMarks = useMemo(
    () => changeSummary.fileMarks.filter((mark) => {
      const normalizedMarkPath = normalizePath(mark.path);
      if (!normalizedMarkPath) {
        return false;
      }
      return !loadedEntryPathSet.has(normalizedMarkPath);
    }),
    [changeSummary.fileMarks, loadedEntryPathSet, normalizePath]
  );

  const renderEntries = (path: string, depth: number): React.ReactNode => {
    const entries = (entriesMap[path] || []).filter((entry) => isEntryVisible(entry.path));
    if (!entries.length) {
      return null;
    }
    return entries.map((entry) => {
      const entryKey = toExpandedKey(entry.path);
      const normalizedEntryPath = normalizePath(entry.path);
      const isActive = selectedPath ? normalizePath(selectedPath) === normalizedEntryPath : false;
      const isDragging = draggingEntryPath ? normalizePath(draggingEntryPath) === normalizedEntryPath : false;
      const isDropTarget = entry.isDir && dropTargetDirPath
        ? normalizePath(dropTargetDirPath) === normalizedEntryPath
        : false;
      const entryChangeKind = aggregatedChangeKindByPath.get(normalizedEntryPath);
      return (
        <div key={entry.path}>
          <button
            type="button"
            onClick={() => (entry.isDir ? onToggleDir(entry) : onOpenFile(entry))}
            onContextMenu={(event) => onOpenContextMenu(event, entry)}
            draggable
            onDragStart={(event) => onDragStart(event, entry)}
            onDragEnd={onDragEnd}
            onDragOver={(event) => {
              if (!entry.isDir) return;
              const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
              if (!sourcePath || !canDropToDirectory(sourcePath, entry.path)) return;
              event.preventDefault();
              event.dataTransfer.dropEffect = 'move';
            }}
            onDragEnter={(event) => {
              if (!entry.isDir) return;
              const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
              if (!sourcePath || !canDropToDirectory(sourcePath, entry.path)) return;
              event.preventDefault();
              onSetDropTargetDirPath(entry.path);
              onScheduleDragExpand(entry.path);
            }}
            onDragLeave={(event) => {
              if (!entry.isDir) return;
              const nextTarget = event.relatedTarget as Node | null;
              if (nextTarget && (event.currentTarget as HTMLElement).contains(nextTarget)) {
                return;
              }
              onCancelDragExpandIfMatches(entry.path);
              onClearDragAutoScroll();
              onSetDropTargetDirPath((prev) => (
                prev && normalizePath(prev) === normalizePath(entry.path) ? null : prev
              ));
            }}
            onDrop={(event) => {
              if (!entry.isDir) return;
              const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
              if (!sourcePath) return;
              if (!canDropToDirectory(sourcePath, entry.path)) return;
              event.preventDefault();
              event.stopPropagation();
              onCancelDragExpandIfMatches(entry.path);
              onClearDragAutoScroll();
              onSetDropTargetDirPath(null);
              onSetDraggingEntryPath(null);
              onMoveEntryByDrop(sourcePath, entry.path);
            }}
            className={cn(
              'min-w-full w-max grid grid-cols-[12px_auto_64px] items-center gap-2 py-1.5 pr-2 text-left rounded hover:bg-accent transition-colors',
              entryChangeKind && CHANGE_KIND_ROW_CLASS[entryChangeKind],
              isActive && 'bg-accent',
              isDragging && 'opacity-50',
              isDropTarget && 'ring-1 ring-blue-500 bg-blue-500/10'
            )}
            style={{ paddingLeft: 12 + depth * 14 }}
          >
            <span className="text-xs text-muted-foreground w-3 shrink-0">
              {entry.isDir ? (expandedPaths.has(entryKey) ? '▾' : '▸') : ''}
            </span>
            <span
              className={cn(
                'text-sm whitespace-nowrap inline-flex items-center gap-1',
                entry.isDir ? 'text-foreground' : 'text-muted-foreground',
                entryChangeKind && CHANGE_KIND_TEXT_CLASS[entryChangeKind]
              )}
            >
              {entry.name}
              {entryChangeKind && (
                <span
                  className={cn('inline-block h-2 w-2 rounded-full', CHANGE_KIND_COLOR_CLASS[entryChangeKind])}
                  title={`未确认${CHANGE_KIND_LABEL[entryChangeKind]}变更`}
                />
              )}
            </span>
            <span className="text-[11px] text-muted-foreground text-right tabular-nums whitespace-nowrap">
              {!entry.isDir && entry.size != null ? formatFileSize(entry.size) : ''}
            </span>
          </button>
          {entry.isDir && expandedPaths.has(entryKey) && renderEntries(entry.path, depth + 1)}
        </div>
      );
    });
  };

  const renderSearchResults = (): React.ReactNode => {
    const keyword = searchQuery.trim();
    if (searchLoading) {
      return <div className="px-3 py-2 text-xs text-muted-foreground">全文搜索中...</div>;
    }
    if (searchError) {
      return <div className="px-3 py-2 text-xs text-destructive">{searchError}</div>;
    }
    if (searchResults.length === 0) {
      return <div className="px-3 py-2 text-xs text-muted-foreground">没有找到匹配内容</div>;
    }

    return searchResults.map((hit) => {
      const hitId = `${hit.path}:${hit.line}:${hit.column}`;
      const isActiveHit = activeSearchHitId === hitId;
      return (
        <button
          key={hitId}
          type="button"
          onClick={() => onOpenSearchHit(hit)}
          className={cn(
            'w-full border-b border-border/60 px-3 py-2 text-left hover:bg-accent transition-colors',
            isActiveHit && 'bg-accent'
          )}
          title={`${hit.relativePath}:${hit.line}:${hit.column}`}
        >
          <div className="flex items-center justify-between gap-2 text-[11px]">
            <span className="min-w-0 truncate text-foreground">
              {renderHighlightedText(hit.relativePath, keyword)}
            </span>
            <span className="shrink-0 text-muted-foreground">L{hit.line}:C{hit.column}</span>
          </div>
          <div className="mt-1 whitespace-pre-wrap break-all font-mono text-xs text-muted-foreground">
            {hit.text
              ? renderHighlightedText(hit.text, keyword)
              : '(空行)'}
          </div>
        </button>
      );
    });
  };

  return (
    <div className="border-r border-border bg-card flex flex-col shrink-0" style={{ width: treeWidth }}>
      <div
        className={cn(
          'px-3 py-2 border-b border-border space-y-2',
          dropTargetDirPath && normalizePath(dropTargetDirPath) === normalizePath(project.rootPath)
            ? 'ring-1 ring-blue-500 bg-blue-500/10'
            : ''
        )}
        onContextMenu={(event) => {
          onOpenContextMenu(event, projectRootEntry);
        }}
        onDragOver={(event) => {
          const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
          if (!sourcePath) return;
          if (!canDropToDirectory(sourcePath, project.rootPath)) return;
          event.preventDefault();
          event.dataTransfer.dropEffect = 'move';
        }}
        onDragEnter={(event) => {
          const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
          if (!sourcePath) return;
          if (!canDropToDirectory(sourcePath, project.rootPath)) return;
          event.preventDefault();
          onClearDragExpandTimer();
          onClearDragAutoScroll();
          onSetDropTargetDirPath(project.rootPath);
        }}
        onDragLeave={(event) => {
          const nextTarget = event.relatedTarget as Node | null;
          if (nextTarget && (event.currentTarget as HTMLElement).contains(nextTarget)) {
            return;
          }
          const normalizedRoot = normalizePath(project.rootPath);
          onSetDropTargetDirPath((prev) => (
            prev && normalizePath(prev) === normalizedRoot ? null : prev
          ));
        }}
        onDrop={(event) => {
          const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
          if (!sourcePath) return;
          if (!canDropToDirectory(sourcePath, project.rootPath)) return;
          event.preventDefault();
          event.stopPropagation();
          onClearDragExpandTimer();
          onClearDragAutoScroll();
          onSetDropTargetDirPath(null);
          onSetDraggingEntryPath(null);
          onMoveEntryByDrop(sourcePath, project.rootPath);
        }}
      >
        <div className="text-xs text-muted-foreground">项目目录</div>
        <div className="text-sm font-medium text-foreground truncate" title={project.rootPath}>
          {project.name}
        </div>
        <div className="text-[11px] text-muted-foreground truncate" title={project.rootPath}>
          {project.rootPath}
        </div>
        <div className="text-[11px] text-muted-foreground truncate" title={selectedEntry?.path || ''}>
          当前选择：{selectedEntry ? selectedEntry.path : '未选择'}
        </div>
        <button
          type="button"
          onClick={(event) => {
            event.stopPropagation();
            onSelectProjectRoot();
          }}
          className="text-[11px] text-blue-600 hover:underline text-left"
        >
          选中项目根目录
        </button>
        <div className="text-[11px] text-muted-foreground flex items-center gap-3">
          <span className="inline-flex items-center gap-1">
            <span className="inline-block h-2 w-2 rounded-full bg-emerald-500" />
            新增 {changeSummary.counts.create}
          </span>
          <span className="inline-flex items-center gap-1">
            <span className="inline-block h-2 w-2 rounded-full bg-amber-500" />
            编辑 {changeSummary.counts.edit}
          </span>
          <span className="inline-flex items-center gap-1">
            <span className="inline-block h-2 w-2 rounded-full bg-rose-500" />
            删除 {changeSummary.counts.delete}
          </span>
        </div>
        <div className="flex flex-wrap gap-1">
          <button
            type="button"
            onClick={onCreateDirectoryAtRoot}
            disabled={actionLoading}
            className="rounded border border-blue-500/40 px-2 py-1 text-[11px] text-blue-700 hover:bg-blue-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            根目录新建目录
          </button>
          <button
            type="button"
            onClick={onCreateFileAtRoot}
            disabled={actionLoading}
            className="rounded border border-blue-500/40 px-2 py-1 text-[11px] text-blue-700 hover:bg-blue-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            根目录新建文件
          </button>
          <button
            type="button"
            onClick={onRefresh}
            disabled={!actionReloadPath || actionLoading}
            className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            刷新
          </button>
          <button
            type="button"
            onClick={onConfirmCurrent}
            disabled={!canConfirmCurrent || actionLoading}
            className="rounded border border-amber-500/40 px-2 py-1 text-[11px] text-amber-700 hover:bg-amber-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            确认当前项
          </button>
          <button
            type="button"
            onClick={onConfirmAll}
            disabled={changeSummary.counts.total <= 0 || actionLoading}
            className="rounded border border-emerald-500/40 px-2 py-1 text-[11px] text-emerald-700 hover:bg-emerald-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            确认全部变更
          </button>
          <button
            type="button"
            onClick={onToggleShowOnlyChanged}
            className={cn(
              'rounded border px-2 py-1 text-[11px] disabled:opacity-50 disabled:cursor-not-allowed',
              showOnlyChanged
                ? 'border-emerald-500/50 text-emerald-700 bg-emerald-500/10 hover:bg-emerald-500/20'
                : 'border-border hover:bg-accent'
            )}
          >
            {showOnlyChanged ? '显示全部' : '仅看变更'}
          </button>
        </div>
        <div className="text-[11px] text-muted-foreground">
          目录/文件的新建、下载、删除请右键对应项操作
        </div>
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={searchQuery}
              onChange={(event) => onSearchQueryChange(event.target.value)}
              onKeyDown={handleSearchInputKeyDown}
              placeholder="全文搜索注释、符号、字符串"
              className="h-8 flex-1 rounded border border-border bg-background px-2 text-xs outline-none focus:border-primary"
            />
            {searchQuery.trim().length > 0 && (
              <button
                type="button"
                onClick={onClearSearch}
                className="h-8 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent"
              >
                清空
              </button>
            )}
          </div>
          <div className="flex flex-wrap gap-1">
            <button
              type="button"
              onClick={onToggleSearchCaseSensitive}
              className={cn(
                'rounded border px-2 py-1 text-[11px] transition-colors',
                searchCaseSensitive
                  ? 'border-amber-500/50 bg-amber-500/10 text-amber-700 hover:bg-amber-500/20'
                  : 'border-border hover:bg-accent'
              )}
            >
              区分大小写
            </button>
            <button
              type="button"
              onClick={onToggleSearchWholeWord}
              className={cn(
                'rounded border px-2 py-1 text-[11px] transition-colors',
                searchWholeWord
                  ? 'border-amber-500/50 bg-amber-500/10 text-amber-700 hover:bg-amber-500/20'
                  : 'border-border hover:bg-accent'
              )}
            >
              全词匹配
            </button>
          </div>
          {searchQuery.trim().length > 0 && totalSearchHits > 0 && (
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={onOpenPreviousSearchHit}
                disabled={!canOpenPreviousSearchHit}
                className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                上一处
              </button>
              <button
                type="button"
                onClick={onOpenNextSearchHit}
                disabled={!canOpenNextSearchHit}
                className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                下一处
              </button>
              <span className="text-[11px] text-muted-foreground">
                当前命中 {activeSearchPositionLabel}
              </span>
            </div>
          )}
          <div className="text-[11px] text-muted-foreground">
            {searchQuery.trim().length > 0
              ? `全文搜索结果 ${searchResults.length}${searchTruncated ? '+' : ''}${activeSearchOptionLabels.length > 0 ? ` · ${activeSearchOptionLabels.join(' · ')}` : ''}`
              : activeSearchOptionLabels.length > 0
                ? `支持跨文件全文搜索 · ${activeSearchOptionLabels.join(' · ')}`
                : '支持跨文件全文搜索'}
          </div>
          {searchQuery.trim().length > 0 && (
            <div className="text-[11px] text-muted-foreground">
              快捷键：Enter 下一处 / Shift+Enter 上一处
            </div>
          )}
        </div>
        {loadingSummary && (
          <div className="text-[11px] text-muted-foreground">正在加载变更标记...</div>
        )}
        {summaryError && (
          <div className="text-[11px] text-destructive truncate" title={summaryError}>
            {summaryError}
          </div>
        )}
        {actionMessage && (
          <div className="text-[11px] text-emerald-600 truncate" title={actionMessage}>
            {actionMessage}
          </div>
        )}
        {actionError && (
          <div className="text-[11px] text-destructive truncate" title={actionError}>
            {actionError}
          </div>
        )}
      </div>
      <div
        ref={treeScrollRef}
        className="flex-1 overflow-y-auto overflow-x-auto py-2"
        onDragOver={(event) => {
          if (!draggingEntryPath) return;
          const container = treeScrollRef.current;
          if (!container) return;
          const rect = container.getBoundingClientRect();
          const threshold = Math.max(28, Math.min(64, rect.height / 3));
          let velocity = 0;

          if (event.clientY < rect.top + threshold) {
            const ratio = (rect.top + threshold - event.clientY) / threshold;
            velocity = -Math.max(4, Math.round(22 * ratio));
          } else if (event.clientY > rect.bottom - threshold) {
            const ratio = (event.clientY - (rect.bottom - threshold)) / threshold;
            velocity = Math.max(4, Math.round(22 * ratio));
          }

          if (velocity !== 0) {
            event.preventDefault();
            onStartDragAutoScroll(velocity);
          } else {
            onClearDragAutoScroll();
          }
        }}
        onDragLeave={(event) => {
          const nextTarget = event.relatedTarget as Node | null;
          if (nextTarget && (event.currentTarget as HTMLElement).contains(nextTarget)) {
            return;
          }
          onClearDragAutoScroll();
        }}
        onDrop={() => {
          onClearDragAutoScroll();
        }}
      >
        {searchQuery.trim().length > 0 ? (
          renderSearchResults()
        ) : (
          <>
            {renderEntries(project.rootPath, 0)}
            {changeSummary.deletedMarks.length > 0 && (
          <div className="mt-2 border-t border-border/70">
            <div className="px-3 py-2 text-[11px] font-medium text-rose-600 dark:text-rose-400">
              已删除（未确认）
            </div>
            <div className="space-y-0.5 pb-2">
              {changeSummary.deletedMarks.map((mark) => {
                const normalizedMarkPath = normalizePath(mark.path);
                const isActive = selectedPath ? normalizePath(selectedPath) === normalizedMarkPath : false;
                return (
                  <button
                    key={mark.lastChangeId || mark.path}
                    type="button"
                    onClick={() => onSelectDeletedPath(mark.path)}
                    className={cn(
                      'min-w-full w-max grid grid-cols-[12px_auto_64px] items-center gap-2 py-1.5 pr-2 text-left rounded hover:bg-accent transition-colors',
                      isActive && 'bg-accent'
                    )}
                    style={{ paddingLeft: 12 + 14 }}
                  >
                    <span className="text-xs text-rose-500 w-3 shrink-0">•</span>
                    <span className={cn('text-sm whitespace-nowrap truncate', CHANGE_KIND_TEXT_CLASS.delete)}>
                      {mark.relativePath || mark.path}
                    </span>
                    <span className="text-[11px] text-muted-foreground text-right tabular-nums whitespace-nowrap">
                      已删除
                    </span>
                  </button>
                );
              })}
            </div>
          </div>
            )}
            {showOnlyChanged && hiddenFileMarks.length > 0 && (
          <div className="mt-2 border-t border-border/70">
            <div className="px-3 py-2 text-[11px] font-medium text-amber-600 dark:text-amber-400">
              未在当前目录树显示（未确认）
            </div>
            <div className="space-y-0.5 pb-2">
              {hiddenFileMarks.map((mark) => {
                const normalizedMarkPath = normalizePath(mark.path);
                const isActive = selectedPath ? normalizePath(selectedPath) === normalizedMarkPath : false;
                return (
                  <button
                    key={mark.lastChangeId || mark.path}
                    type="button"
                    onClick={() => onSelectMarkedPath(mark.path)}
                    className={cn(
                      'min-w-full w-max grid grid-cols-[12px_auto_64px] items-center gap-2 py-1.5 pr-2 text-left rounded hover:bg-accent transition-colors',
                      isActive && 'bg-accent'
                    )}
                    style={{ paddingLeft: 12 + 14 }}
                  >
                    <span className={cn('inline-block h-2 w-2 rounded-full', CHANGE_KIND_COLOR_CLASS[mark.kind])} />
                    <span className={cn('text-sm whitespace-nowrap truncate', CHANGE_KIND_TEXT_CLASS[mark.kind])}>
                      {mark.relativePath || mark.path}
                    </span>
                    <span className="text-[11px] text-muted-foreground text-right tabular-nums whitespace-nowrap">
                      {CHANGE_KIND_LABEL[mark.kind]}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>
            )}
            {loadingPaths.has(project.rootPath) && (
          <div className="px-3 py-2 text-xs text-muted-foreground">加载中...</div>
            )}
            {!loadingPaths.has(project.rootPath) && visibleRootEntryCount === 0 && (
          <div className="px-3 py-2 text-xs text-muted-foreground">
            {showOnlyChanged
              ? (changeSummary.counts.total > 0
                ? '存在未确认变更，但当前目录树未命中。请查看下方列表。'
                : '暂无未确认变更文件')
              : '目录为空'}
          </div>
            )}
          </>
        )}
      </div>
    </div>
  );
};
