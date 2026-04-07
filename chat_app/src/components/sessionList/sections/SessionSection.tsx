import React from 'react';
import { cn } from '../../../lib/utils';
import type { Session } from '../../../types';
import { ChatIcon, DotsVerticalIcon, PlusIcon, TrashIcon } from '../../ui/icons';
import SessionBusyBadge from '../../chat/SessionBusyBadge';

type SessionStatus = 'active' | 'archiving' | 'archived';

type SessionChatStateMap = Record<
  string,
  {
    isLoading?: boolean;
    isStreaming?: boolean;
  }
>;

type ImRuntimeStateMap = Record<
  string,
  {
    busy?: boolean;
    unreadCount?: number;
    latestRunStatus?: string | null;
    lastMessagePreview?: string | null;
  }
>;

interface SessionSectionProps {
  expanded: boolean;
  sessions: Session[];
  currentSessionId?: string | null;
  summarySessionId?: string | null;
  displaySessionRuntimeIdMap?: Record<string, string>;
  imRuntimeStateBySessionId?: ImRuntimeStateMap;
  sessionChatState?: SessionChatStateMap;
  taskReviewPanelsBySession?: Record<string, any[]>;
  uiPromptPanelsBySession?: Record<string, any[]>;
  hasMore: boolean;
  isRefreshing: boolean;
  isLoadingMore: boolean;
  onToggle: () => void;
  onRefresh: () => void;
  onCreateSession: () => void;
  onSelectSession: (sessionId: string) => void;
  onOpenSummary: (sessionId: string) => void;
  onManageBuiltinMcpGrants: (sessionId: string) => void;
  onDeleteSession: (sessionId: string) => void;
  onLoadMore: () => void;
  onToggleActionMenu: (event: React.MouseEvent<HTMLButtonElement>) => void;
  closeActionMenus: () => void;
  formatTimeAgo: (date: string | Date | undefined | null) => string;
  getSessionStatus: (session: Session) => SessionStatus;
}

