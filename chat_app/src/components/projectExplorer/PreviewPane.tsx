import React, { useEffect, useMemo, useRef, useState } from 'react';
import hljs from 'highlight.js';

import type {
  ChangeLogItem,
  CodeNavCapabilities,
  CodeNavDocumentSymbolsResult,
  CodeNavLocation,
  CodeNavLocationsResult,
  FsEntry,
  FsReadResult,
  ProjectSearchHit,
} from '../../types';
import { cn, formatFileSize } from '../../lib/utils';
import { DiffPanel } from './ChangeLogPanels';
import type {
  ProjectRunnerActiveTerminal,
  ProjectRunnerMember,
} from './useProjectExplorerRunState';
import {
  buildProjectSearchHitId,
  escapeHtml,
  getHighlightLanguage,
  splitTextByQuery,
} from './utils';

const isTokenChar = (value: string): boolean => /[A-Za-z0-9_$]/.test(value);

const extractTokenAtColumn = (
  lineText: string,
  column: number
): { token: string; column: number } | null => {
  if (!lineText) return null;
  const chars = Array.from(lineText);
  if (chars.length === 0) return null;

  let index = Math.max(0, Math.min(column - 1, chars.length - 1));
  if (!isTokenChar(chars[index]) && index > 0 && isTokenChar(chars[index - 1])) {
    index -= 1;
  }
  if (!isTokenChar(chars[index])) {
    return null;
  }

  let start = index;
  while (start > 0 && isTokenChar(chars[start - 1])) {
    start -= 1;
  }
  let end = index;
  while (end + 1 < chars.length && isTokenChar(chars[end + 1])) {
    end += 1;
  }

  return {
    token: chars.slice(start, end + 1).join(''),
    column: start + 1,
  };
};

const getColumnFromPointer = (
  lineNode: HTMLDivElement,
  event: React.MouseEvent<HTMLDivElement>
): number | null => {
  if (typeof document === 'undefined') {
    return null;
  }

  const doc = document as Document & {
    caretPositionFromPoint?: (
      x: number,
      y: number
    ) => { offsetNode: Node; offset: number } | null;
    caretRangeFromPoint?: (x: number, y: number) => Range | null;
  };

  const caretPosition = doc.caretPositionFromPoint?.(event.clientX, event.clientY);
  if (caretPosition?.offsetNode && lineNode.contains(caretPosition.offsetNode)) {
    const prefixRange = document.createRange();
    prefixRange.selectNodeContents(lineNode);
    prefixRange.setEnd(caretPosition.offsetNode, caretPosition.offset);
    return prefixRange.toString().length + 1;
  }

  const caretRange = doc.caretRangeFromPoint?.(event.clientX, event.clientY);
  if (caretRange?.startContainer && lineNode.contains(caretRange.startContainer)) {
    const prefixRange = document.createRange();
    prefixRange.selectNodeContents(lineNode);
    prefixRange.setEnd(caretRange.startContainer, caretRange.startOffset);
    return prefixRange.toString().length + 1;
  }

  return null;
};

interface ProjectPreviewPaneProps {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  loadingFile: boolean;
  error: string | null;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  activeSearchHitId: string | null;
  activeSearchHitIndex: number;
  totalSearchHits: number;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
  targetLine: number | null;
  targetLineRevision: number;
  navCapabilities: CodeNavCapabilities | null;
  navCapabilitiesLoading: boolean;
  navCapabilitiesError: string | null;
  selectedToken: string | null;
  selectedTokenLine: number | null;
  selectedTokenColumn: number | null;
  navResult: CodeNavLocationsResult | null;
  navRequestKind: 'definition' | 'references' | null;
  navLoading: boolean;
  navError: string | null;
  activeNavLocationId: string | null;
  documentSymbols: CodeNavDocumentSymbolsResult | null;
  documentSymbolsLoading: boolean;
  documentSymbolsError: string | null;
  selectedLog: ChangeLogItem | null;
  projectRootPath: string;
  runStatus: string;
  runCatalogLoading: boolean;
  runCatalogError: string | null;
  projectMembers: ProjectRunnerMember[];
  projectMembersLoading: boolean;
  projectMembersError: string | null;
  runnerScriptExists: boolean;
  runnerScriptChecking: boolean;
  runnerScriptPath: string;
  runnerStartCommand: string;
  runnerStopCommand: string;
  runnerRestartCommand: string;
  starting: boolean;
  stopping: boolean;
  restarting: boolean;
  runnerMessage: string | null;
  runnerError: string | null;
  activeRun: ProjectRunnerActiveTerminal | null;
  activeTerminalBusy: boolean;
  onTokenSelection: (selection: { token: string; line: number; column: number } | null) => void;
  onClearTokenSelection: () => void;
  onRequestDefinition: () => void;
  onRequestReferences: () => void;
  onSearchInProject: (query: string) => void;
  onOpenPreviousSearchHit: () => void;
  onOpenNextSearchHit: () => void;
  onActivateSearchHit: (hit: ProjectSearchHit) => void;
  onOpenNavLocation: (location: CodeNavLocation) => void;
  onOpenDocumentSymbol: (line: number) => void;
  onRunnerStart: () => void;
  onRunnerStop: () => void;
  onRunnerRestart: () => void;
  onRefreshRunnerState: () => void;
  onGenerateRunnerScriptForContact: (member: ProjectRunnerMember) => Promise<void>;
}

