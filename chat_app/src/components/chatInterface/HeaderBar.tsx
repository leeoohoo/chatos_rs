import React, { useEffect, useMemo, useRef, useState } from 'react';
import type { AuthUser } from '../../lib/auth/authStore';
import { ThemeToggle } from '../ThemeToggle';

interface HeaderBarProps {
  headerTitle: string;
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
  onOpenNotepad: () => void;
  onOpenApplications: () => void;
  onOpenMcpManager: () => void;
  onOpenAgentManager: () => void;
  onOpenAiModelManager: () => void;
  onOpenSystemContextEditor: () => void;
  onOpenUserSettings: () => void;
  onLogout: () => void;
  user: AuthUser | null;
}

const HeaderBar: React.FC<HeaderBarProps> = ({
  headerTitle,
  sidebarOpen,
  onToggleSidebar,
  onOpenNotepad,
  onOpenApplications,
  onOpenMcpManager,
  onOpenAgentManager,
  onOpenAiModelManager,
  onOpenSystemContextEditor,
  onOpenUserSettings,
  onLogout,
  user,
}) => {
  const [showUserMenu, setShowUserMenu] = useState(false);
  const userMenuRef = useRef<HTMLDivElement | null>(null);

  const userDisplayName = useMemo(() => (
    user?.display_name?.trim()
    || user?.username?.trim()
    || user?.email?.trim()
    || user?.id
    || '当前用户'
  ), [user]);
  const userInitial = useMemo(() => (
    userDisplayName.trim().charAt(0).toUpperCase() || 'U'
  ), [userDisplayName]);

  useEffect(() => {
    if (!showUserMenu) {
      return;
    }

    const onDocumentClick = (event: MouseEvent) => {
      const target = event.target as Node;
      if (showUserMenu && userMenuRef.current && !userMenuRef.current.contains(target)) {
        setShowUserMenu(false);
      }
    };

    document.addEventListener('mousedown', onDocumentClick);
    return () => document.removeEventListener('mousedown', onDocumentClick);
  }, [showUserMenu]);

  return (
    <div className="flex items-center justify-between p-4 bg-card border-b border-border">
      <div className="flex items-center space-x-3">
        <button
          onClick={onToggleSidebar}
          className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          title={sidebarOpen ? '收起会话列表' : '展开会话列表'}
        >
          <svg className={`w-5 h-5 transition-transform ${sidebarOpen ? '' : 'rotate-180'}`} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="M15 18L9 12l6-6" />
          </svg>
        </button>

        {headerTitle ? (
          <div className="flex-1 min-w-0">
            <h1 className="text-lg font-semibold text-foreground truncate">
              {headerTitle}
            </h1>
          </div>
        ) : null}
      </div>

      <div className="flex items-center space-x-2">
        <button
          onClick={onOpenNotepad}
          className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          title="打开记事本"
        >
          <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
            <path d="M7 3h10a2 2 0 0 1 2 2v14l-3-2-3 2-3-2-3 2V5a2 2 0 0 1 2-2z" strokeWidth="1.8" />
          </svg>
        </button>
        <button
          onClick={onOpenApplications}
          className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          title="打开应用列表"
        >
          <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
            <path d="M4 5h6v14H4z" strokeWidth="2" />
            <path d="M12 5h8v14h-8z" strokeWidth="2" />
          </svg>
        </button>
        <ThemeToggle />
        <div className="relative" ref={userMenuRef}>
          <button
            onClick={() => {
              setShowUserMenu((prev) => !prev);
            }}
            className="flex items-center gap-2 pl-2 pr-3 py-1.5 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="当前用户"
          >
            <span className="w-6 h-6 rounded-full bg-primary/15 text-primary text-xs font-semibold flex items-center justify-center">
              {userInitial}
            </span>
            <span className="text-sm max-w-[140px] truncate">
              {userDisplayName}
            </span>
          </button>
          {showUserMenu ? (
            <div className="absolute right-0 mt-2 w-64 bg-popover border border-border rounded-lg shadow-lg z-50 py-1">
              <div className="px-3 py-2 border-b border-border">
                <div className="text-sm font-medium text-foreground truncate">
                  {user?.display_name?.trim() || '未设置昵称'}
                </div>
                <div className="text-xs text-muted-foreground truncate mt-0.5">
                  {user?.username || user?.email || user?.id}
                </div>
              </div>
              <button
                onClick={() => {
                  setShowUserMenu(false);
                  onOpenMcpManager();
                }}
                className="w-full text-left px-3 py-2 text-sm hover:bg-accent"
              >
                MCP 服务管理
              </button>
              <button
                onClick={() => {
                  setShowUserMenu(false);
                  onOpenAgentManager();
                }}
                className="w-full text-left px-3 py-2 text-sm hover:bg-accent"
              >
                智能体管理
              </button>
              <button
                onClick={() => {
                  setShowUserMenu(false);
                  onOpenAiModelManager();
                }}
                className="w-full text-left px-3 py-2 text-sm hover:bg-accent"
              >
                AI 模型管理
              </button>
              <button
                onClick={() => {
                  setShowUserMenu(false);
                  onOpenSystemContextEditor();
                }}
                className="w-full text-left px-3 py-2 text-sm hover:bg-accent"
              >
                系统上下文设置
              </button>
              <button
                onClick={() => {
                  setShowUserMenu(false);
                  onOpenUserSettings();
                }}
                className="w-full text-left px-3 py-2 text-sm hover:bg-accent"
              >
                用户参数设置
              </button>
              <div className="my-1 border-t border-border" />
              <button
                onClick={() => {
                  setShowUserMenu(false);
                  onLogout();
                }}
                className="w-full text-left px-3 py-2 text-sm text-red-600 hover:bg-accent"
              >
                退出登录
              </button>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
};

export default HeaderBar;
