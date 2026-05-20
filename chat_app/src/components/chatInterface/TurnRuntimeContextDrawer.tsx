import React from 'react';
import type {
  TurnRuntimeSnapshotContextItem,
  TurnRuntimeSnapshotLookupResponse,
} from '../../lib/api/client/types';

interface TurnRuntimeContextDrawerProps {
  open: boolean;
  sessionId: string | null;
  loading: boolean;
  error: string | null;
  data: TurnRuntimeSnapshotLookupResponse | null;
  onRefresh: () => void;
  onClose: () => void;
}

const renderValue = (value?: string | null): string => {
  const normalized = value?.trim();
  return normalized ? normalized : '-';
};

const getPreviewItemTone = (role?: string | null, type?: string | null): string => {
  if (type === 'tool') {
    return 'border-amber-500/40 bg-amber-500/10';
  }
  if (role === 'system') {
    return 'border-sky-500/40 bg-sky-500/10';
  }
  if (role === 'assistant') {
    return 'border-border bg-background/80';
  }
  return 'border-border bg-background/70';
};

const buildItemSummary = (item: TurnRuntimeSnapshotContextItem): string => {
  const parts = [
    item.role ? `role=${item.role}` : '',
    item.type ? `type=${item.type}` : '',
    item.source ? `source=${item.source}` : '',
  ].filter(Boolean);
  return parts.join(' · ') || '上下文项';
};

const TurnRuntimeContextDrawer: React.FC<TurnRuntimeContextDrawerProps> = ({
  open,
  sessionId,
  loading,
  error,
  data,
  onRefresh,
  onClose,
}) => {
  if (!open) {
    return null;
  }

  const snapshot = data?.snapshot || null;
  const runtime = snapshot?.runtime || null;
  const actualPreviewItems = Array.isArray(runtime?.actual_context_items)
    ? runtime.actual_context_items
    : [];
  const status = data?.status || 'unknown';
  const snapshotSource = data?.snapshot_source || 'missing';

  return (
    <div className="fixed inset-0 z-50">
      <button
        type="button"
        aria-label="关闭上下文快照抽屉"
        className="absolute inset-0 bg-black/35"
        onClick={onClose}
      />
      <div className="absolute right-0 top-0 h-full w-full max-w-2xl border-l border-border bg-card shadow-xl">
        <div className="flex h-full flex-col">
          <div className="flex items-center justify-between border-b border-border px-4 py-3">
            <div>
              <div className="text-sm font-semibold text-foreground">轮次运行上下文</div>
              <div className="text-xs text-muted-foreground">
                {sessionId ? `会话 ${sessionId}` : '未选择会话'}
              </div>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60"
                disabled={!sessionId || loading}
                onClick={onRefresh}
              >
                {loading ? '刷新中...' : '刷新'}
              </button>
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent"
                onClick={onClose}
              >
                关闭
              </button>
            </div>
          </div>

          <div className="custom-scrollbar flex-1 overflow-y-scroll px-4 py-3 [scrollbar-gutter:stable]">
            {error ? (
              <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs text-destructive">
                {error}
              </div>
            ) : null}

            <div className="mb-3 rounded-md border border-border bg-background/70 p-3 text-xs text-muted-foreground">
              <div>{`turn_id: ${data?.turn_id || '-'}`}</div>
              <div>{`status: ${status}`}</div>
              <div>{`snapshot_source: ${snapshotSource}`}</div>
              <div>{`captured_at: ${snapshot?.captured_at || '-'}`}</div>
              <div>{`mode: ${renderValue(runtime?.actual_context_mode)}`}</div>
              <div>{`previous_response_id: ${renderValue(runtime?.actual_previous_response_id)}`}</div>
              <div>{`item_count: ${actualPreviewItems.length}`}</div>
            </div>

            <div className="mb-3 rounded-md border border-sky-500/30 bg-sky-500/10 p-3 text-xs text-sky-950 dark:text-sky-100">
              这里只保留当前轮次最新一次发给 AI 的上下文内容。
            </div>

            {actualPreviewItems.length === 0 ? (
              <div className="rounded-md border border-border bg-background/70 px-3 py-2 text-xs text-muted-foreground">
                当前快照里还没有记录实际发送上下文
              </div>
            ) : (
              <div className="space-y-2">
                {actualPreviewItems.map((item, index) => (
                  <details
                    key={`actual-preview:${index}:${item.role || '-'}:${item.type || '-'}:${item.source || '-'}`}
                    className={`rounded-md border p-3 ${getPreviewItemTone(item.role, item.type)}`}
                  >
                    <summary className="cursor-pointer list-none text-xs text-foreground">
                      <span className="font-medium">{`${index + 1}. ${buildItemSummary(item)}`}</span>
                      <span className="ml-2 text-muted-foreground">默认折叠，点击展开内容</span>
                    </summary>
                    <pre className="mt-2 whitespace-pre-wrap break-words text-xs text-foreground">
{item.content}
                    </pre>
                  </details>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default TurnRuntimeContextDrawer;
