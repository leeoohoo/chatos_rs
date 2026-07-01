// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo, useRef, useState, type FC } from 'react';
import { RefreshCw, X } from 'lucide-react';
import type { Message } from '../../types';
import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';
import { MessageTaskDetailModal, MessageTaskProcessLogModal } from './MessageTaskDetailModal';
import { MessageTaskGraphPanel } from './MessageTaskGraphPanel';
import { MessageTaskRunDetailModal } from './MessageTaskRunDetailModal';
import { formatDateTime, readString } from './utils';
import { useMessageTaskGraph } from './useMessageTaskGraph';

interface MessageTaskDrawerProps {
  open: boolean;
  message: Message;
  onClose: () => void;
}

const MESSAGE_TASK_DRAWER_WIDTH_KEY = 'message_task_drawer_width';
const MESSAGE_TASK_DRAWER_DEFAULT_WIDTH = 760;

const getDrawerWidthBounds = () => {
  if (typeof window === 'undefined') {
    return {
      minWidth: 460,
      maxWidth: 1120,
    };
  }
  const maxWidth = Math.max(360, Math.min(1120, window.innerWidth - 40));
  const minWidth = Math.min(460, maxWidth);
  return { minWidth, maxWidth };
};

const clampDrawerWidth = (value: number): number => {
  const { minWidth, maxWidth } = getDrawerWidthBounds();
  return Math.min(Math.max(value, minWidth), maxWidth);
};

const readInitialDrawerWidth = (): number => {
  if (typeof window === 'undefined') {
    return clampDrawerWidth(MESSAGE_TASK_DRAWER_DEFAULT_WIDTH);
  }
  const saved = Number(window.localStorage.getItem(MESSAGE_TASK_DRAWER_WIDTH_KEY));
  if (Number.isFinite(saved) && saved > 0) {
    return clampDrawerWidth(saved);
  }
  return clampDrawerWidth(MESSAGE_TASK_DRAWER_DEFAULT_WIDTH);
};

