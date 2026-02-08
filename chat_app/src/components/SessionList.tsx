import React, { useState, useEffect } from 'react';
import { useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import type { Session } from '../types';
import { PlusIcon, DotsVerticalIcon, PencilIcon, TrashIcon, ChatIcon } from './ui/icons';
import ConfirmDialog from './ui/ConfirmDialog';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { cn } from '../lib/utils';

// 简化的时间格式化函数
const formatTimeAgo = (date: string | Date | undefined | null) => {
  const now = new Date();
  let past: Date;
  
  // 处理不同的日期格式
  if (!date) {
    return '时间未知';
  }
  
  if (typeof date === 'string') {
    // 处理数据库返回的时间格式 "YYYY-MM-DD HH:mm:ss"
    // 将其转换为ISO格式以便正确解析
    const isoString = date.replace(' ', 'T') + 'Z';
    past = new Date(isoString);
    
    // 如果ISO格式解析失败，尝试直接解析原字符串
    if (isNaN(past.getTime())) {
      past = new Date(date);
    }
  } else {
    past = date;
  }
  
  // 检查日期是否有效
  if (!past || isNaN(past.getTime())) {
    return '时间未知';
  }
  
  const diffInSeconds = Math.floor((now.getTime() - past.getTime()) / 1000);
  
  if (diffInSeconds < 60) return '刚刚';
  if (diffInSeconds < 3600) return `${Math.floor(diffInSeconds / 60)}分钟前`;
  if (diffInSeconds < 86400) return `${Math.floor(diffInSeconds / 3600)}小时前`;
  if (diffInSeconds < 2592000) return `${Math.floor(diffInSeconds / 86400)}天前`;
  return past.toLocaleDateString('zh-CN');
};

interface SessionListProps {
  isOpen?: boolean;
  onClose?: () => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  className?: string;
  store?: typeof useChatStore;
}

export const SessionList: React.FC<SessionListProps> = ({
  isOpen = true,
  onClose,
  collapsed,
  onToggleCollapse,
  className,
  store,
}) => {
  // 尝试从Context获取store（如果可用）
  let contextStore = null;
  try {
    contextStore = useChatStoreFromContext();
  } catch (error) {
    // 如果Context不可用，contextStore保持为null
  }
  
  const storeToUse = store ? store() : contextStore;
  
  if (!storeToUse) {
    throw new Error('SessionList must be used within a ChatStoreProvider or receive a store prop');
  }
  
  const { sessions, currentSession, createSession, selectSession, deleteSession, updateSession, loadSessions, sessionChatState } = storeToUse;
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState('');
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [hasMoreLocked, setHasMoreLocked] = useState(false);
  const PAGE_SIZE = 30;
  
  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

  const isCollapsed = collapsed ?? !isOpen;
  const handleToggleCollapse = () => {
    if (onToggleCollapse) {
      onToggleCollapse();
      return;
    }
    onClose?.();
  };

  const handleCreateSession = async () => {
    try {
      await createSession();
    } catch (error) {
      console.error('Failed to create session:', error);
    }
  };

  const handleSelectSession = async (sessionId: string) => {
    try {
      await selectSession(sessionId);
    } catch (error) {
      console.error('Failed to select session:', error);
    }
  };

  const handleRefreshSessions = async () => {
    setIsRefreshing(true);
    const fetched = await loadSessions({ limit: PAGE_SIZE, offset: 0, append: false, silent: true });
    setIsRefreshing(false);
    setHasMoreLocked(false);
    setHasMore(fetched.length >= PAGE_SIZE);
  };

  const handleLoadMoreSessions = async () => {
    if (isLoadingMore) return;
    setIsLoadingMore(true);
    const fetched = await loadSessions({ limit: PAGE_SIZE, offset: sessions.length, append: true, silent: true });
    setIsLoadingMore(false);
    if (!fetched || fetched.length < PAGE_SIZE) {
      setHasMore(false);
      setHasMoreLocked(true);
    }
  };

  const handleDeleteSession = async (sessionId: string) => {
    const session = sessions.find((s: Session) => s.id === sessionId);
    showConfirmDialog({
      title: '删除确认',
      message: `确定要删除会话 "${session?.title || 'Untitled'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteSession(sessionId);
        } catch (error) {
          console.error('Failed to delete session:', error);
        }
      }
    });
  };

  const handleStartEdit = (sessionId: string, currentTitle: string) => {
    setEditingSessionId(sessionId);
    setEditingTitle(currentTitle);
  };

  const handleSaveEdit = async () => {
    if (editingSessionId && editingTitle.trim()) {
      try {
        await updateSession(editingSessionId, { title: editingTitle.trim() });
        setEditingSessionId(null);
        setEditingTitle('');
      } catch (error) {
        console.error('Failed to update session:', error);
      }
    }
  };

  const handleCancelEdit = () => {
    setEditingSessionId(null);
    setEditingTitle('');
  };

  const handleKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleSaveEdit();
    } else if (e.key === 'Escape') {
      handleCancelEdit();
    }
  };

  useEffect(() => {
    if (hasMoreLocked) return;
    if (sessions.length === 0) return;
    setHasMore(sessions.length >= PAGE_SIZE);
  }, [sessions.length, hasMoreLocked]);

  return (
    <div
      className={cn(
        'flex flex-col h-full bg-card border-r border-border transition-all duration-200',
        isCollapsed ? 'w-14' : 'w-80 sm:w-96',
        className
      )}
    >
      {/* 头部 */}
      <div className={cn('flex items-center justify-between p-3 border-b border-border', isCollapsed && 'flex-col gap-2')}>
        <div className={cn('flex items-center gap-2', isCollapsed && 'flex-col')}>
          <button
            onClick={handleToggleCollapse}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title={isCollapsed ? '展开会话列表' : '收起会话列表'}
          >
            <svg className={cn('w-4 h-4 transition-transform', isCollapsed ? 'rotate-180' : '')} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
              <polyline points="15 18 9 12 15 6" />
            </svg>
          </button>
          {!isCollapsed && (
            <h2 className="text-base font-semibold text-foreground">
              会话列表
            </h2>
          )}
        </div>
        <div className={cn('flex items-center gap-1', isCollapsed && 'flex-col')}>
          <button
            onClick={handleRefreshSessions}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="刷新会话列表"
          >
            <svg className={cn('w-5 h-5', isRefreshing && 'animate-spin')} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12a7.5 7.5 0 0112.125-5.303M19.5 12a7.5 7.5 0 01-12.125 5.303M16.5 6.697V3m0 3.697h-3.697M7.5 17.303V21m0-3.697H3.803" />
            </svg>
          </button>
          <button
            onClick={handleCreateSession}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="新建会话"
          >
            <PlusIcon className="w-5 h-5" />
          </button>
        </div>
      </div>

      {/* 会话列表 */}
      {!isCollapsed && (
        <div className="flex-1 overflow-y-auto">
          {sessions.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
              <ChatIcon className="w-12 h-12 mb-4 opacity-50" />
              <p className="text-sm">还没有会话</p>
              <button
                onClick={handleCreateSession}
                className="mt-2 px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
              >
                创建第一个会话
              </button>
            </div>
          ) : (
            <div className="p-2 space-y-1">
              {sessions.map((session: Session) => (
                <div
                  key={session.id}
                  className={`group relative flex items-center p-3 rounded-lg cursor-pointer transition-colors ${
                    currentSession?.id === session.id
                      ? 'bg-accent border border-border'
                      : 'hover:bg-accent/50'
                  }`}
                  onClick={() => handleSelectSession(session.id)}
                >
                  <div className="flex-1 min-w-0">
                    {editingSessionId === session.id ? (
                      <input
                        type="text"
                        value={editingTitle}
                        onChange={(e) => setEditingTitle(e.target.value)}
                        onBlur={handleSaveEdit}
                        onKeyDown={handleKeyPress}
                        className="w-full px-2 py-1 text-sm bg-background border border-border rounded focus:outline-none focus:ring-2 focus:ring-ring"
                        autoFocus
                        onClick={(e) => e.stopPropagation()}
                      />
                    ) : (
                      <>
                        <h3 className="text-sm font-medium text-foreground truncate">
                          {session.title}
                        </h3>
                        <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
                          <span>{formatTimeAgo(session.updatedAt)}</span>
                          <span className="text-muted-foreground/60">·</span>
                          {(() => {
                            const chatState = sessionChatState?.[session.id];
                            const isBusy = !!(chatState?.isLoading || chatState?.isStreaming);
                            return (
                              <span className={cn('inline-flex items-center gap-1', isBusy ? 'text-amber-600' : 'text-muted-foreground')}>
                                <span className={cn('inline-block w-2 h-2 rounded-full', isBusy ? 'bg-amber-500' : 'bg-muted-foreground/40')} />
                                {isBusy ? '执行中' : '空闲'}
                              </span>
                            );
                          })()}
                        </div>
                      </>
                    )}
                  </div>

                  {/* 操作菜单 */}
                  {editingSessionId !== session.id && (
                    <div className="relative">
                      <button
                        className="p-1 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                        onClick={(e: React.MouseEvent) => {
                          e.stopPropagation();
                          const menu = e.currentTarget.nextElementSibling as HTMLElement;
                          if (menu) {
                            menu.classList.toggle('hidden');
                          }
                        }}
                      >
                        <DotsVerticalIcon className="w-4 h-4" />
                      </button>
                      <div className="hidden absolute right-0 z-10 mt-1 w-32 bg-popover border border-border rounded-md shadow-lg">
                        <div className="py-1">
                          <button
                            onClick={(e: React.MouseEvent) => {
                              e.stopPropagation();
                              handleStartEdit(session.id, session.title);
                              const menu = e.currentTarget.closest('.absolute') as HTMLElement;
                              if (menu) menu.classList.add('hidden');
                            }}
                            className="flex items-center w-full px-3 py-2 text-sm text-popover-foreground hover:bg-accent"
                          >
                            <PencilIcon className="w-4 h-4 mr-2" />
                            重命名
                          </button>
                          <button
                            onClick={(e: React.MouseEvent) => {
                              e.stopPropagation();
                              handleDeleteSession(session.id);
                              const menu = e.currentTarget.closest('.absolute') as HTMLElement;
                              if (menu) menu.classList.add('hidden');
                            }}
                            className="flex items-center w-full px-3 py-2 text-sm text-destructive hover:bg-destructive/10"
                          >
                            <TrashIcon className="w-4 h-4 mr-2" />
                            删除
                          </button>
                        </div>
                      </div>
                    </div>
                )}
              </div>
              ))}
              {hasMore && (
                <div className="pt-2">
                  <button
                    onClick={handleLoadMoreSessions}
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

      {/* 确认对话框 */}
      <ConfirmDialog
        isOpen={dialogState.isOpen}
        title={dialogState.title}
        message={dialogState.message}
        confirmText={dialogState.confirmText}
        cancelText={dialogState.cancelText}
        type={dialogState.type}
        onConfirm={handleConfirm}
        onCancel={handleCancel}
      />
    </div>
  );
};
