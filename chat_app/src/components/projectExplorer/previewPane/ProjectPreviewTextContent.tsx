import React, { useCallback, useEffect, useMemo, useRef } from 'react';

import { LazyMarkdownRenderer } from '../../LazyMarkdownRenderer';
import { highlightCodeBlock, highlightCodeBlockAuto } from '../../../lib/tools/highlight';
import { cn } from '../../../lib/utils';
import type { FsReadResult, ProjectSearchHit } from '../../../types';
import {
  buildProjectSearchHitId,
  escapeHtml,
  getHighlightLanguage,
  isMarkdownFile,
  splitTextByQuery,
} from '../utils';
import type { PreviewTokenSelection } from './previewPaneTypes';
import { usePreviewTextTokenSelection } from './usePreviewTextTokenSelection';

interface ProjectPreviewTextContentProps {
  selectedFile: FsReadResult;
  selectedPath: string | null;
  isEditing: boolean;
  draftContent: string;
  saveError: string | null;
  savingFile: boolean;
  targetLine: number | null;
  targetLineRevision: number;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  activeSearchHitId: string | null;
  onActivateSearchHit: (hit: ProjectSearchHit) => void;
  onTokenSelection: (selection: PreviewTokenSelection | null) => void;
  onDraftContentChange: (value: string) => void;
  onSaveDraft: () => Promise<boolean>;
}

const highlightTextContent = (filename: string, content: string): string => {
  const language = getHighlightLanguage(filename);
  try {
    if (language) {
      return highlightCodeBlock(content, language).value;
    }
    return highlightCodeBlockAuto(content).value;
  } catch {
    return escapeHtml(content);
  }
};

const buildFileSearchHitsByLine = ({
  activeSearchQuery,
  searchResults,
  selectedFilePath,
  selectedPath,
}: {
  activeSearchQuery: string;
  searchResults: ProjectSearchHit[];
  selectedFilePath: string | null;
  selectedPath: string | null;
}): Map<number, ProjectSearchHit[]> => {
  const path = selectedFilePath || selectedPath || '';
  const map = new Map<number, ProjectSearchHit[]>();
  if (!path || !activeSearchQuery) {
    return map;
  }

  searchResults.forEach((hit) => {
    if (hit.path !== path) {
      return;
    }
    const existing = map.get(hit.line) || [];
    existing.push(hit);
    map.set(hit.line, existing);
  });
  map.forEach((hits) => {
    hits.sort((left, right) => left.column - right.column);
  });
  return map;
};

