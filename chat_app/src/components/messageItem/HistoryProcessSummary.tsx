import type { FC } from 'react';

interface HistoryProcessSummaryProps {
  userMessageId: string;
  historyToolCount: number;
  historyThinkingCount: number;
  historyUnavailableToolCount: number;
  onToggleTurnProcess?: (userMessageId: string) => void;
}

export const HistoryProcessSummary: FC<HistoryProcessSummaryProps> = ({
  userMessageId,
  historyToolCount,
  historyThinkingCount,
  historyUnavailableToolCount,
  onToggleTurnProcess,
}) => (
  <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
    <button
      type="button"
      onClick={() => onToggleTurnProcess?.(userMessageId)}
      disabled={!onToggleTurnProcess}
      className="px-2 py-0.5 rounded border border-border bg-muted text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
    >
      查看过程
    </button>
    <span className="px-2 py-0.5 rounded bg-muted text-muted-foreground">
      Tools: {historyToolCount}
    </span>
    <span className="px-2 py-0.5 rounded bg-muted text-muted-foreground">
      Thinking: {historyThinkingCount}
    </span>
    {historyUnavailableToolCount > 0 && (
      <span className="px-2 py-0.5 rounded bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-200">
        Unavailable: {historyUnavailableToolCount}
      </span>
    )}
  </div>
);
