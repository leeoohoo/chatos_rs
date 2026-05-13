import React from 'react';

import { cn } from '../../../lib/utils';
import type {
  CodeNavDocumentSymbolsResult,
  CodeNavLocation,
  CodeNavLocationsResult,
} from '../../../types';

interface ProjectPreviewNavigationProps {
  displayedToken: string | null;
  activeSearchQuery: string;
  activeSearchPositionLabel: string | null;
  totalSearchHits: number;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
  canNavigateToDefinition: boolean;
  canNavigateToReferences: boolean;
  selectedToken: string | null;
  navLoading: boolean;
  navRequestKind: 'definition' | 'references' | null;
  navResult: CodeNavLocationsResult | null;
  navResultLabel: string | null;
  navCapabilitiesError: string | null;
  navError: string | null;
  activeNavLocationId: string | null;
  canGoBackFromNav: boolean;
  documentSymbolsExpanded: boolean;
  documentSymbolsLoading: boolean;
  documentSymbolsError: string | null;
  documentSymbolCount: number;
  documentSymbols: CodeNavDocumentSymbolsResult | null;
  targetLine: number | null;
  onToggleDocumentSymbols: () => void;
  onOpenPreviousSearchHit: () => void;
  onOpenNextSearchHit: () => void;
  onRequestDefinition: () => void;
  onRequestReferences: () => void;
  onGoBackFromNav: () => void;
  onSearchInProject: (query: string) => void;
  onClearTokenSelection: () => void;
  onOpenNavLocation: (location: CodeNavLocation) => void;
  onOpenDocumentSymbol: (line: number) => void;
}