export const ProjectPreviewPane: React.FC<ProjectPreviewPaneProps> = ({
  selectedFile,
  selectedPath,
  selectedEntry,
  loadingFile,
  error,
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
  searchResults,
  activeSearchHitId,
  activeSearchHitIndex,
  totalSearchHits,
  canOpenPreviousSearchHit,
  canOpenNextSearchHit,
  targetLine,
  targetLineRevision,
  navCapabilities,
  navCapabilitiesError,
  selectedToken,
  navResult,
  navRequestKind,
  navLoading,
  navError,
  activeNavLocationId,
  documentSymbols,
  documentSymbolsLoading,
  documentSymbolsError,
  selectedLog,
  runStatus,
  runCatalogLoading,
  projectMembers,
  projectMembersLoading,
  runnerScriptExists,
  runnerScriptChecking,
  runnerScriptPath,
  runnerStartCommand,
  runnerStopCommand,
  runnerRestartCommand,
  starting,
  stopping,
  restarting,
  runnerMessage,
  runnerError,
  onTokenSelection,
  onClearTokenSelection,
  onRequestDefinition,
  onRequestReferences,
  onSearchInProject,
  onOpenPreviousSearchHit,
  onOpenNextSearchHit,
  onActivateSearchHit,
  onOpenNavLocation,
  onOpenDocumentSymbol,
  onRunnerStart,
  onRunnerStop,
  onRunnerRestart,
  onRefreshRunnerState,
  onGenerateRunnerScriptForContact,
}) => {
  const [memberPickerOpen, setMemberPickerOpen] = useState(false);
  const [memberPickerSelectedId, setMemberPickerSelectedId] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [generationError, setGenerationError] = useState<string | null>(null);
  const [generationMessage, setGenerationMessage] = useState<string | null>(null);
  const [documentSymbolsExpanded, setDocumentSymbolsExpanded] = useState(false);
  const lineRefMap = useRef<Record<number, HTMLDivElement | null>>({});
  const renderedFilePathRef = useRef<string | null>(null);
  const selectedFilePath = selectedFile?.path || null;

  if (renderedFilePathRef.current !== selectedFilePath) {
    lineRefMap.current = {};
    renderedFilePathRef.current = selectedFilePath;
  }

  useEffect(() => {
    setDocumentSymbolsExpanded(false);
  }, [selectedFilePath]);

  useEffect(() => {
    if (!selectedFile || selectedFile.isBinary || !targetLine || targetLine < 1) {
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

  const selectedMember = useMemo(
    () => projectMembers.find((member) => member.contactId === memberPickerSelectedId) || null,
    [memberPickerSelectedId, projectMembers]
  );
  const activeSearchQuery = searchQuery.trim();
  const rawLines = useMemo(
    () => (selectedFile && !selectedFile.isBinary ? selectedFile.content.split(/\r?\n/) : []),
    [selectedFile]
  );
  const fileSearchHitsByLine = useMemo(() => {
    const path = selectedFile?.path || selectedPath || '';
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
  }, [activeSearchQuery, searchResults, selectedFile?.path, selectedPath]);
  const displayedToken = selectedToken || navResult?.token || null;
  const canSearchInProject = Boolean(displayedToken?.trim());
  const activeSearchPositionLabel = totalSearchHits > 0
    ? `${activeSearchHitIndex >= 0 ? activeSearchHitIndex + 1 : 0} / ${totalSearchHits}`
    : null;
  const canNavigateToDefinition = Boolean(
    navCapabilities?.supportsDefinition || navCapabilities?.fallbackAvailable
  );
  const canNavigateToReferences = Boolean(
    navCapabilities?.supportsReferences || navCapabilities?.fallbackAvailable
  );
  const navResultLabel = useMemo(() => {
    if (!navResult || !navRequestKind) return null;
    if (navRequestKind === 'definition') return '定义结果';
    if (navRequestKind === 'references') return '引用结果';
    return '导航结果';
  }, [navRequestKind, navResult]);
  const documentSymbolCount = documentSymbols?.symbols?.length || 0;

  const handleLineMouseUp = (lineNumber: number, event: React.MouseEvent<HTMLDivElement>) => {
    if (typeof window === 'undefined' || typeof document === 'undefined') {
      return;
    }
    window.requestAnimationFrame(() => {
      const lineNode = lineRefMap.current[lineNumber];
      const selection = window.getSelection();
      if (!lineNode) {
        return;
      }

      if (selection && selection.rangeCount > 0) {
        const rawSelection = selection.toString();
        const token = rawSelection.trim();
        if (token && !rawSelection.includes('\n')) {
          const range = selection.getRangeAt(0);
          if (lineNode.contains(range.startContainer) && lineNode.contains(range.endContainer)) {
            const prefixRange = document.createRange();
            prefixRange.selectNodeContents(lineNode);
            prefixRange.setEnd(range.startContainer, range.startOffset);

            const lineText = rawLines[lineNumber - 1] ?? '';
            const leadingWhitespace = rawSelection.match(/^\s*/)?.[0].length ?? 0;
            const column = Math.max(
              1,
              Math.min(prefixRange.toString().length + leadingWhitespace + 1, lineText.length + 1)
            );

            onTokenSelection({
              token,
              line: lineNumber,
              column,
            });
            return;
          }
        }
      }

      const lineText = rawLines[lineNumber - 1] ?? '';
      const clickedColumn = getColumnFromPointer(lineNode, event);
      if (!clickedColumn) {
        return;
      }
      const extracted = extractTokenAtColumn(lineText, clickedColumn);
      if (!extracted) {
        return;
      }
      onTokenSelection({
        token: extracted.token,
        line: lineNumber,
        column: extracted.column,
      });
    });
  };

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
        return (
          <React.Fragment key={`${segment.text}-${index}`}>
            {segment.text}
          </React.Fragment>
        );
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
              : 'bg-amber-300/60 hover:bg-amber-300/85'
          )}
          title={`跳转到 L${hit.line}:C${hit.column}`}
        >
          {segment.text}
        </button>
      );
    });
  };

  const preview = useMemo(() => {
    if (loadingFile) {
      return <div className="p-4 text-sm text-muted-foreground">加载文件中...</div>;
    }
    if (!selectedFile) {
      if (selectedPath && !selectedEntry) {
        return (
          <div className="p-4 text-sm text-muted-foreground">
            该路径已删除或不存在，当前仅支持查看变更记录。
          </div>
        );
      }
      return <div className="p-4 text-sm text-muted-foreground">请选择文件以预览</div>;
    }
    const isImage = selectedFile.contentType.startsWith('image/');
    if (isImage && selectedFile.isBinary) {
      const src = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
      return (
        <div className="p-4 overflow-auto h-full">
          <img src={src} alt={selectedFile.name} className="max-w-full max-h-full rounded border border-border" />
        </div>
      );
    }
    if (!selectedFile.isBinary) {
      const language = getHighlightLanguage(selectedFile.name);
      let highlighted = '';
      try {
        if (language) {
          highlighted = hljs.highlight(selectedFile.content, { language }).value;
        } else {
          highlighted = hljs.highlightAuto(selectedFile.content).value;
        }
      } catch {
        highlighted = escapeHtml(selectedFile.content);
      }
      const lines = highlighted.split(/\r?\n/);
      return (
        <div className="h-full overflow-auto bg-muted/30">
          <div className="flex min-h-full text-sm">
            <div className="shrink-0 py-4 pr-3 pl-2 border-r border-border text-right text-muted-foreground select-none">
              {lines.map((_, idx) => (
                <div
                  key={idx}
                  className={cn(
                    'leading-5',
                    targetLine === idx + 1 && 'rounded bg-amber-500/15 px-1 text-amber-700 dark:text-amber-300'
                  )}
                >
                  {idx + 1}
                </div>
              ))}
            </div>
            <div className="flex-1 min-w-0 py-4 pl-3 pr-4 hljs">
              {lines.map((line, idx) => {
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
                      'leading-5 font-mono whitespace-pre w-full cursor-text',
                      targetLine === lineNumber && 'rounded bg-amber-500/10 shadow-[inset_3px_0_0_rgba(245,158,11,0.9)]'
                    )}
                    {...(shouldHighlightSearchMatch
                      ? {}
                      : { dangerouslySetInnerHTML: { __html: line || '&nbsp;' } })}
                  >
                    {shouldHighlightSearchMatch
                      ? (rawLineText.length > 0 ? renderSearchHighlightedLine(rawLineText, lineHits) : '\u00A0')
                      : null}
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      );
    }
    const downloadHref = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
    return (
      <div className="p-4 text-sm text-muted-foreground space-y-2">
        <div>该文件为二进制内容，暂不支持直接预览。</div>
        <a
          href={downloadHref}
          download={selectedFile.name || 'file'}
          className="inline-flex items-center px-3 py-1.5 rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
        >
          下载文件
        </a>
      </div>
    );
  }, [
    activeSearchQuery,
    activeSearchHitId,
    fileSearchHitsByLine,
    handleLineMouseUp,
    loadingFile,
    onActivateSearchHit,
    rawLines,
    searchCaseSensitive,
    searchWholeWord,
    selectedEntry,
    selectedFile,
    selectedPath,
    targetLine,
  ]);

  const mergeError = runnerError || generationError;
  const mergeMessage = !mergeError ? (generationMessage || runnerMessage) : null;

  const handleGenerateForMember = async (member: ProjectRunnerMember): Promise<boolean> => {
    setGenerating(true);
    setGenerationError(null);
    setGenerationMessage(null);
    try {
      await onGenerateRunnerScriptForContact(member);
      setGenerationMessage(`已向 ${member.name || member.contactId} 发送脚本生成任务`);
      await onRefreshRunnerState();
      return true;
    } catch (error) {
      setGenerationError(error instanceof Error ? error.message : '发送脚本生成任务失败');
      return false;
    } finally {
      setGenerating(false);
    }
  };

  const handleGenerateClick = () => {
    setGenerationError(null);
    setGenerationMessage(null);
    if (projectMembersLoading) {
      return;
    }
    if (projectMembers.length === 0) {
      setGenerationError('当前项目还没有团队成员，请先添加联系人');
      return;
    }
    if (projectMembers.length === 1) {
      void handleGenerateForMember(projectMembers[0]);
      return;
    }
    setMemberPickerSelectedId(projectMembers[0]?.contactId || null);
    setMemberPickerOpen(true);
  };

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="px-4 py-2 border-b border-border bg-card flex items-center justify-between">
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium text-foreground truncate">
            {selectedFile?.name || (selectedPath ? '文件预览（当前项不可预览）' : '文件预览')}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            {selectedFile?.path || selectedPath || '请选择文件'}
          </div>
        </div>
        <div className="ml-3 flex items-center gap-2">
          {!runnerScriptExists ? (
            <button
              type="button"
              onClick={handleGenerateClick}
              disabled={generating || projectMembersLoading || runnerScriptChecking}
              className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              title="向团队成员发送固定提示词，生成项目启动脚本"
            >
              {generating ? '生成请求中...' : '生成启动脚本'}
            </button>
          ) : (
            <>
              <button
                type="button"
                onClick={() => { void onRunnerStart(); }}
                disabled={runStatus === 'no_member' || runStatus === 'missing_root' || starting || stopping || restarting || generating || runnerScriptChecking}
                className="h-8 rounded border border-emerald-500/40 px-3 text-xs text-emerald-700 hover:bg-emerald-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
                title={runnerStartCommand}
              >
                {starting ? '启动中...' : '启动'}
              </button>
              <button
                type="button"
                onClick={() => { void onRunnerStop(); }}
                disabled={runStatus === 'no_member' || runStatus === 'missing_root' || starting || stopping || restarting || generating || runnerScriptChecking}
                className="h-8 rounded border border-rose-500/40 px-3 text-xs text-rose-700 hover:bg-rose-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
                title={runnerStopCommand}
              >
                {stopping ? '停止中...' : '停止'}
              </button>
              <button
                type="button"
                onClick={() => { void onRunnerRestart(); }}
                disabled={runStatus === 'no_member' || runStatus === 'missing_root' || starting || stopping || restarting || generating || runnerScriptChecking}
                className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
                title={runnerRestartCommand}
              >
                {restarting ? '重启中...' : '重启'}
              </button>
            </>
          )}
          <button
            type="button"
            onClick={() => { void onRefreshRunnerState(); }}
            disabled={runCatalogLoading || runnerScriptChecking || projectMembersLoading}
            className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {runnerScriptChecking ? '检查中...' : '刷新状态'}
          </button>
          {selectedFile && (
            <div className="text-[11px] text-muted-foreground whitespace-nowrap">
              {formatFileSize(selectedFile.size)}
            </div>
          )}
        </div>
      </div>
      {(mergeMessage || mergeError) && (
        <div className="px-4 py-1.5 border-b border-border/70 bg-card">
          <div className={mergeError ? 'text-[11px] text-destructive' : 'text-[11px] text-emerald-600'}>
            {mergeError || mergeMessage}
          </div>
        </div>
      )}
      <div className="flex-1 overflow-hidden flex flex-col">
        {selectedFile && !selectedFile.isBinary && (
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
                    className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    上一处
                  </button>
                  <button
                    type="button"
                    onClick={onOpenNextSearchHit}
                    disabled={!canOpenNextSearchHit}
                    className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
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
                  className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {navLoading && navRequestKind === 'definition' ? '查询中...' : '跳到定义'}
                </button>
              )}
              {canNavigateToReferences && (
                <button
                  type="button"
                  onClick={onRequestReferences}
                  disabled={!selectedToken || navLoading}
                  className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {navLoading && navRequestKind === 'references' ? '查询中...' : '查找引用'}
                </button>
              )}
              <button
                type="button"
                onClick={() => {
                  if (displayedToken) {
                    onSearchInProject(displayedToken);
                  }
                }}
                disabled={!canSearchInProject}
                className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                项目内搜索
              </button>
              <button
                type="button"
                onClick={onClearTokenSelection}
                disabled={!selectedToken && !navResult && !navError}
                className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
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
                onClick={() => setDocumentSymbolsExpanded((value) => !value)}
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
                        isActiveLocation && 'bg-accent'
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
                            isActiveSymbol && 'bg-accent'
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
                  <div className="px-3 py-2 text-[11px] text-muted-foreground">当前文件没有提取到可导航符号</div>
                )}
              </div>
            )}
          </div>
        )}
        <DiffPanel selectedLog={selectedLog} />
        <div className="flex-1 min-h-0 overflow-hidden">
          {error ? (
            <div className="p-4 text-sm text-destructive">{error}</div>
          ) : (
            preview
          )}
        </div>
      </div>

      {memberPickerOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            className="absolute inset-0 bg-black/50"
            onClick={() => {
              if (generating) return;
              setMemberPickerOpen(false);
            }}
            aria-label="关闭成员选择"
          />
          <div className="relative w-[520px] max-w-[calc(100vw-24px)] rounded-lg border border-border bg-card p-5 shadow-xl">
            <div className="mb-1 text-base font-semibold text-foreground">选择执行成员</div>
            <div className="mb-3 text-xs text-muted-foreground">
              请选择一个团队成员来生成 `${runnerScriptPath}`。
            </div>
            <div className="max-h-72 overflow-y-auto rounded border border-border">
              {projectMembers.map((member) => {
                const active = member.contactId === memberPickerSelectedId;
                return (
                  <button
                    key={member.contactId}
                    type="button"
                    onClick={() => setMemberPickerSelectedId(member.contactId)}
                    className={`w-full border-b border-border px-3 py-2 text-left last:border-b-0 ${active ? 'bg-accent' : 'hover:bg-accent/50'}`}
                  >
                    <div className="text-sm text-foreground truncate">{member.name || member.contactId}</div>
                    <div className="text-[11px] text-muted-foreground truncate">{member.agentId}</div>
                  </button>
                );
              })}
            </div>
            {generationError && (
              <div className="mt-3 text-xs text-destructive">{generationError}</div>
            )}
            <div className="mt-4 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => {
                  if (generating) return;
                  setMemberPickerOpen(false);
                }}
                disabled={generating}
                className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => {
                  if (!selectedMember) return;
                  void handleGenerateForMember(selectedMember).then((success) => {
                    if (success) {
                      setMemberPickerOpen(false);
                    }
                  });
                }}
                disabled={!selectedMember || generating}
                className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {generating ? '提交中...' : '确认并执行'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
