import React from 'react';

import { formatFileSize } from '../../../lib/utils';
import type { FsReadResult } from '../../../types';

interface ProjectPreviewHeaderProps {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
  runnerScriptExists: boolean;
  generating: boolean;
  projectMembersLoading: boolean;
  runnerScriptChecking: boolean;
  runStatus: string;
  runCatalogLoading: boolean;
  starting: boolean;
  stopping: boolean;
  restarting: boolean;
  runnerStartCommand: string;
  runnerStopCommand: string;
  runnerRestartCommand: string;
  onGenerateClick: () => void;
  onRunnerStart: () => void;
  onRunnerStop: () => void;
  onRunnerRestart: () => void;
  onRefreshRunnerState: () => void;
}

export const ProjectPreviewHeader: React.FC<ProjectPreviewHeaderProps> = ({
  selectedFile,
  selectedPath,
  runnerScriptExists,
  generating,
  projectMembersLoading,
  runnerScriptChecking,
  runStatus,
  runCatalogLoading,
  starting,
  stopping,
  restarting,
  runnerStartCommand,
  runnerStopCommand,
  runnerRestartCommand,
  onGenerateClick,
  onRunnerStart,
  onRunnerStop,
  onRunnerRestart,
  onRefreshRunnerState,
}) => (
  <div className="flex items-center justify-between border-b border-border bg-card px-4 py-2">
    <div className="min-w-0 flex-1">
      <div className="truncate text-sm font-medium text-foreground">
        {selectedFile?.name || (selectedPath ? '文件预览（当前项不可预览）' : '文件预览')}
      </div>
      <div className="truncate text-[11px] text-muted-foreground">
        {selectedFile?.path || selectedPath || '请选择文件'}
      </div>
    </div>
    <div className="ml-3 flex items-center gap-2">
      {!runnerScriptExists ? (
        <button
          type="button"
          onClick={onGenerateClick}
          disabled={generating || projectMembersLoading || runnerScriptChecking}
          className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
          title="向团队成员发送固定提示词，生成项目启动脚本"
        >
          {generating ? '生成请求中...' : '生成启动脚本'}
        </button>
      ) : (
        <>
          <button
            type="button"
            onClick={onRunnerStart}
            disabled={
              runStatus === 'no_member'
              || runStatus === 'missing_root'
              || starting
              || stopping
              || restarting
              || generating
              || runnerScriptChecking
            }
            className="h-8 rounded border border-emerald-500/40 px-3 text-xs text-emerald-700 hover:bg-emerald-500/10 disabled:cursor-not-allowed disabled:opacity-50"
            title={runnerStartCommand}
          >
            {starting ? '启动中...' : '启动'}
          </button>
          <button
            type="button"
            onClick={onRunnerStop}
            disabled={
              runStatus === 'no_member'
              || runStatus === 'missing_root'
              || starting
              || stopping
              || restarting
              || generating
              || runnerScriptChecking
            }
            className="h-8 rounded border border-rose-500/40 px-3 text-xs text-rose-700 hover:bg-rose-500/10 disabled:cursor-not-allowed disabled:opacity-50"
            title={runnerStopCommand}
          >
            {stopping ? '停止中...' : '停止'}
          </button>
          <button
            type="button"
            onClick={onRunnerRestart}
            disabled={
              runStatus === 'no_member'
              || runStatus === 'missing_root'
              || starting
              || stopping
              || restarting
              || generating
              || runnerScriptChecking
            }
            className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
            title={runnerRestartCommand}
          >
            {restarting ? '重启中...' : '重启'}
          </button>
        </>
      )}
      <button
        type="button"
        onClick={onRefreshRunnerState}
        disabled={runCatalogLoading || runnerScriptChecking || projectMembersLoading}
        className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
      >
        {runnerScriptChecking ? '检查中...' : '刷新状态'}
      </button>
      {selectedFile && (
        <div className="whitespace-nowrap text-[11px] text-muted-foreground">
          {formatFileSize(selectedFile.size)}
        </div>
      )}
    </div>
  </div>
);