export const ProjectPreviewNavigation: React.FC<ProjectPreviewNavigationProps> = ({
  displayedToken,
  activeSearchQuery,
  activeSearchPositionLabel,
  totalSearchHits,
  canOpenPreviousSearchHit,
  canOpenNextSearchHit,
  canNavigateToDefinition,
  canNavigateToReferences,
  selectedToken,
  navLoading,
  navRequestKind,
  navResult,
  navResultLabel,
  navCapabilitiesError,
  navError,
  activeNavLocationId,
  canGoBackFromNav,
  documentSymbolsExpanded,
  documentSymbolsLoading,
  documentSymbolsError,
  documentSymbolCount,
  documentSymbols,
  targetLine,
  onToggleDocumentSymbols,
  onOpenPreviousSearchHit,
  onOpenNextSearchHit,
  onRequestDefinition,
  onRequestReferences,
  onGoBackFromNav,
  onSearchInProject,
  onClearTokenSelection,
  onOpenNavLocation,
  onOpenDocumentSymbol,
}) => {
  const canSearchInProject = Boolean(displayedToken?.trim());
  const canClearNavigation = Boolean(selectedToken || navResult || navError);

  return (
    <div className="border-b border-border/70 bg-card/60 px-3 py-1.5">
      <div className="flex flex-wrap items-center gap-2">
        {displayedToken && (
          <span
            className="max-w-48 truncate rounded border border-border px-2 py-1 text-[11px] text-muted-foreground"
            title={displayedToken}
          >
            {displayedToken}
          </span>
        )}
        {activeSearchQuery && totalSearchHits > 0 && (
          <>
            <span className="rounded border border-border px-2 py-1 text-[11px] text-muted-foreground">
              搜索命中 {activeSearchPositionLabel}
            </span>
            <button
              type="button"
              onClick={onOpenPreviousSearchHit}
              disabled={!canOpenPreviousSearchHit}
              className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
            >
              上一处
            </button>
            <button
              type="button"
              onClick={onOpenNextSearchHit}
              disabled={!canOpenNextSearchHit}
              className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
            >
              下一处
            </button>
          </>
        )}
        {canNavigateToDefinition && (
          <button
            type="button"
            onClick={onRequestDefinition}
            disabled={!selectedToken || navLoading}
            className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
          >
            {navLoading && navRequestKind === 'definition' ? '查询中...' : '跳到定义'}
          </button>
        )}
        {canNavigateToReferences && (
          <button
            type="button"
            onClick={onRequestReferences}
            disabled={!selectedToken || navLoading}
            className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
          >
            {navLoading && navRequestKind === 'references' ? '查询中...' : '查找引用'}
          </button>
        )}
        <button
          type="button"
          onClick={onGoBackFromNav}
          disabled={!canGoBackFromNav || navLoading}
          className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          返回上一处
        </button>
        <button
          type="button"
          onClick={() => {
            if (displayedToken) {
              onSearchInProject(displayedToken);
            }
          }}
          disabled={!canSearchInProject}
          className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          项目内搜索
        </button>
        <button
          type="button"
          onClick={onClearTokenSelection}
          disabled={!canClearNavigation}
          className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          清空导航
        </button>
        {navResultLabel && navResult && (
          <span className="rounded border border-border px-2 py-1 text-[11px] text-muted-foreground">
            {navResultLabel} {navResult.locations.length} 条 · {navResult.mode}
          </span>
        )}
        <button
          type="button"
          aria-expanded={documentSymbolsExpanded}
          onClick={onToggleDocumentSymbols}
          className="ml-auto inline-flex h-8 items-center gap-2 rounded border border-border px-3 text-xs hover:bg-accent"
        >
          <span>{documentSymbolsExpanded ? 'v' : '>'} 文件符号</span>
          <span className="text-[11px] text-muted-foreground">
            {documentSymbolsLoading
              ? '加载中'
              : documentSymbolsError
                ? '加载失败'
                : documentSymbolCount}
          </span>
        </button>
      </div>
      {(navCapabilitiesError || navError) && (
        <div className="mt-1.5 text-[11px] text-destructive">
          {navCapabilitiesError || navError}
        </div>
      )}
      {navResult && navResult.locations.length > 0 && (
        <div className="mt-1.5 max-h-40 overflow-y-auto rounded border border-border bg-background">
          {navResult.locations.map((location) => {
            const locationId = `${location.path}:${location.line}:${location.column}:${location.endLine}:${location.endColumn}`;
            const isActiveLocation = activeNavLocationId === locationId;
            return (
              <button
                key={locationId}
                type="button"
                onClick={() => {
                  onOpenNavLocation(location);
                }}
                className={cn(
                  'flex w-full flex-col border-b border-border px-3 py-2 text-left last:border-b-0 hover:bg-accent',
                  isActiveLocation && 'bg-accent',
                )}
                title={`${location.relativePath}:${location.line}`}
              >
                <span className="text-[11px] text-foreground">
                  {location.relativePath} · L{location.line}
                </span>
                <span className="mt-1 whitespace-pre-wrap break-all font-mono text-xs text-muted-foreground">
                  {location.preview || '(无预览)'}
                </span>
              </button>
            );
          })}
        </div>
      )}
      {documentSymbolsExpanded && (
        <div className="mt-1.5 rounded border border-border bg-background">
          {documentSymbolsLoading ? (
            <div className="px-3 py-2 text-[11px] text-muted-foreground">正在加载文件符号...</div>
          ) : documentSymbolsError ? (
            <div className="px-3 py-2 text-[11px] text-destructive">{documentSymbolsError}</div>
          ) : documentSymbols?.symbols && documentSymbols.symbols.length > 0 ? (
            <div className="max-h-40 overflow-y-auto">
              {documentSymbols.symbols.map((symbol) => {
                const symbolId = `${symbol.kind}:${symbol.name}:${symbol.line}:${symbol.column}`;
                const isActiveSymbol = targetLine === symbol.line;
                return (
                  <button
                    key={symbolId}
                    type="button"
                    onClick={() => {
                      onOpenDocumentSymbol(symbol.line);
                    }}
                    className={cn(
                      'flex w-full items-center justify-between border-b border-border px-3 py-2 text-left last:border-b-0 hover:bg-accent',
                      isActiveSymbol && 'bg-accent',
                    )}
                    title={`${symbol.kind} · L${symbol.line}`}
                  >
                    <span className="min-w-0 truncate text-[11px] text-foreground">
                      {symbol.name}
                    </span>
                    <span className="ml-3 shrink-0 text-[11px] text-muted-foreground">
                      {symbol.kind} · L{symbol.line}
                    </span>
                  </button>
                );
              })}
            </div>
          ) : (
            <div className="px-3 py-2 text-[11px] text-muted-foreground">
              当前文件没有提取到可导航符号
            </div>
          )}
        </div>
      )}
    </div>
  );
};
