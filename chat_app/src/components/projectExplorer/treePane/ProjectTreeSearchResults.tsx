// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type { ProjectSearchHit } from '../../../types';
import { cn } from '../../../lib/utils';
import { splitTextByQuery } from '../utils';

interface ProjectTreeSearchResultsProps {
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  activeSearchHitId: string | null;
  onOpenSearchHit: (hit: ProjectSearchHit) => void;
}

export const ProjectTreeSearchResults: React.FC<ProjectTreeSearchResultsProps> = ({
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  activeSearchHitId,
  onOpenSearchHit,
}) => {
  const { t } = useI18n();

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

  const keyword = searchQuery.trim();
  if (searchResults.length === 0) {
    return <div className="px-3 py-2 text-xs text-muted-foreground">{t('projectExplorer.search.noResults')}</div>;
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
          'w-full border-b border-border/60 px-3 py-2 text-left transition-colors hover:bg-accent',
          isActiveHit && 'bg-accent',
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
          {hit.text ? renderHighlightedText(hit.text, keyword) : t('projectExplorer.search.emptyLine')}
        </div>
      </button>
    );
  });
};
