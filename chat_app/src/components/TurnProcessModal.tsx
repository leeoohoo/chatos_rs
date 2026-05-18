import React from 'react';
import { useEffect, useRef } from 'react';

import type ApiClient from '../lib/api/client';
import type { Message } from '../types';
import { getToolDisplayName } from '../lib/tools/displayName';
import { LazyMarkdownRenderer } from './LazyMarkdownRenderer';
import { ToolCallRenderer } from './ToolCallRenderer';
import { useTurnProcessViewerModel } from './turnProcessViewer/useTurnProcessViewerModel';
import { useI18n } from '../i18n/I18nProvider';

interface TurnProcessModalProps {
  open: boolean;
  sessionId: string | null;
  userMessageId: string | null;
  turnId?: string | null;
  messages: Message[];
  cachedProcessMessages?: Record<string, Message[]> | null;
  apiClient: ApiClient;
  onClose: () => void;
}

const MODAL_TITLE_ID = 'turn-process-modal-title';
const MODAL_DESC_ID = 'turn-process-modal-description';
const STREAM_FOLLOW_BOTTOM_THRESHOLD = 72;
type TimelineItem = ReturnType<typeof useTurnProcessViewerModel>['timelineItems'][number];
type ToolCallTimelineItem = Extract<TimelineItem, { kind: 'tool_call' }>;

const formatTime = (value: Date | null): string => {
  if (!value) {
    return '';
  }
  return value.toLocaleString();
};

const buildUserMessageTitle = (content: string): string => {
  const normalized = typeof content === 'string' ? content.replace(/\s+/g, ' ').trim() : '';
  if (!normalized) {
    return '过程详情';
  }
  return normalized.length > 72 ? `${normalized.slice(0, 72)}...` : normalized;
};

const hasToolResult = (value: unknown): boolean => {
  if (value === null || value === undefined) {
    return false;
  }
  if (typeof value === 'string') {
    return value.trim().length > 0;
  }
  return true;
};

const getTimelineToolName = (rawName: string | undefined, fallbackLabel: string): string => {
  if (typeof rawName !== 'string' || rawName.trim().length === 0) {
    return fallbackLabel;
  }
  const displayName = getToolDisplayName(rawName);
  return displayName === 'unknown_tool' ? fallbackLabel : displayName;
};

const getTimelineItemTitle = (
  item: TimelineItem,
  labels: {
    thinking: string;
    toolLabel: string;
    unavailableToolLabel: string;
    unnamedTool: string;
  },
): string => {
  if (item.kind === 'thinking') {
    return labels.thinking;
  }
  if (item.kind === 'tool_call') {
    return `${labels.toolLabel} · ${getTimelineToolName(item.toolCall.name, labels.unnamedTool)}`;
  }
  return `${labels.unavailableToolLabel} · ${item.entry.serverName}_${item.entry.toolName}`;
};

const getToolCallFinalResult = (toolCall: ToolCallTimelineItem['toolCall']): unknown => (
  (toolCall as { finalResult?: unknown }).finalResult
);

const isToolCallSettled = (item: ToolCallTimelineItem): boolean => {
  if (item.toolCall.error || item.completed) {
    return true;
  }

  const finalResult = getToolCallFinalResult(item.toolCall);
  if (hasToolResult(finalResult)) {
    return true;
  }

  if (!item.streamLog) {
    return hasToolResult(item.toolCall.result);
  }

  if (!hasToolResult(item.toolCall.result)) {
    return false;
  }

  if (typeof item.toolCall.result !== 'string') {
    return true;
  }

  return item.toolCall.result.trim() !== item.streamLog.trim();
};

const shouldShowToolStreamLog = (item: ToolCallTimelineItem): boolean => (
  item.streamLog.trim().length > 0 && !isToolCallSettled(item)
);

const isNearBottom = (element: HTMLDivElement | null): boolean => {
  if (!element) {
    return true;
  }
  const distanceToBottom = element.scrollHeight - element.scrollTop - element.clientHeight;
  return distanceToBottom <= STREAM_FOLLOW_BOTTOM_THRESHOLD;
};