export const SessionSection: React.FC<SessionSectionProps> = ({
  expanded,
  sessions,
  currentSessionId,
  summarySessionId,
  displaySessionRuntimeIdMap = {},
  imRuntimeStateBySessionId = {},
  sessionChatState,
  taskReviewPanelsBySession = {},
  uiPromptPanelsBySession = {},
  hasMore,
  isRefreshing,
  isLoadingMore,
  onToggle,
  onRefresh,
  onCreateSession,
  onSelectSession,
  onOpenSummary,
  onManageBuiltinMcpGrants,
  onDeleteSession,
  onLoadMore,
  onToggleActionMenu,
  closeActionMenus,
  formatTimeAgo,
  getSessionStatus,
}) => {
  return (
    <div className={cn('flex flex-col min-h-0', expanded ? 'flex-1' : 'shrink-0')}>
      <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
        <button
          type="button"
          onClick={onToggle}
          className="flex items-center gap-2 uppercase tracking-wide"
        >
          <span>{expanded ? '▾' : '▸'}</span>
          <span>CONTACTS</span>
        </button>
        <div className="flex items-center gap-1">
          <button
            onClick={onRefresh}
            className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
            title="刷新联系人列表"
          >
            <svg className={cn('w-4 h-4', isRefreshing && 'animate-spin')} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12a7.5 7.5 0 0112.125-5.303M19.5 12a7.5 7.5 0 01-12.125 5.303M16.5 6.697V3m0 3.697h-3.697M7.5 17.303V21m0-3.697H3.803" />
            </svg>
          </button>
          <button
            onClick={onCreateSession}
            className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
            title="添加联系人"
          >
            <PlusIcon className="w-4 h-4" />
          </button>
        </div>
      </div>

      {expanded && (
        <div className="flex-1 min-h-0 overflow-y-auto">
          {sessions.length === 0 ? (
            <div className="flex flex-col items-center justify-center text-muted-foreground py-6">
              <ChatIcon className="w-12 h-12 mb-4 opacity-50" />
              <p className="text-sm">还没有联系人</p>
              <button
                onClick={onCreateSession}
                className="mt-2 px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
              >
                添加第一个联系人
              </button>
            </div>
          ) : (
            <div className="p-2 space-y-1">
              {sessions.map((session) => {
                const sessionStatus = getSessionStatus(session);
                const isArchivedSession = sessionStatus !== 'active';
                const isArchivingSession = sessionStatus === 'archiving';
                const runtimeSessionId = displaySessionRuntimeIdMap[session.id] || session.id;
                const imRuntimeState = imRuntimeStateBySessionId?.[runtimeSessionId];
                const unreadCount = Number(imRuntimeState?.unreadCount || 0);
                const latestRunStatus = String(imRuntimeState?.latestRunStatus || '').trim().toLowerCase();
                const busyText = latestRunStatus === 'queued' || latestRunStatus === 'pending'
                  ? '处理中'
                  : '执行中';
                const previewText = typeof imRuntimeState?.lastMessagePreview === 'string'
                  ? imRuntimeState.lastMessagePreview.trim()
                  : '';

                return (
                  <div
                    key={session.id}
                    className={cn(
                      'group relative flex items-center p-3 rounded-lg transition-colors',
                      isArchivedSession ? 'cursor-default opacity-70' : 'cursor-pointer',
                      currentSessionId === session.id
                        ? 'bg-accent border border-border'
                        : (!isArchivedSession && 'hover:bg-accent/50'),
                    )}
                    onClick={() => {
                      if (isArchivedSession) {
                        return;
                      }
                      onSelectSession(session.id);
                    }}
                  >
                    <div className="flex-1 min-w-0">
                      <h3 className="text-sm font-medium text-foreground truncate">
                        {session.title}
                      </h3>
                      {previewText ? (
                        <p className="mt-1 text-xs text-muted-foreground truncate">
                          {previewText}
                        </p>
                      ) : null}
                      <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
                        <span>{formatTimeAgo(session.updatedAt)}</span>
                        <span className="text-muted-foreground/60">·</span>
                        {isArchivedSession ? (
                          <span className={cn('inline-flex items-center gap-1', isArchivingSession ? 'text-amber-600' : 'text-slate-500')}>
                            <span className={cn('inline-block w-2 h-2 rounded-full', isArchivingSession ? 'bg-amber-500 animate-pulse' : 'bg-slate-400')} />
                            {isArchivingSession ? '归档中' : '已归档'}
                          </span>
                        ) : (
                          (() => {
                            const chatState = sessionChatState?.[runtimeSessionId];
                            const isBusy = typeof imRuntimeState?.busy === 'boolean'
                              ? imRuntimeState.busy
                              : !!(chatState?.isLoading || chatState?.isStreaming);
                            return <SessionBusyBadge busy={isBusy} busyText={busyText} />;
                          })()
                        )}
                        {!isArchivedSession && unreadCount > 0 ? (
                          <span className="inline-flex items-center gap-1 text-emerald-600">
                            <span className="inline-block min-w-[16px] px-1 h-4 rounded-full bg-emerald-100 text-[10px] leading-4 text-center">
                              {unreadCount > 99 ? '99+' : unreadCount}
                            </span>
                            未读
                          </span>
                        ) : null}
                        {(() => {
                          if (isArchivedSession) {
                            return null;
                          }
                          const taskReviewCount = Array.isArray(taskReviewPanelsBySession?.[runtimeSessionId])
                            ? taskReviewPanelsBySession[runtimeSessionId].length
                            : 0;
                          const uiPromptCount = Array.isArray(uiPromptPanelsBySession?.[runtimeSessionId])
                            ? uiPromptPanelsBySession[runtimeSessionId].length
                            : 0;
                          const pendingCount = taskReviewCount + uiPromptCount;
                          if (pendingCount <= 0) {
                            return null;
                          }
                          return (
                            <span className="inline-flex items-center gap-1 text-blue-600">
                              <span className="inline-block w-2 h-2 rounded-full bg-blue-500 animate-pulse" />
                              {`待处理 ${pendingCount}`}
                            </span>
                          );
                        })()}
                      </div>
                    </div>

                    <div className="flex items-center gap-1 shrink-0">
                      <button
                        type="button"
                        className={cn(
                          'px-1.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground hover:text-foreground hover:bg-accent',
                          summarySessionId === session.id && 'text-blue-600 border-blue-200',
                        )}
                        onClick={(e) => {
                          e.stopPropagation();
                          if (isArchivedSession) {
                            return;
                          }
                          onOpenSummary(session.id);
                        }}
                        disabled={isArchivedSession}
                        title={summarySessionId === session.id ? '关闭总结视图' : '打开总结视图'}
                      >
                        {summarySessionId === session.id ? '关闭总结' : '总结'}
                      </button>
                      <div className="relative" data-action-menu-root="true">
                        <button
                          className="p-1 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                          onClick={onToggleActionMenu}
                        >
                          <DotsVerticalIcon className="w-4 h-4" />
                        </button>
                        <div className="js-inline-action-menu hidden absolute right-0 z-10 mt-1 w-32 bg-popover border border-border rounded-md shadow-lg">
                          <div className="py-1">
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                if (isArchivedSession) {
                                  return;
                                }
                                onManageBuiltinMcpGrants(session.id);
                                closeActionMenus();
                              }}
                              disabled={isArchivedSession}
                              className={cn(
                                'flex items-center w-full px-3 py-2 text-sm text-foreground hover:bg-accent',
                                isArchivedSession && 'opacity-50 cursor-not-allowed hover:bg-transparent',
                              )}
                            >
                              内置能力
                            </button>
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                onDeleteSession(session.id);
                                closeActionMenus();
                              }}
                              disabled={isArchivedSession}
                              className={cn(
                                'flex items-center w-full px-3 py-2 text-sm text-destructive hover:bg-destructive/10',
                                isArchivedSession && 'opacity-50 cursor-not-allowed hover:bg-transparent',
                              )}
                            >
                              <TrashIcon className="w-4 h-4 mr-2" />
                              {isArchivedSession ? '已归档' : '删除联系人'}
                            </button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                );
              })}
              {hasMore && (
                <div className="pt-2">
                  <button
                    onClick={onLoadMore}
                    disabled={isLoadingMore}
                    className="w-full px-3 py-2 text-sm text-muted-foreground hover:text-foreground border border-border rounded-lg hover:bg-accent transition-colors disabled:opacity-50"
                  >
                    {isLoadingMore ? '加载中...' : '加载更多'}
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
