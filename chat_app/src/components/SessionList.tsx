import React, { useState, useEffect, useRef } from 'react';
import { useChatStoreFromContext, useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import type { Session, Project, FsEntry } from '../types';
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

export const SessionList: React.FC<SessionListProps> = (props) => {
  const {
    isOpen = true,
    collapsed,
    className,
    store,
  } = props;
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
  
  const {
    sessions,
    currentSession,
    createSession,
    selectSession,
    deleteSession,
    updateSession,
    loadSessions,
    sessionChatState,
    projects,
    currentProject,
    loadProjects,
    createProject,
    selectProject,
    deleteProject,
  } = storeToUse;
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState('');
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [hasMoreLocked, setHasMoreLocked] = useState(false);
  const [sessionsExpanded, setSessionsExpanded] = useState(true);
  const [projectsExpanded, setProjectsExpanded] = useState(true);
  const PAGE_SIZE = 30;

  const [projectModalOpen, setProjectModalOpen] = useState(false);
  const [projectRoot, setProjectRoot] = useState('');
  const [projectError, setProjectError] = useState<string | null>(null);

  const [dirPickerOpen, setDirPickerOpen] = useState(false);
  const [dirPickerPath, setDirPickerPath] = useState<string | null>(null);
  const [dirPickerParent, setDirPickerParent] = useState<string | null>(null);
  const [dirPickerEntries, setDirPickerEntries] = useState<FsEntry[]>([]);
  const [dirPickerRoots, setDirPickerRoots] = useState<FsEntry[]>([]);
  const [dirPickerLoading, setDirPickerLoading] = useState(false);
  const [dirPickerError, setDirPickerError] = useState<string | null>(null);

  const apiClient = useChatApiClientFromContext();
  const apiBaseUrl = apiClient?.getBaseUrl ? apiClient.getBaseUrl() : '/api';
  const didLoadProjectsRef = useRef(false);
  
  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

  const isCollapsed = collapsed ?? !isOpen;
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

  const openProjectModal = () => {
    setProjectRoot('');
    setProjectError(null);
    setProjectModalOpen(true);
  };

  const deriveProjectName = (path: string) => {
    const trimmed = path.trim().replace(/[\\/]+$/, '');
    if (!trimmed) return 'Project';
    const parts = trimmed.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] || 'Project';
  };

  const handleCreateProject = async () => {
    if (!projectRoot.trim()) {
      setProjectError('请选择项目目录');
      return;
    }
    try {
      const name = deriveProjectName(projectRoot);
      await createProject(name, projectRoot.trim());
      setProjectModalOpen(false);
    } catch (error) {
      setProjectError(error instanceof Error ? error.message : '创建项目失败');
    }
  };

  const handleSelectProject = async (projectId: string) => {
    try {
      await selectProject(projectId);
      await loadSessions({ limit: PAGE_SIZE, offset: 0, append: false, silent: true });
    } catch (error) {
      console.error('Failed to select project:', error);
    }
  };

  const handleDeleteProject = async (projectId: string) => {
    const project = projects.find((p: Project) => p.id === projectId);
    showConfirmDialog({
      title: '删除确认',
      message: `确定要删除项目 "${project?.name || 'Untitled'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteProject(projectId);
        } catch (error) {
          console.error('Failed to delete project:', error);
        }
      }
    });
  };

  const loadDirEntries = async (path?: string | null) => {
    setDirPickerLoading(true);
    setDirPickerError(null);
    try {
      const url = `${apiBaseUrl}/fs/list${path ? `?path=${encodeURIComponent(path)}` : ''}`;
      const resp = await fetch(url);
      if (!resp.ok) {
        throw new Error(`HTTP ${resp.status}`);
      }
      const data = await resp.json();
      const mapEntry = (entry: any): FsEntry => ({
        name: entry?.name ?? '',
        path: entry?.path ?? '',
        isDir: entry?.is_dir ?? entry?.isDir ?? true,
        size: entry?.size ?? null,
        modifiedAt: entry?.modified_at ?? entry?.modifiedAt ?? null,
      });
      setDirPickerPath(data?.path ?? null);
      setDirPickerParent(data?.parent ?? null);
      setDirPickerEntries(Array.isArray(data?.entries) ? data.entries.map(mapEntry) : []);
      setDirPickerRoots(Array.isArray(data?.roots) ? data.roots.map(mapEntry) : []);
    } catch (err: any) {
      setDirPickerError(err?.message || '加载目录失败');
    } finally {
      setDirPickerLoading(false);
    }
  };

  const openDirPicker = async () => {
    setDirPickerOpen(true);
    const current = projectRoot.trim();
    await loadDirEntries(current ? current : null);
  };

  const chooseDir = (path: string | null) => {
    if (!path) return;
    setProjectRoot(path);
    setDirPickerOpen(false);
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

  useEffect(() => {
    if (didLoadProjectsRef.current) return;
    didLoadProjectsRef.current = true;
    loadProjects();
  }, [loadProjects]);

  return (
    <div
      className={cn(
        'flex flex-col h-full bg-card transition-all duration-200 overflow-hidden',
        isCollapsed ? 'w-0' : 'w-64 sm:w-72 border-r border-border',
        className
      )}
    >
      {/* 会话与项目列表 */}
      {!isCollapsed && (
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className={cn('flex flex-col min-h-0', sessionsExpanded ? 'flex-1' : 'shrink-0')}>
            <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
            <button
              type="button"
              onClick={() => setSessionsExpanded((prev) => !prev)}
              className="flex items-center gap-2 uppercase tracking-wide"
            >
              <span>{sessionsExpanded ? '▾' : '▸'}</span>
              <span>SESSIONS</span>
            </button>
            <div className="flex items-center gap-1">
              <button
                onClick={handleRefreshSessions}
                className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
                title="刷新会话列表"
              >
                <svg className={cn('w-4 h-4', isRefreshing && 'animate-spin')} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12a7.5 7.5 0 0112.125-5.303M19.5 12a7.5 7.5 0 01-12.125 5.303M16.5 6.697V3m0 3.697h-3.697M7.5 17.303V21m0-3.697H3.803" />
                </svg>
              </button>
              <button
                onClick={handleCreateSession}
                className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
                title="新建会话"
              >
                <PlusIcon className="w-4 h-4" />
              </button>
            </div>
            </div>
            {sessionsExpanded && (
              <div className="flex-1 min-h-0 overflow-y-auto">
                {sessions.length === 0 ? (
                  <div className="flex flex-col items-center justify-center text-muted-foreground py-6">
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
          </div>

          <div className="my-2 border-t border-border" />

          <div className="flex flex-col">
            <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
            <button
              type="button"
              onClick={() => setProjectsExpanded((prev) => !prev)}
              className="flex items-center gap-2 uppercase tracking-wide"
            >
              <span>{projectsExpanded ? '▾' : '▸'}</span>
              <span>PROJECTS</span>
            </button>
            <button
              type="button"
              onClick={openProjectModal}
              className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
              title="新增项目"
            >
              <PlusIcon className="w-4 h-4" />
            </button>
            </div>

            {projectsExpanded && (
              <div className="max-h-64 overflow-y-auto">
                {projects.length === 0 ? (
                  <div className="px-3 py-3 text-xs text-muted-foreground">
                    还没有项目，点击右侧 + 新建。
                  </div>
                ) : (
                  <div className="p-2 space-y-1">
                    {projects.map((project: Project) => (
                      <div
                        key={project.id}
                        className={`group relative flex items-center p-2 rounded-lg cursor-pointer transition-colors ${
                          currentProject?.id === project.id
                            ? 'bg-accent border border-border'
                            : 'hover:bg-accent/50'
                        }`}
                        onClick={() => handleSelectProject(project.id)}
                      >
                        <div className="flex-1 min-w-0">
                          <h3 className="text-sm font-medium text-foreground truncate">
                            {project.name}
                          </h3>
                          <div className="mt-1 text-xs text-muted-foreground truncate" title={project.rootPath}>
                            {project.rootPath}
                          </div>
                        </div>
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
                                  handleDeleteProject(project.id);
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
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      )}

      {/* 项目创建弹窗 */}
      {projectModalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="fixed inset-0 bg-black/50" onClick={() => setProjectModalOpen(false)} />
          <div className="relative bg-card border border-border rounded-lg shadow-xl w-[520px] p-6">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-foreground">新增项目</h3>
              <button
                onClick={() => setProjectModalOpen(false)}
                className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
              >
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div className="space-y-4">
              <div>
                <label className="text-sm text-muted-foreground">项目目录</label>
                <div className="mt-1 flex items-center gap-2">
                  <input
                    value={projectRoot}
                    onChange={(e) => setProjectRoot(e.target.value)}
                    className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="选择或输入本地目录路径"
                  />
                  <button
                    type="button"
                    onClick={openDirPicker}
                    className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                  >
                    选择目录
                  </button>
                </div>
              </div>
              {projectRoot.trim() && (
                <div className="text-xs text-muted-foreground">
                  项目名称将默认使用：<span className="text-foreground">{deriveProjectName(projectRoot)}</span>
                </div>
              )}
              {projectError && (
                <div className="text-xs text-destructive">{projectError}</div>
              )}
            </div>
            <div className="mt-6 flex justify-end gap-2">
              <button
                onClick={() => setProjectModalOpen(false)}
                className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
              >
                取消
              </button>
              <button
                onClick={handleCreateProject}
                className="px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90"
              >
                创建
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 目录选择弹窗 */}
      {dirPickerOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="fixed inset-0 bg-black/50" onClick={() => setDirPickerOpen(false)} />
          <div className="relative bg-card border border-border rounded-lg shadow-xl w-[640px] max-h-[80vh] p-6 flex flex-col">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-lg font-semibold text-foreground">选择项目目录</h3>
              <button onClick={() => setDirPickerOpen(false)} className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors">
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div className="text-xs text-muted-foreground break-all">
              当前路径：<span className="text-foreground">{dirPickerPath || '请选择盘符/目录'}</span>
            </div>
            <div className="mt-3 flex items-center gap-2">
              <button
                type="button"
                onClick={() => loadDirEntries(dirPickerParent)}
                disabled={!dirPickerParent}
                className="px-3 py-1.5 rounded bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                返回上级
              </button>
              <button
                type="button"
                onClick={() => chooseDir(dirPickerPath)}
                disabled={!dirPickerPath}
                className="px-3 py-1.5 rounded bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                选择当前目录
              </button>
            </div>
            <div className="mt-3 flex-1 overflow-y-auto border border-border rounded">
              {dirPickerLoading && (
                <div className="p-4 text-sm text-muted-foreground">加载中...</div>
              )}
              {!dirPickerLoading && (dirPickerPath ? dirPickerEntries : dirPickerRoots).length === 0 && (
                <div className="p-4 text-sm text-muted-foreground">没有可用目录</div>
              )}
              {!dirPickerLoading && (dirPickerPath ? dirPickerEntries : dirPickerRoots).length > 0 && (
                <div className="divide-y divide-border">
                  {(dirPickerPath ? dirPickerEntries : dirPickerRoots).map((entry) => (
                    <button
                      key={entry.path}
                      type="button"
                      onClick={() => loadDirEntries(entry.path)}
                      className="w-full text-left px-4 py-2 hover:bg-accent flex items-center gap-2"
                    >
                      <span className="text-foreground">{entry.name}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
            {dirPickerError && (
              <div className="mt-2 text-xs text-red-500">{dirPickerError}</div>
            )}
          </div>
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