const getTimelineItemStatus = (
  item: TimelineItem,
): { label: string; className: string } => {
  if (item.kind === 'thinking') {
    return item.isStreaming
      ? {
        label: '进行中',
        className: 'bg-sky-100 text-sky-700 dark:bg-sky-950/40 dark:text-sky-300',
      }
      : {
        label: '已记录',
        className: 'bg-muted text-muted-foreground',
      };
  }

  if (item.kind === 'tool_unavailable') {
    return {
      label: '不可用',
      className: 'bg-amber-100 text-amber-800 dark:bg-amber-950/40 dark:text-amber-200',
    };
  }

  if (item.toolCall.error) {
    return {
      label: '失败',
      className: 'bg-rose-100 text-rose-700 dark:bg-rose-950/40 dark:text-rose-300',
    };
  }

  if (isToolCallSettled(item)) {
    return {
      label: '已完成',
      className: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300',
    };
  }

  if (item.streamLog) {
    return {
      label: '执行中',
      className: 'bg-sky-100 text-sky-700 dark:bg-sky-950/40 dark:text-sky-300',
    };
  }

  return {
    label: '等待中',
    className: 'bg-muted text-muted-foreground',
  };
};

const getTimelineOverview = (items: TimelineItem[]): {
  total: number;
  running: number;
  completed: number;
  failed: number;
  unavailable: number;
} => items.reduce((acc, item) => {
  acc.total += 1;

  if (item.kind === 'tool_unavailable') {
    acc.unavailable += 1;
    return acc;
  }

  const status = getTimelineItemStatus(item).label;
  if (status === '失败') {
    acc.failed += 1;
    return acc;
  }
  if (status === '已完成' || status === '已记录') {
    acc.completed += 1;
    return acc;
  }
  if (status === '进行中' || status === '执行中' || status === '等待中') {
    acc.running += 1;
  }

  return acc;
}, {
  total: 0,
  running: 0,
  completed: 0,
  failed: 0,
  unavailable: 0,
});

const isRunningStatusLabel = (label: string): boolean => (
  label === '进行中' || label === '执行中'
);