export const ProjectPreviewTextContent: React.FC<ProjectPreviewTextContentProps> = ({
  selectedFile,
  selectedPath,
  isEditing,
  draftContent,
  saveError,
  savingFile,
  targetLine,
  targetLineRevision,
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  activeSearchHitId,
  onActivateSearchHit,
  onTokenSelection,
  onDraftContentChange,
  onSaveDraft,
}) => {
  const lineRefMap = useRef<Record<number, HTMLDivElement | null>>({});
  const renderedFilePathRef = useRef<string | null>(null);
  const editorTextareaRef = useRef<HTMLTextAreaElement | null>(null);
  const editorBackdropRef = useRef<HTMLDivElement | null>(null);
  const editorLineNumbersRef = useRef<HTMLDivElement | null>(null);
  const selectedFilePath = selectedFile.path || null;
  const editorTextRenderStyle = useMemo<React.CSSProperties>(() => ({
    fontVariantLigatures: 'none',
    fontFeatureSettings: '"liga" 0, "calt" 0',
    WebkitFontSmoothing: 'auto',
  }), []);

  if (renderedFilePathRef.current !== selectedFilePath) {
    lineRefMap.current = {};
    renderedFilePathRef.current = selectedFilePath;
  }

  useEffect(() => {
    if (isEditing || selectedFile.isBinary || !targetLine || targetLine < 1) {
      return;
    }

    const scrollToTargetLine = () => {
      const target = lineRefMap.current[targetLine];
      if (!target) {
        return;
      }
      target.scrollIntoView({
        block: 'center',
        inline: 'nearest',
        behavior: 'smooth',
      });
    };

    if (typeof window === 'undefined') {
      scrollToTargetLine();
      return;
    }

    const frame = window.requestAnimationFrame(scrollToTargetLine);
    return () => {
      window.cancelAnimationFrame(frame);
    };
  }, [selectedFile, selectedFilePath, targetLine, targetLineRevision]);

  const syncEditorScrollOffsets = useCallback((scrollTop: number, scrollLeft: number) => {
    if (editorBackdropRef.current) {
      editorBackdropRef.current.style.transform = `translate(${-scrollLeft}px, ${-scrollTop}px)`;
    }
    if (editorLineNumbersRef.current) {
      editorLineNumbersRef.current.style.transform = `translateY(${-scrollTop}px)`;
    }
  }, []);

  useEffect(() => {
    if (!isEditing) {
      return;
    }
    if (editorTextareaRef.current) {
      editorTextareaRef.current.scrollTop = 0;
      editorTextareaRef.current.scrollLeft = 0;
    }
    syncEditorScrollOffsets(0, 0);
  }, [isEditing, selectedFilePath, syncEditorScrollOffsets]);

  const activeSearchQuery = searchQuery.trim();
  const displayedContent = isEditing ? draftContent : selectedFile.content;
  const renderMarkdownPreview = !isEditing && isMarkdownFile(selectedFile.name, selectedFile.contentType);
  const rawLines = useMemo(
    () => displayedContent.split(/\r?\n/),
    [displayedContent],
  );
  const highlightedLines = useMemo(
    () => (
      renderMarkdownPreview
        ? []
        : highlightTextContent(selectedFile.name, displayedContent).split(/\r?\n/)
    ),
    [displayedContent, renderMarkdownPreview, selectedFile.name],
  );
  const fileSearchHitsByLine = useMemo(
    () => buildFileSearchHitsByLine({
      activeSearchQuery,
      searchResults,
      selectedFilePath,
      selectedPath,
    }),
    [activeSearchQuery, searchResults, selectedFilePath, selectedPath],
  );
  const { handleLineMouseUp } = usePreviewTextTokenSelection({
    lineRefMap,
    rawLines,
    onTokenSelection,
  });

  if (isEditing) {
    return (
      <div className="flex h-full flex-col bg-muted/30">
        {saveError && (
          <div className="border-b border-border bg-destructive/5 px-4 py-2 text-xs text-destructive">
            {saveError}
          </div>
        )}
        <div className="flex min-h-0 flex-1 text-sm">
          <div className="shrink-0 overflow-hidden border-r border-border bg-muted/40 text-right text-muted-foreground">
            <div ref={editorLineNumbersRef} className="py-4 pl-2 pr-3">
              {rawLines.map((_, idx) => (
                <div key={idx} className="leading-5">
                  {idx + 1}
                </div>
              ))}
            </div>
          </div>
          <div className="relative min-w-0 flex-1 overflow-hidden bg-background">
            <div aria-hidden className="pointer-events-none absolute inset-0 overflow-hidden">
              <div
                ref={editorBackdropRef}
                className="min-w-max py-4 pl-3 pr-4 font-mono text-sm leading-5 text-[#24292f] dark:text-[#c9d1d9]"
                style={editorTextRenderStyle}
              >
                {highlightedLines.map((line, idx) => (
                  <div
                    key={idx}
                    className="whitespace-pre"
                    dangerouslySetInnerHTML={{ __html: line || '&nbsp;' }}
                  />
                ))}
              </div>
            </div>
            <textarea
              ref={editorTextareaRef}
              value={draftContent}
              onChange={(event) => onDraftContentChange(event.target.value)}
              onScroll={(event) => {
                syncEditorScrollOffsets(
                  event.currentTarget.scrollTop,
                  event.currentTarget.scrollLeft,
                );
              }}
              onKeyDown={(event) => {
                if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
                  event.preventDefault();
                  void onSaveDraft();
                }
              }}
              wrap="off"
              spellCheck={false}
              disabled={savingFile}
              className="relative z-10 h-full w-full resize-none overflow-auto border-0 bg-transparent px-3 py-4 font-mono text-sm leading-5 text-transparent outline-none disabled:cursor-not-allowed"
              style={{
                ...editorTextRenderStyle,
                caretColor: 'hsl(var(--foreground))',
                WebkitTextFillColor: 'transparent',
              }}
            />
          </div>
        </div>
      </div>
    );
  }

  if (renderMarkdownPreview) {
    return (
      <div className="h-full overflow-auto bg-muted/30">
        <div className="min-h-full px-4 py-4 text-sm">
          <LazyMarkdownRenderer content={selectedFile.content} className="not-prose" />
        </div>
      </div>
    );
  }

  const renderSearchHighlightedLine = (
    lineText: string,
    lineHits: ProjectSearchHit[],
  ): React.ReactNode => {
    let hitCursor = 0;
    return splitTextByQuery(lineText, activeSearchQuery, {
      caseSensitive: searchCaseSensitive,
      wholeWord: searchWholeWord,
    }).map((segment, index) => {
      if (!segment.matched) {
        return <React.Fragment key={`${segment.text}-${index}`}>{segment.text}</React.Fragment>;
      }

      const hit = lineHits[hitCursor] || null;
      hitCursor += 1;
      const isActive = hit ? buildProjectSearchHitId(hit) === activeSearchHitId : false;
      if (!hit) {
        return (
          <mark
            key={`${segment.text}-${index}`}
            className="rounded bg-amber-300/60 px-0.5 text-inherit"
          >
            {segment.text}
          </mark>
        );
      }

      return (
        <button
          key={`${segment.text}-${index}-${hit.column}`}
          type="button"
          onMouseDown={(event) => {
            event.preventDefault();
            event.stopPropagation();
          }}
          onMouseUp={(event) => {
            event.preventDefault();
            event.stopPropagation();
          }}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onActivateSearchHit(hit);
          }}
          className={cn(
            'rounded px-0.5 font-inherit text-inherit transition-colors',
            isActive
              ? 'bg-amber-500/80 text-black shadow-[0_0_0_1px_rgba(245,158,11,0.65)]'
              : 'bg-amber-300/60 hover:bg-amber-300/85',
          )}
          title={`跳转到 L${hit.line}:C${hit.column}`}
        >
          {segment.text}
        </button>
      );
    });
  };

  return (
    <div className="h-full overflow-auto bg-muted/30">
      <div className="flex min-h-full text-sm">
        <div className="shrink-0 select-none border-r border-border py-4 pl-2 pr-3 text-right text-muted-foreground">
          {highlightedLines.map((_, idx) => (
            <div
              key={idx}
              className={cn(
                'leading-5',
                targetLine === idx + 1
                  && 'rounded bg-amber-500/15 px-1 text-amber-700 dark:text-amber-300',
              )}
            >
              {idx + 1}
            </div>
          ))}
        </div>
        <div className="hljs min-w-0 flex-1 py-4 pl-3 pr-4">
          {highlightedLines.map((line, idx) => {
            const lineNumber = idx + 1;
            const rawLineText = rawLines[idx] ?? '';
            const lineHits = fileSearchHitsByLine.get(lineNumber) || [];
            const shouldHighlightSearchMatch = lineHits.length > 0;

            return (
              <div
                key={idx}
                ref={(node) => {
                  lineRefMap.current[lineNumber] = node;
                }}
                onMouseUp={(event) => {
                  handleLineMouseUp(lineNumber, event);
                }}
                className={cn(
                  'w-full cursor-text whitespace-pre font-mono leading-5',
                  targetLine === lineNumber
                    && 'rounded bg-amber-500/10 shadow-[inset_3px_0_0_rgba(245,158,11,0.9)]',
                )}
                {...(shouldHighlightSearchMatch
                  ? {}
                  : { dangerouslySetInnerHTML: { __html: line || '&nbsp;' } })}
              >
                {shouldHighlightSearchMatch
                  ? (rawLineText.length > 0
                    ? renderSearchHighlightedLine(rawLineText, lineHits)
                    : '\u00A0')
                  : null}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
};
