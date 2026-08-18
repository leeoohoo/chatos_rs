// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { Cloud, Laptop, Maximize2, Minimize2, X } from 'lucide-react';

import { cn } from '../../../lib/utils';

export const requirementExecutionModalShellClassName = (fullscreen: boolean): string => cn(
  'absolute flex flex-col overflow-hidden border border-border bg-card shadow-2xl',
  fullscreen
    ? 'inset-0 h-[100dvh] w-screen max-w-none rounded-none border-0'
    : 'left-1/2 top-1/2 h-[94dvh] w-[calc(100vw-20px)] max-w-[1500px] -translate-x-1/2 -translate-y-1/2 rounded-xl sm:w-[calc(100vw-36px)]',
);

export const FullscreenToggleButton: React.FC<{
  fullscreen: boolean;
  onToggle: () => void;
}> = ({ fullscreen, onToggle }) => (
  <button
    type="button"
    className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
    onClick={onToggle}
    aria-label={fullscreen ? '退出全屏' : '全屏'}
    aria-pressed={fullscreen}
    title={fullscreen ? '退出全屏' : '全屏显示'}
  >
    {fullscreen ? <Minimize2 className="h-3.5 w-3.5" /> : <Maximize2 className="h-3.5 w-3.5" />}
    <span className="hidden sm:inline">{fullscreen ? '退出全屏' : '全屏'}</span>
  </button>
);

export const RequirementExecutionModalFrame: React.FC<{
  children: React.ReactNode;
  fullscreen: boolean;
  headerActions?: React.ReactNode;
  isLocalExecution: boolean;
  onClose: () => void;
  onToggleFullscreen: () => void;
  requirementTitle: string;
}> = ({
  children,
  fullscreen,
  headerActions,
  isLocalExecution,
  onClose,
  onToggleFullscreen,
  requirementTitle,
}) => (
  <div className="fixed inset-0 z-[50]" role="dialog" aria-modal="true" aria-label="执行计划工作台">
    <button
      type="button"
      aria-label="关闭执行计划工作台"
      className="absolute inset-0 bg-black/55"
      onClick={onClose}
    />
    <section
      className={requirementExecutionModalShellClassName(fullscreen)}
      data-fullscreen={fullscreen}
    >
      <header className="flex shrink-0 items-start justify-between gap-4 border-b border-border px-4 py-3 sm:px-5">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-base font-semibold text-foreground">执行计划工作台</h2>
            <span className={cn(
              'inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium',
              isLocalExecution
                ? 'border-violet-200 bg-violet-50 text-violet-700 dark:border-violet-800 dark:bg-violet-950/30 dark:text-violet-200'
                : 'border-sky-200 bg-sky-50 text-sky-700 dark:border-sky-800 dark:bg-sky-950/30 dark:text-sky-200',
            )}
            >
              {isLocalExecution ? <Laptop className="h-3 w-3" /> : <Cloud className="h-3 w-3" />}
              {isLocalExecution ? '云端编排 / Local Connector 承载' : '云端编排 / 云端承载'}
            </span>
          </div>
          <p className="mt-1 truncate text-sm text-muted-foreground">{requirementTitle}</p>
          <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
            {isLocalExecution
              ? '任务计划由云端统一生成，本机目录、命令和沙箱能力会通过 Local Connector 网关受控执行。'
              : '任务计划与执行资源都由云端统一编排和承载。'}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {headerActions}
          <FullscreenToggleButton fullscreen={fullscreen} onToggle={onToggleFullscreen} />
          <button
            type="button"
            className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
            onClick={onClose}
            aria-label="关闭"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      </header>
      {children}
    </section>
  </div>
);