export const MessageTaskDrawer: FC<MessageTaskDrawerProps> = ({
  open,
  message,
  onClose,
}) => {
  const { t } = useI18n();
  const resizeStartX = useRef(0);
  const resizeStartWidth = useRef(0);
  const [drawerWidth, setDrawerWidth] = useState(readInitialDrawerWidth);
  const [isResizing, setIsResizing] = useState(false);

  const taskLookup = useMemo(() => {
    const taskRunnerAsync = message.metadata?.task_runner_async;
    const rawSourceUserMessageId = readString(taskRunnerAsync?.source_user_message_id);
    const sourceUserMessageId = rawSourceUserMessageId?.startsWith('temp_')
      ? null
      : rawSourceUserMessageId;
    return {
      sessionId: message.sessionId,
      turnId: readString(message.metadata?.conversation_turn_id)
        || readString(taskRunnerAsync?.source_turn_id),
      sourceUserMessageId,
    };
  }, [message.metadata, message.sessionId]);

  const {
    graph,
    rootTasks,
    allTasks,
    sourceUserMessageId,
    loading,
    error,
    detailTask,
    processTask,
    runDetail,
    loadingProcessTaskId,
    loadingRunId,
    reloadGraph,
    openDetail,
    openProcessLog,
    openRun,
    loadMoreRunEvents,
    closeDetail,
    closeProcessLog,
    closeRun,
  } = useMessageTaskGraph({
    open,
    messageId: message.id,
    lookup: taskLookup,
  });

  const role = message.role === 'user'
    ? t('message.role.user')
    : message.role === 'assistant'
      ? t('message.role.assistant')
      : message.role;
  const messageSummary = `${role} · ${formatDateTime(message.createdAt.toISOString())}`;
  const dependencyTaskCount = Math.max(allTasks.length - rootTasks.length, 0);

  useEffect(() => {
    if (!isResizing) {
      return undefined;
    }
    const handleMove = (event: MouseEvent) => {
      const delta = resizeStartX.current - event.clientX;
      setDrawerWidth(clampDrawerWidth(resizeStartWidth.current + delta));
    };
    const handleUp = () => {
      setIsResizing(false);
    };
    window.addEventListener('mousemove', handleMove);
    window.addEventListener('mouseup', handleUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    return () => {
      window.removeEventListener('mousemove', handleMove);
      window.removeEventListener('mouseup', handleUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isResizing]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    window.localStorage.setItem(MESSAGE_TASK_DRAWER_WIDTH_KEY, String(drawerWidth));
  }, [drawerWidth]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return undefined;
    }
    const handleResize = () => {
      setDrawerWidth((current) => clampDrawerWidth(current));
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  if (!open) {
    return null;
  }

  return (
    <>
      <div
        className={cn(
          'w-1.5 shrink-0 cursor-col-resize bg-border/60 transition-colors hover:bg-primary/35',
          isResizing && 'bg-primary/45',
        )}
        onMouseDown={(event) => {
          resizeStartX.current = event.clientX;
          resizeStartWidth.current = drawerWidth;
          setIsResizing(true);
        }}
        aria-hidden
      />
      <aside
        className="h-full shrink-0 border-l border-border bg-card shadow-xl"
        style={{
          width: drawerWidth,
          minWidth: drawerWidth,
          maxWidth: drawerWidth,
        }}
      >
        <div className="flex h-full flex-col">
          <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
            <div className="min-w-0">
              <h2 className="text-sm font-semibold text-foreground">任务流程图</h2>
              <p className="mt-0.5 truncate text-xs text-muted-foreground">{messageSummary}</p>
              <p className="mt-0.5 truncate text-xs text-muted-foreground">
                源消息：{sourceUserMessageId || message.id}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
                disabled={loading}
                onClick={() => void reloadGraph()}
                aria-label="刷新任务"
              >
                <RefreshCw className={cn('h-4 w-4', loading && 'animate-spin')} />
              </button>
              <button
                type="button"
                className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
                onClick={onClose}
                aria-label="关闭"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          </div>

          <div className="flex min-h-0 flex-1 flex-col px-4 py-4">
            {error ? (
              <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                {error}
              </div>
            ) : null}

            <div className="mb-4 grid grid-cols-3 gap-3">
              <div className="rounded-lg border border-border bg-background px-3 py-2">
                <div className="text-[11px] text-muted-foreground">当前消息任务</div>
                <div className="mt-1 text-lg font-semibold text-foreground">{rootTasks.length}</div>
              </div>
              <div className="rounded-lg border border-border bg-background px-3 py-2">
                <div className="text-[11px] text-muted-foreground">已展开前置任务</div>
                <div className="mt-1 text-lg font-semibold text-foreground">{dependencyTaskCount}</div>
              </div>
              <div className="rounded-lg border border-border bg-background px-3 py-2">
                <div className="text-[11px] text-muted-foreground">依赖连线</div>
                <div className="mt-1 text-lg font-semibold text-foreground">{graph.edges.length}</div>
              </div>
            </div>

            <p className="mb-4 text-xs leading-5 text-muted-foreground">
              这里会把当前消息直接关联的任务和它们的前置依赖一起展开成 DAG。节点上的
              <span className="font-medium text-foreground">执行过程</span>
              可直接查看过程记录，按钮也可查看
              <span className="font-medium text-foreground">详情</span>
              或
              <span className="font-medium text-foreground">运行详情</span>
              。
            </p>

            <div className="min-h-0 flex-1">
              <MessageTaskGraphPanel
                graph={graph}
                loading={loading}
                error={error}
                loadingRunId={loadingRunId}
                loadingProcessTaskId={loadingProcessTaskId}
                panelWidth={drawerWidth}
                onOpenDetail={openDetail}
                onOpenProcessLog={openProcessLog}
                onOpenRun={openRun}
              />
            </div>
          </div>
        </div>
      </aside>

      <MessageTaskDetailModal task={detailTask} relatedTasks={allTasks} onClose={closeDetail} />
      <MessageTaskProcessLogModal task={processTask} onClose={closeProcessLog} />
      <MessageTaskRunDetailModal
        detail={runDetail}
        loadingMoreEvents={Boolean(runDetail && loadingRunId === runDetail.run?.id)}
        onLoadMoreEvents={loadMoreRunEvents}
        onClose={closeRun}
      />
    </>
  );
};
