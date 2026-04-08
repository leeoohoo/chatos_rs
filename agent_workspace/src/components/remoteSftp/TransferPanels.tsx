import React from 'react';
import type { SftpTransferRequest, SftpTransferStatus } from './types';

interface TransferStatusBannerProps {
  transferStatus: SftpTransferStatus | null;
  transfering: boolean;
  formatBytes: (value: number) => string;
  onCancelTransfer: () => void;
}

export const TransferStatusBanner: React.FC<TransferStatusBannerProps> = ({
  transferStatus,
  transfering,
  formatBytes,
  onCancelTransfer,
}) => {
  if (!transferStatus || !transfering) {
    return null;
  }

  return (
    <div className="px-4 py-2 border-b border-border bg-muted/30">
      <div className="flex items-center justify-between text-xs text-foreground">
        <span>
          {transferStatus.state === 'cancelling'
            ? '取消中...'
            : transferStatus.direction === 'upload'
              ? '上传中'
              : '下载中'}
        </span>
        <div className="flex items-center gap-2">
          <span>
            {transferStatus.percent !== null
              ? `${transferStatus.percent.toFixed(1)}%`
              : `${formatBytes(transferStatus.transferredBytes)}${transferStatus.totalBytes !== null ? ` / ${formatBytes(transferStatus.totalBytes)}` : ''}`}
          </span>
          <button
            type="button"
            onClick={onCancelTransfer}
            disabled={transferStatus.state === 'cancelling'}
            className="rounded border border-border px-2 py-0.5 text-[11px] text-destructive hover:bg-destructive/10 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            取消
          </button>
        </div>
      </div>
      <div className="mt-2 h-1.5 rounded bg-muted overflow-hidden">
        {transferStatus.percent !== null ? (
          <div
            className="h-full bg-blue-500 transition-all duration-200"
            style={{ width: `${Math.max(2, Math.min(100, transferStatus.percent))}%` }}
          />
        ) : (
          <div className="h-full w-1/3 bg-blue-500/80 animate-pulse" />
        )}
      </div>
      {transferStatus.currentPath && (
        <div className="mt-1 text-[11px] text-muted-foreground truncate">{transferStatus.currentPath}</div>
      )}
    </div>
  );
};

interface TransferQueuePanelProps {
  queuedTransfers: SftpTransferRequest[];
  onClearQueuedTransfers: () => void;
  onRemoveQueuedTransfer: (transferId: string) => void;
}

export const TransferQueuePanel: React.FC<TransferQueuePanelProps> = ({
  queuedTransfers,
  onClearQueuedTransfers,
  onRemoveQueuedTransfer,
}) => {
  if (queuedTransfers.length === 0) {
    return null;
  }

  return (
    <div className="px-4 py-2 border-b border-border bg-muted/20">
      <div className="flex items-center justify-between text-[11px] text-muted-foreground">
        <span>排队任务：{queuedTransfers.length}</span>
        <button
          type="button"
          onClick={onClearQueuedTransfers}
          className="rounded border border-border px-2 py-0.5 text-[11px] text-foreground hover:bg-accent"
        >
          清空队列
        </button>
      </div>
      <div className="mt-1 space-y-1">
        {queuedTransfers.slice(0, 3).map((item, index) => (
          <div
            key={item.id}
            className="flex items-center justify-between gap-2 rounded border border-border px-2 py-1 text-[11px]"
          >
            <span className="truncate text-foreground">{index + 1}. {item.label}</span>
            <button
              type="button"
              onClick={() => onRemoveQueuedTransfer(item.id)}
              className="rounded border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground hover:bg-accent"
            >
              移除
            </button>
          </div>
        ))}
        {queuedTransfers.length > 3 && (
          <div className="text-[11px] text-muted-foreground">还有 {queuedTransfers.length - 3} 项待传输…</div>
        )}
      </div>
    </div>
  );
};
