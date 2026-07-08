// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC, ReactNode } from 'react';
import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
import { cn } from '../../lib/utils';
import { CollapsibleText } from './CollapsibleSection';
import type { RunEventTimelineEntry } from './runEventTimelineUtils';
import { describeStructuredValueSummary } from './runEventTimelineUtils';
import { formatDateTime } from './utils';

interface RunEventTimelineProps {
  entries: RunEventTimelineEntry[];
}

export const RunEventTimeline: FC<RunEventTimelineProps> = ({ entries }) => {
  if (!entries.length) {
    return <p className="text-sm text-muted-foreground">暂无事件</p>;
  }

  return (
    <ol className="space-y-4">
      {entries.map((entry, index) => (
        <li key={entry.key} className="relative pl-7">
          {index < entries.length - 1 ? (
            <span className="absolute left-[8px] top-5 bottom-[-18px] w-px bg-border" aria-hidden="true" />
          ) : null}
          <span
            className={cn(
              'absolute left-[3px] top-2.5 h-3.5 w-3.5 rounded-full border-2 border-card',
              toneClasses[entry.tone],
            )}
            aria-hidden="true"
          />

          <div className="rounded-lg border border-border bg-muted/20 p-3">
            <div className="flex flex-wrap items-start justify-between gap-2">
              <div className="min-w-0 space-y-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm font-medium text-foreground">{entry.title}</span>
                  {entry.kind === 'group' ? (
                    <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                      {entry.events.length > 1 ? `${entry.events.length} 条已聚合` : '已折叠'}
                    </span>
                  ) : null}
                </div>
                <p className="text-xs text-muted-foreground">
                  {formatTimeRange(entry.startedAt, entry.finishedAt)}
                </p>
              </div>
              {entry.summary ? (
                <span className="rounded-full bg-background px-2 py-0.5 text-[11px] text-muted-foreground">
                  {entry.summary}
                </span>
              ) : null}
            </div>

            {entry.message ? renderMessage(entry.message) : null}

            {entry.aggregatedText ? (
              <InlineDisclosure
                className="mt-3"
                summary={`查看内容 · ${describeStructuredValueSummary(entry.aggregatedText)}`}
              >
                <div className="rounded-md border border-border bg-background/80 p-3">
                  <LazyMarkdownRenderer content={entry.aggregatedText} className="text-sm" />
                </div>
              </InlineDisclosure>
            ) : null}

            {entry.kind === 'single' && entry.payload !== undefined ? (
              <InlineDisclosure
                className="mt-3"
                summary={`查看 payload · ${entry.payloadSummary || describeStructuredValueSummary(entry.payload)}`}
              >
                <CollapsibleText value={entry.payload} code maxHeightClassName="max-h-56" />
              </InlineDisclosure>
            ) : null}

            {entry.kind === 'group' ? (
              <InlineDisclosure
                className="mt-3"
                summary={`查看原始 ${entry.events.length} 条事件`}
              >
                <div className="space-y-3">
                  {entry.events.map((event) => {
                    const message = readString(event.message);
                    const payloadSummary = event.payload === undefined
                      ? null
                      : describeStructuredValueSummary(event.payload);

                    return (
                      <div
                        key={event.id}
                        className="rounded-md border border-border bg-background/80 px-3 py-2"
                      >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <span className="text-xs font-medium text-foreground">{event.event_type}</span>
                          <span className="text-[11px] text-muted-foreground">
                            {formatDateTime(event.created_at)}
                          </span>
                        </div>
                        {message ? renderMessage(message, 'mt-2') : null}
                        {event.payload !== undefined ? (
                          <InlineDisclosure
                            className="mt-2"
                            summary={`查看 payload · ${payloadSummary}`}
                          >
                            <CollapsibleText value={event.payload} code maxHeightClassName="max-h-48" />
                          </InlineDisclosure>
                        ) : null}
                      </div>
                    );
                  })}
                </div>
              </InlineDisclosure>
            ) : null}
          </div>
        </li>
      ))}
    </ol>
  );
};

const toneClasses: Record<RunEventTimelineEntry['tone'], string> = {
  danger: 'bg-red-500',
  info: 'bg-blue-500',
  muted: 'bg-slate-400',
  success: 'bg-emerald-500',
  warning: 'bg-amber-500',
};

const renderMessage = (message: string, className = 'mt-3') => {
  if (shouldCollapseText(message)) {
    return (
      <InlineDisclosure className={className} summary={`查看说明 · ${describeStructuredValueSummary(message)}`}>
        <p className="whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">
          {message}
        </p>
      </InlineDisclosure>
    );
  }

  return (
    <p className={cn(className, 'whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground')}>
      {message}
    </p>
  );
};

const shouldCollapseText = (text: string): boolean => (
  text.length > 220 || text.split(/\r?\n/).length > 5
);

const formatTimeRange = (
  startedAt?: string | null,
  finishedAt?: string | null,
): string => {
  const started = formatDateTime(startedAt);
  const finished = formatDateTime(finishedAt);
  return started === finished ? started : `${started} -> ${finished}`;
};

const readString = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const text = value.trim();
  return text ? text : null;
};

const InlineDisclosure: FC<{
  summary: string;
  children: ReactNode;
  className?: string;
}> = ({
  summary,
  children,
  className,
}) => (
  <details className={cn('rounded-md border border-border bg-background/80', className)}>
    <summary className="cursor-pointer px-3 py-2 text-xs font-medium text-primary hover:text-primary/80">
      {summary}
    </summary>
    <div className="border-t border-border px-3 py-3">
      {children}
    </div>
  </details>
);
