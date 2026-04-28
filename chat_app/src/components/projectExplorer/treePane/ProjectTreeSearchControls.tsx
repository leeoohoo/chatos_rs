import React, { useMemo } from 'react';

import type { ProjectSearchHit } from '../../../types';
import { cn } from '../../../lib/utils';

interface ProjectTreeSearchControlsProps {
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  totalSearchHits: number;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
  activeSearchHitIndex: number;
  searchLoading: boolean;
  searchError: string | null;
  searchTruncated: boolean;
  onSearchQueryChange: (value: string) => void;
  onToggleSearchCaseSensitive: () => void;
  onToggleSearchWholeWord: () => void;
  onClearSearch: () => void;
  onOpenPreviousSearchHit: () => void;
  onOpenNextSearchHit: () => void;
}

export const ProjectTreeSearchControls: React.FC<ProjectTreeSearchControlsProps> = ({
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  totalSearchHits,
  canOpenPreviousSearchHit,
  canOpenNextSearchHit,
  activeSearchHitIndex,
  searchLoading,
  searchError,
  searchTruncated,
  onSearchQueryChange,
  onToggleSearchCaseSensitive,
  onToggleSearchWholeWord,
  onClearSearch,
  onOpenPreviousSearchHit,
  onOpenNextSearchHit,
}) => {
  const activeSearchOptionLabels = useMemo(
    () => [
      searchCaseSensitive ? '区分大小写' : null,
      searchWholeWord ? '全词匹配' : null,
    ].filter((value): value is string => Boolean(value)),
    [searchCaseSensitive, searchWholeWord],
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

  return (
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
              : 'border-border hover:bg-accent',
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
              : 'border-border hover:bg-accent',
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
            className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
          >
            上一处
          </button>
          <button
            type="button"
            onClick={onOpenNextSearchHit}
            disabled={!canOpenNextSearchHit}
            className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
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
      {searchLoading && (
        <div className="text-[11px] text-muted-foreground">全文搜索中...</div>
      )}
      {searchError && (
        <div className="text-[11px] text-destructive truncate" title={searchError}>
          {searchError}
        </div>
      )}
    </div>
  );
};
