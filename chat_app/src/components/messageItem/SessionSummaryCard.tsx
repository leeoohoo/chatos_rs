import type { FC } from 'react';
import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
import type { Message } from '../../types';

interface SessionSummaryCardProps {
  message: Message;
  keepLastN: number | null;
}

export const SessionSummaryCard: FC<SessionSummaryCardProps> = ({
  message,
  keepLastN,
}) => (
  <div className="mb-3 border border-amber-300 dark:border-amber-600/50 bg-amber-50 dark:bg-amber-950/20 rounded-md p-3">
    <div className="text-xs text-amber-900 dark:text-amber-200 font-medium mb-1">
      上下文已压缩为摘要{typeof keepLastN === 'number' ? `（保留最近 ${keepLastN} 条）` : ''}
    </div>
    <details className="group">
      <summary className="cursor-pointer text-xs text-muted-foreground select-none">
        查看摘要内容
      </summary>
      <div className="mt-2 prose prose-sm max-w-none">
        <LazyMarkdownRenderer
          content={(message.rawContent || message.metadata?.summary || '').toString()}
          isStreaming={false}
          onApplyCode={() => {}}
        />
      </div>
    </details>
  </div>
);
