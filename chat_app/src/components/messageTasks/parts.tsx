import type { FC, ReactNode } from 'react';
import { X } from 'lucide-react';
import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
import { cn } from '../../lib/utils';
import { readString, statusTone, stringifyValue } from './utils';

export const valueOrDash = (value: unknown): string => readString(value) || '-';

export const StatusBadge: FC<{ status?: string | null }> = ({ status }) => (
  <span className={cn('rounded border px-1.5 py-0.5 text-[11px]', statusTone(status))}>
    {valueOrDash(status)}
  </span>
);

export const FieldGrid: FC<{ items: Array<[string, unknown]> }> = ({ items }) => (
  <dl className="grid grid-cols-1 gap-2 text-xs sm:grid-cols-2">
    {items.map(([label, value]) => (
      <div key={label} className="min-w-0 rounded-md border border-border bg-muted/30 px-2 py-1.5">
        <dt className="text-muted-foreground">{label}</dt>
        <dd className="mt-0.5 break-words text-foreground">{stringifyValue(value)}</dd>
      </div>
    ))}
  </dl>
);

export const ModalShell: FC<{
  title: string;
  subtitle?: string;
  onClose: () => void;
  widthClassName?: string;
  children: ReactNode;
}> = ({
  title,
  subtitle,
  onClose,
  widthClassName = 'max-w-3xl',
  children,
}) => (
  <div className="fixed inset-0 z-[60]">
    <button
      type="button"
      className="absolute inset-0 bg-black/45"
      aria-label="关闭"
      onClick={onClose}
    />
    <div
      className={cn(
        'absolute left-1/2 top-1/2 max-h-[88vh] w-[calc(100vw-24px)] -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-lg border border-border bg-card shadow-xl sm:w-[calc(100vw-40px)]',
        widthClassName,
      )}
    >
      <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
        <div className="min-w-0">
          <h2 className="break-words text-sm font-semibold text-foreground">{title}</h2>
          {subtitle ? (
            <p className="mt-0.5 break-words text-xs text-muted-foreground">{subtitle}</p>
          ) : null}
        </div>
        <button
          type="button"
          className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          onClick={onClose}
          aria-label="关闭"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
      <div className="max-h-[calc(88vh-58px)] overflow-y-auto px-4 py-4">
        <div className="space-y-3">{children}</div>
      </div>
    </div>
  </div>
);

export const MarkdownCard: FC<{
  content?: string | null;
  emptyText?: string;
}> = ({
  content,
  emptyText = '-',
}) => {
  const text = readString(content);

  return (
    <div className="rounded-md border border-border bg-muted/20 p-4">
      {text ? (
        <LazyMarkdownRenderer content={text} className="text-sm" />
      ) : (
        <p className="text-sm text-muted-foreground">{emptyText}</p>
      )}
    </div>
  );
};