export const TurnProcessModal: React.FC<TurnProcessModalProps> = ({
  open,
  sessionId,
  userMessageId,
  turnId = null,
  messages,
  cachedProcessMessages,
  apiClient,
  onClose,
}) => {
  const { t } = useI18n();
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);
  const scrollContainerRef = useRef<HTMLDivElement | null>(null);
  const [followStreaming, setFollowStreaming] = React.useState(true);
  const [pendingUpdateCount, setPendingUpdateCount] = React.useState(0);
  const previousStreamingActivityKeyRef = React.useRef('');
  const {
    userMessage,
    timelineItems,
    stats,
    loading,
    error,
    isStreaming,
  } = useTurnProcessViewerModel({
    open,
    sessionId,
    userMessageId,
    turnId,
    messages,
    cachedProcessMessages,
    apiClient,
  });
  const streamingActivityKey = React.useMemo(() => timelineItems.map((item) => {
    if (item.kind === 'thinking') {
      return `${item.id}:${item.text.length}:${item.isStreaming ? '1' : '0'}`;
    }
    if (item.kind === 'tool_call') {
      const finalResult = getToolCallFinalResult(item.toolCall);
      return `${item.id}:${item.streamLog.length}:${item.completed ? '1' : '0'}:${hasToolResult(item.toolCall.result) ? '1' : '0'}:${hasToolResult(finalResult) ? '1' : '0'}`;
    }
    return `${item.id}:${item.entry.reason.length}`;
  }).join('|'), [timelineItems]);
  const timelineOverview = React.useMemo(
    () => getTimelineOverview(timelineItems),
    [timelineItems],
  );
  const primaryActiveItemId = React.useMemo(() => {
    for (let index = timelineItems.length - 1; index >= 0; index -= 1) {
      const item = timelineItems[index];
      if (!item) {
        continue;
      }
      if (isRunningStatusLabel(getTimelineItemStatus(item).label)) {
        return item.id;
      }
    }
    return '';
  }, [timelineItems]);

  useEffect(() => {
    if (!open) {
      return undefined;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [onClose, open]);

  useEffect(() => {
    if (!open) {
      return;
    }
    setFollowStreaming(true);
    setPendingUpdateCount(0);
    previousStreamingActivityKeyRef.current = '';
  }, [open, sessionId, userMessageId, turnId]);

  useEffect(() => {
    if (!open) {
      return undefined;
    }

    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    closeButtonRef.current?.focus();

    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, [open]);

  useEffect(() => {
    const previousKey = previousStreamingActivityKeyRef.current;
    previousStreamingActivityKeyRef.current = streamingActivityKey;

    if (!open || !isStreaming || !streamingActivityKey) {
      return;
    }

    if (!previousKey || previousKey === streamingActivityKey) {
      return;
    }

    if (!followStreaming) {
      setPendingUpdateCount((prev) => prev + 1);
      return;
    }

    setPendingUpdateCount(0);
  }, [followStreaming, isStreaming, open, streamingActivityKey]);

  useEffect(() => {
    if (!open || !isStreaming || !followStreaming) {
      return;
    }
    const container = scrollContainerRef.current;
    if (!container) {
      return;
    }
    container.scrollTo({
      top: container.scrollHeight,
      behavior: 'smooth',
    });
  }, [followStreaming, isStreaming, open, streamingActivityKey]);

  const handleTimelineScroll = () => {
    if (!isStreaming) {
      return;
    }
    setFollowStreaming((prev) => {
      const next = isNearBottom(scrollContainerRef.current);
      return prev === next ? prev : next;
    });
  };

  const handleJumpToLatest = () => {
    const container = scrollContainerRef.current;
    setFollowStreaming(true);
    setPendingUpdateCount(0);
    if (!container) {
      return;
    }
    container.scrollTo({
      top: container.scrollHeight,
      behavior: 'smooth',
    });
  };

  if (!open) {
    return null;
  }

  const timelineLabels = {
    thinking: t('turnProcess.item.thinking'),
    toolLabel: t('turnProcess.item.tool', { name: '' }).replace(/\s*[·:：-]\s*$/, '').trim() || '工具',
    unavailableToolLabel: t('turnProcess.item.unavailableTool', { name: '' }).replace(/\s*[·:：-]\s*$/, '').trim() || '不可用工具',
    unnamedTool: t('turnProcess.item.toolFallback'),
  };

  return (
    <div className="fixed inset-0 z-[75] flex items-center justify-center p-4">
      <div
        aria-hidden="true"
        data-testid="turn-process-modal-overlay"
        className="fixed inset-0 bg-black/55"
        onClick={onClose}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={MODAL_TITLE_ID}
        aria-describedby={MODAL_DESC_ID}
        className="relative flex max-h-[88vh] w-full max-w-5xl flex-col overflow-hidden rounded-2xl border border-border bg-card shadow-2xl"
      >
        <div className="border-b border-border px-5 py-4">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <h2 id={MODAL_TITLE_ID} className="text-base font-semibold text-foreground">{t('turnProcess.title')}</h2>
                <span className={`inline-flex items-center rounded-full px-2 py-0.5 text-[11px] ${
                  isStreaming
                    ? 'bg-sky-100 text-sky-700 dark:bg-sky-950/40 dark:text-sky-300'
                    : 'bg-muted text-muted-foreground'
                }`}>
                  {isStreaming ? t('turnProcess.status.streaming') : t('turnProcess.status.completed')}
                </span>
              </div>
              <div className="mt-1 text-sm text-muted-foreground">
                {buildUserMessageTitle(userMessage?.content || '')}
              </div>
              <div id={MODAL_DESC_ID} className="mt-1 text-[11px] text-muted-foreground">
                {isStreaming ? t('turnProcess.streamingDescription') : t('turnProcess.staticDescription')}
              </div>
              {isStreaming && (
                <div className="mt-2 flex flex-wrap items-center gap-2 text-[11px]">
                  <span className={`inline-flex items-center rounded-full px-2.5 py-1 font-medium ${
                    followStreaming
                      ? 'bg-sky-100 text-sky-700 dark:bg-sky-950/40 dark:text-sky-300'
                      : 'bg-amber-100 text-amber-800 dark:bg-amber-950/40 dark:text-amber-200'
                  }`}>
                    {followStreaming ? t('turnProcess.following') : t('turnProcess.followPaused')}
                  </span>
                  <span className="text-muted-foreground">
                    {followStreaming
                      ? t('turnProcess.followingHelp')
                      : t('turnProcess.followPausedHelp')}
                  </span>
                </div>
              )}
              <div className="mt-2 flex flex-wrap gap-2 text-xs">
                <span className="rounded bg-muted px-2 py-0.5 text-muted-foreground">
                  {t('turnProcess.tools', { count: stats.toolCount })}
                </span>
                <span className="rounded bg-muted px-2 py-0.5 text-muted-foreground">
                  {t('turnProcess.thinking', { count: stats.thinkingCount })}
                </span>
                {stats.unavailableCount > 0 && (
                  <span className="rounded bg-amber-100 px-2 py-0.5 text-amber-800 dark:bg-amber-950/40 dark:text-amber-200">
                    {t('turnProcess.unavailable', { count: stats.unavailableCount })}
                  </span>
                )}
              </div>
              {timelineItems.length > 0 && (
                <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-5">
                  <div className="rounded-xl border border-border bg-background/70 px-3 py-2">
                    <div className="text-[11px] text-muted-foreground">{t('turnProcess.overview.total')}</div>
                    <div className="mt-1 text-sm font-semibold text-foreground">
                      {timelineOverview.total}
                    </div>
                  </div>
                  <div className="rounded-xl border border-sky-200 bg-sky-50/80 px-3 py-2 dark:border-sky-950/40 dark:bg-sky-950/20">
                    <div className="text-[11px] text-sky-700 dark:text-sky-300">{t('turnProcess.overview.running')}</div>
                    <div className="mt-1 text-sm font-semibold text-sky-800 dark:text-sky-200">
                      {timelineOverview.running}
                    </div>
                  </div>
                  <div className="rounded-xl border border-emerald-200 bg-emerald-50/80 px-3 py-2 dark:border-emerald-950/40 dark:bg-emerald-950/20">
                    <div className="text-[11px] text-emerald-700 dark:text-emerald-300">{t('turnProcess.overview.completed')}</div>
                    <div className="mt-1 text-sm font-semibold text-emerald-800 dark:text-emerald-200">
                      {timelineOverview.completed}
                    </div>
                  </div>
                  <div className="rounded-xl border border-rose-200 bg-rose-50/80 px-3 py-2 dark:border-rose-950/40 dark:bg-rose-950/20">
                    <div className="text-[11px] text-rose-700 dark:text-rose-300">{t('turnProcess.overview.failed')}</div>
                    <div className="mt-1 text-sm font-semibold text-rose-800 dark:text-rose-200">
                      {timelineOverview.failed}
                    </div>
                  </div>
                  <div className="rounded-xl border border-amber-200 bg-amber-50/80 px-3 py-2 dark:border-amber-950/40 dark:bg-amber-950/20">
                    <div className="text-[11px] text-amber-700 dark:text-amber-300">{t('turnProcess.overview.unavailable')}</div>
                    <div className="mt-1 text-sm font-semibold text-amber-800 dark:text-amber-200">
                      {timelineOverview.unavailable}
                    </div>
                  </div>
                </div>
              )}
            </div>
            <button
              ref={closeButtonRef}
              type="button"
              className="rounded-md border border-border bg-background px-3 py-1.5 text-xs text-foreground hover:bg-accent"
              onClick={onClose}
            >
              {t('common.close')}
            </button>
          </div>
        </div>

        <div
          ref={scrollContainerRef}
          data-testid="turn-process-modal-scroll"
          onScroll={handleTimelineScroll}
          className="flex-1 overflow-y-auto px-5 py-4"
        >
          {!userMessage && !loading && (
            <div className="text-sm text-muted-foreground">{t('turnProcess.notFound')}</div>
          )}

          {error && (
            <div className="mb-4 rounded-md border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-200">
              {error}
            </div>
          )}

          {loading && timelineItems.length === 0 && (
            <div className="text-sm text-muted-foreground">{t('turnProcess.loading')}</div>
          )}

          {!loading && timelineItems.length === 0 && !error && (
            <div className="text-sm text-muted-foreground">{t('turnProcess.empty')}</div>
          )}

          {timelineItems.length > 0 && (
            <div className="space-y-4" aria-live={isStreaming ? 'polite' : undefined}>
              {timelineItems.map((item, index) => {
                const itemStatus = getTimelineItemStatus(item);
                const showStreamLog = item.kind === 'tool_call'
                  ? shouldShowToolStreamLog(item)
                  : false;
                const isRunningStep = isRunningStatusLabel(itemStatus.label);
                const isPrimaryActiveStep = item.id === primaryActiveItemId;
                const isSecondaryRunningStep = isRunningStep && !isPrimaryActiveStep;
                const itemDotClass = item.kind === 'tool_unavailable'
                  ? 'bg-amber-500'
                  : item.kind === 'tool_call'
                    ? (itemStatus.label === '失败'
                      ? 'bg-rose-500'
                      : (itemStatus.label === '已完成' ? 'bg-emerald-500' : 'bg-sky-500'))
                    : 'bg-violet-500';

                return (
                  <div key={item.id} className="flex gap-3">
                    <div className="flex w-20 shrink-0 flex-col items-center">
                      <div className={`mt-1 h-2.5 w-2.5 rounded-full ${itemDotClass} ${
                        isPrimaryActiveStep
                          ? 'animate-pulse shadow-[0_0_0_6px_rgba(14,165,233,0.12)]'
                          : (isSecondaryRunningStep ? 'shadow-[0_0_0_4px_rgba(14,165,233,0.08)]' : '')
                      }`} />
                      {index < timelineItems.length - 1 && (
                        <div className="mt-1 min-h-8 flex-1 w-px bg-border" />
                      )}
                    </div>

                    <div className={`min-w-0 flex-1 rounded-xl border px-4 py-3 ${
                      isPrimaryActiveStep
                        ? 'border-sky-300 bg-sky-50/60 shadow-sm dark:border-sky-900/50 dark:bg-sky-950/10'
                        : (isSecondaryRunningStep
                          ? 'border-sky-200 bg-sky-50/30 dark:border-sky-950/30 dark:bg-sky-950/5'
                          : 'border-border bg-background/80')
                    }`}>
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="flex flex-wrap items-center gap-2">
                          <div className="text-xs font-medium text-foreground">
                            {getTimelineItemTitle(item, timelineLabels)}
                          </div>
                          <span className={`inline-flex items-center rounded-full px-2 py-0.5 text-[11px] ${itemStatus.className}`}>
                            {itemStatus.label}
                          </span>
                          {isPrimaryActiveStep && (
                          <span className="inline-flex items-center rounded-full bg-sky-600 px-2 py-0.5 text-[11px] font-medium text-white">
                              {t('turnProcess.activeLatest')}
                            </span>
                          )}
                        </div>
                        <div className="text-[11px] text-muted-foreground">
                          {formatTime(item.createdAt)}
                        </div>
                      </div>
                      {isPrimaryActiveStep && (
                        <div className="mt-2 text-[11px] font-medium text-sky-700 dark:text-sky-300">
                          {t('turnProcess.activeCurrent')}
                        </div>
                      )}
                      {isSecondaryRunningStep && (
                        <div className="mt-2 text-[11px] text-sky-700/80 dark:text-sky-300/80">
                          {t('turnProcess.activeSecondary')}
                        </div>
                      )}

                      {item.kind === 'thinking' && (
                        <div className="mt-2 rounded-lg border border-border bg-card px-3 py-2">
                          <LazyMarkdownRenderer
                            content={item.text}
                            isStreaming={item.isStreaming}
                            className="thinking not-prose"
                          />
                        </div>
                      )}

                      {item.kind === 'tool_call' && (
                        <div className="mt-2 space-y-2">
                          <ToolCallRenderer toolCall={item.toolCall} />
                          {showStreamLog && (
                            <div className="rounded-lg border border-border bg-card px-3 py-2">
                              <div className="mb-1 text-[11px] font-medium text-muted-foreground">
                                {t('turnProcess.liveOutput')}
                              </div>
                              <pre className="overflow-x-auto whitespace-pre-wrap break-words text-xs text-foreground/85">
                                {item.streamLog}
                              </pre>
                            </div>
                          )}
                        </div>
                      )}

                      {item.kind === 'tool_unavailable' && (
                        <div className="mt-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200">
                          {item.entry.reason}
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {isStreaming && !followStreaming && (
          <div className="pointer-events-none absolute bottom-4 right-5 flex flex-col items-end gap-2">
            <div className="pointer-events-auto rounded-full border border-border bg-card/95 px-3 py-1 text-[11px] text-muted-foreground shadow-sm backdrop-blur">
              {pendingUpdateCount > 0
                ? t('turnProcess.pendingUpdates', { count: pendingUpdateCount })
                : t('turnProcess.pendingUpdateSingle')}
            </div>
            <button
              type="button"
              onClick={handleJumpToLatest}
              className="pointer-events-auto rounded-full bg-primary px-4 py-2 text-xs font-medium text-primary-foreground shadow-lg hover:bg-primary/90"
            >
              {t('turnProcess.jumpToLatest')}
            </button>
          </div>
        )}
      </div>
    </div>
  );
};

export default TurnProcessModal;
