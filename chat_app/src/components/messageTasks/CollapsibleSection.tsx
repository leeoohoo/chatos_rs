// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC, ReactNode } from 'react';
import { useEffect, useState } from 'react';
import { ChevronDownIcon } from '../ui/icons';
import { cn } from '../../lib/utils';

interface CollapsibleSectionProps {
  title: string;
  defaultOpen?: boolean;
  summary?: string;
  children: ReactNode;
}

export const CollapsibleSection: FC<CollapsibleSectionProps> = ({
  title,
  defaultOpen = false,
  summary,
  children,
}) => {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <section className="rounded-md border border-border bg-background">
      <button
        type="button"
        className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        <span className="min-w-0">
          <span className="block text-sm font-medium text-foreground">{title}</span>
          {summary ? (
            <span className="block truncate text-xs text-muted-foreground">{summary}</span>
          ) : null}
        </span>
        <ChevronDownIcon
          className={cn(
            'h-4 w-4 shrink-0 text-muted-foreground transition-transform',
            open && 'rotate-180',
          )}
        />
      </button>
      {open ? (
        <div className="border-t border-border px-3 py-3">
          {children}
        </div>
      ) : null}
    </section>
  );
};

interface CollapsibleTextProps {
  value?: unknown;
  code?: boolean;
  maxHeightClassName?: string;
}

export const CollapsibleText: FC<CollapsibleTextProps> = ({
  value,
  code = false,
  maxHeightClassName = 'max-h-72',
}) => {
  const [expanded, setExpanded] = useState(false);
  const text = value === null || value === undefined || value === ''
    ? '-'
    : typeof value === 'string'
      ? value
      : JSON.stringify(value, null, 2);
  const lineCount = text.split(/\r?\n/).length;
  const canToggle = text.length > 480 || lineCount > 12;

  useEffect(() => {
    setExpanded(false);
  }, [text]);

  return (
    <div>
      <pre
        className={cn(
          'whitespace-pre-wrap break-words rounded-md border border-border bg-muted/40 p-3 text-xs leading-5 text-foreground',
          code && 'font-mono',
          canToggle && !expanded && `${maxHeightClassName} overflow-hidden`,
        )}
      >
        {text || '-'}
      </pre>
      {canToggle ? (
        <button
          type="button"
          className="mt-2 text-xs font-medium text-primary hover:text-primary/80"
          onClick={() => setExpanded((value) => !value)}
        >
          {expanded ? '收起' : '展开'}
        </button>
      ) : null}
    </div>
  );
};
