import React, { useMemo, useState } from 'react';
import hljs from 'highlight.js';

import type { ChangeLogItem, FsEntry, FsReadResult } from '../../types';
import { formatFileSize } from '../../lib/utils';
import { DiffPanel } from './ChangeLogPanels';
import type {
  ProjectRunnerActiveTerminal,
  ProjectRunnerMember,
} from './useProjectExplorerRunState';
import { escapeHtml, getHighlightLanguage } from './utils';

interface ProjectPreviewPaneProps {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  loadingFile: boolean;
  error: string | null;
  selectedLog: ChangeLogItem | null;
  projectRootPath: string;
  runStatus: string;
  runCatalogLoading: boolean;
  runCatalogError: string | null;
  projectMembers: ProjectRunnerMember[];
  projectMembersLoading: boolean;
  projectMembersError: string | null;
  runnerScriptExists: boolean;
  runnerScriptChecking: boolean;
  runnerScriptPath: string;
  runnerStartCommand: string;
  runnerStopCommand: string;
  runnerRestartCommand: string;
  starting: boolean;
  stopping: boolean;
  restarting: boolean;
  runnerMessage: string | null;
  runnerError: string | null;
  activeRun: ProjectRunnerActiveTerminal | null;
  activeTerminalBusy: boolean;
  onRunnerStart: () => void;
  onRunnerStop: () => void;
  onRunnerRestart: () => void;
  onRefreshRunnerState: () => void;
  onGenerateRunnerScriptForContact: (member: ProjectRunnerMember) => Promise<void>;
}

export const ProjectPreviewPane: React.FC<ProjectPreviewPaneProps> = ({
  selectedFile,
  selectedPath,
  selectedEntry,
  loadingFile,
  error,
  selectedLog,
  projectRootPath,
  runStatus,
  runCatalogLoading,
  runCatalogError,
  projectMembers,
  projectMembersLoading,
  projectMembersError,
  runnerScriptExists,
  runnerScriptChecking,
  runnerScriptPath,
  runnerStartCommand,
  runnerStopCommand,
  runnerRestartCommand,
  starting,
  stopping,
  restarting,
  runnerMessage,
  runnerError,
  activeRun,
  activeTerminalBusy,
  onRunnerStart,
  onRunnerStop,
  onRunnerRestart,
  onRefreshRunnerState,
  onGenerateRunnerScriptForContact,
}) => {
  const [memberPickerOpen, setMemberPickerOpen] = useState(false);
  const [memberPickerSelectedId, setMemberPickerSelectedId] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [generationError, setGenerationError] = useState<string | null>(null);
  const [generationMessage, setGenerationMessage] = useState<string | null>(null);

  const selectedMember = useMemo(
    () => projectMembers.find((member) => member.contactId === memberPickerSelectedId) || null,
    [memberPickerSelectedId, projectMembers]
  );

  const preview = useMemo(() => {
    if (loadingFile) {
      return <div className="p-4 text-sm text-muted-foreground">加载文件中...</div>;
    }
    if (!selectedFile) {
      if (selectedPath && !selectedEntry) {
        return (
          <div className="p-4 text-sm text-muted-foreground">
            该路径已删除或不存在，当前仅支持查看变更记录。
          </div>
        );
      }
      return <div className="p-4 text-sm text-muted-foreground">请选择文件以预览</div>;
    }
    const isImage = selectedFile.contentType.startsWith('image/');
    if (isImage && selectedFile.isBinary) {
      const src = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
      return (
        <div className="p-4 overflow-auto h-full">
          <img src={src} alt={selectedFile.name} className="max-w-full max-h-full rounded border border-border" />
        </div>
      );
    }
    if (!selectedFile.isBinary) {
      const language = getHighlightLanguage(selectedFile.name);
      let highlighted = '';
      try {
        if (language) {
          highlighted = hljs.highlight(selectedFile.content, { language }).value;
        } else {
          highlighted = hljs.highlightAuto(selectedFile.content).value;
        }
      } catch {
        highlighted = escapeHtml(selectedFile.content);
      }
      const lines = highlighted.split(/\r?\n/);
      return (
        <div className="h-full overflow-auto bg-muted/30">
          <div className="flex min-h-full text-sm">
            <div className="shrink-0 py-4 pr-3 pl-2 border-r border-border text-right text-muted-foreground select-none">
              {lines.map((_, idx) => (
                <div key={idx} className="leading-5">
                  {idx + 1}
                </div>
              ))}
            </div>
            <div className="flex-1 min-w-0 py-4 pl-3 pr-4 hljs">
              {lines.map((line, idx) => (
                <div
                  key={idx}
                  className="leading-5 font-mono whitespace-pre w-full"
                  dangerouslySetInnerHTML={{ __html: line || '&nbsp;' }}
                />
              ))}
            </div>
          </div>
        </div>
      );
    }
    const downloadHref = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
    return (
      <div className="p-4 text-sm text-muted-foreground space-y-2">
        <div>该文件为二进制内容，暂不支持直接预览。</div>
        <a
          href={downloadHref}
          download={selectedFile.name || 'file'}
          className="inline-flex items-center px-3 py-1.5 rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
        >
          下载文件
        </a>
      </div>
    );
  }, [loadingFile, selectedEntry, selectedFile, selectedPath]);

  const runModeLabel = useMemo(() => {
    if (runStatus === 'loading') return '检查中';
    if (runStatus === 'no_member') return '缺少团队成员';
    if (runStatus === 'script_missing') return '缺少启动脚本';
    if (runStatus === 'ready') return '可运行';
    if (runStatus === 'error') return '异常';
    return runStatus || '-';
  }, [runStatus]);

  const runModeHint = useMemo(() => {
    if (runStatus === 'no_member') {
      return '请先在 TEAM 面板为当前项目添加至少一个联系人。';
    }
    if (runStatus === 'script_missing') {
      return '请先点击“生成启动脚本”，由团队成员在项目根目录生成脚本。';
    }
    return null;
  }, [runStatus]);

  const runnerScriptAbsolutePath = useMemo(() => {
    const root = (projectRootPath || '').trim().replace(/[\\/]+$/, '');
    if (!root) {
      return runnerScriptPath;
    }
    return `${root}/${runnerScriptPath}`;
  }, [projectRootPath, runnerScriptPath]);

  const mergeError = runnerError || generationError;
  const mergeMessage = !mergeError ? (generationMessage || runnerMessage) : null;

  const handleGenerateForMember = async (member: ProjectRunnerMember): Promise<boolean> => {
    setGenerating(true);
    setGenerationError(null);
    setGenerationMessage(null);
    try {
      await onGenerateRunnerScriptForContact(member);
      setGenerationMessage(`已向 ${member.name || member.contactId} 发送脚本生成任务`);
      await onRefreshRunnerState();
      return true;
    } catch (error) {
      setGenerationError(error instanceof Error ? error.message : '发送脚本生成任务失败');
      return false;
    } finally {
      setGenerating(false);
    }
  };

  const handleGenerateClick = () => {
    setGenerationError(null);
    setGenerationMessage(null);
    if (projectMembersLoading) {
      return;
    }
    if (projectMembers.length === 0) {
      setGenerationError('当前项目还没有团队成员，请先添加联系人');
      return;
    }
    if (projectMembers.length === 1) {
      void handleGenerateForMember(projectMembers[0]);
      return;
    }
    setMemberPickerSelectedId(projectMembers[0]?.contactId || null);
    setMemberPickerOpen(true);
  };

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="px-4 py-2 border-b border-border bg-card flex items-center justify-between">
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium text-foreground truncate">
            {selectedFile?.name || (selectedPath ? '文件预览（当前项不可预览）' : '文件预览')}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            {selectedFile?.path || selectedPath || '请选择文件'}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            项目根目录：{projectRootPath || '-'}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            运行模式：脚本托管 / {runModeLabel}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            启动脚本：{runnerScriptExists ? `已生成（${runnerScriptAbsolutePath}）` : `未生成（${runnerScriptAbsolutePath}）`}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            团队成员：{projectMembersLoading ? '加载中...' : projectMembers.length}
          </div>
          {activeRun && (
            <div className="text-[11px] text-muted-foreground truncate">
              <span className={activeTerminalBusy ? 'text-emerald-600' : 'text-slate-500'}>●</span>
              {' '}终端：{activeRun.terminalName || activeRun.terminalId} / {activeTerminalBusy ? '运行中' : '空闲'}
            </div>
          )}
          {runModeHint && (
            <div className="text-[11px] text-muted-foreground truncate" title={runModeHint}>
              {runModeHint}
            </div>
          )}
          {(projectMembersError || runCatalogError) && (
            <div className="text-[11px] text-destructive truncate" title={projectMembersError || runCatalogError || ''}>
              {projectMembersError || runCatalogError}
            </div>
          )}
        </div>
        <div className="ml-3 flex items-center gap-2">
          {!runnerScriptExists ? (
            <button
              type="button"
              onClick={handleGenerateClick}
              disabled={generating || projectMembersLoading || runnerScriptChecking}
              className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              title="向团队成员发送固定提示词，生成项目启动脚本"
            >
              {generating ? '生成请求中...' : '生成启动脚本'}
            </button>
          ) : (
            <>
              <button
                type="button"
                onClick={() => { void onRunnerStart(); }}
                disabled={runStatus === 'no_member' || starting || stopping || restarting || generating || runnerScriptChecking}
                className="h-8 rounded border border-emerald-500/40 px-3 text-xs text-emerald-700 hover:bg-emerald-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
                title={runnerStartCommand}
              >
                {starting ? '启动中...' : '启动'}
              </button>
              <button
                type="button"
                onClick={() => { void onRunnerStop(); }}
                disabled={runStatus === 'no_member' || starting || stopping || restarting || generating || runnerScriptChecking}
                className="h-8 rounded border border-rose-500/40 px-3 text-xs text-rose-700 hover:bg-rose-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
                title={runnerStopCommand}
              >
                {stopping ? '停止中...' : '停止'}
              </button>
              <button
                type="button"
                onClick={() => { void onRunnerRestart(); }}
                disabled={runStatus === 'no_member' || starting || stopping || restarting || generating || runnerScriptChecking}
                className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
                title={runnerRestartCommand}
              >
                {restarting ? '重启中...' : '重启'}
              </button>
            </>
          )}
          <button
            type="button"
            onClick={() => { void onRefreshRunnerState(); }}
            disabled={runCatalogLoading || runnerScriptChecking || projectMembersLoading}
            className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {runnerScriptChecking ? '检查中...' : '刷新状态'}
          </button>
          {selectedFile && (
            <div className="text-[11px] text-muted-foreground whitespace-nowrap">
              {formatFileSize(selectedFile.size)}
            </div>
          )}
        </div>
      </div>
      {(mergeMessage || mergeError) && (
        <div className="px-4 py-1.5 border-b border-border/70 bg-card">
          <div className={mergeError ? 'text-[11px] text-destructive' : 'text-[11px] text-emerald-600'}>
            {mergeError || mergeMessage}
          </div>
        </div>
      )}
      <div className="flex-1 overflow-hidden flex flex-col">
        <DiffPanel selectedLog={selectedLog} />
        <div className="flex-1 min-h-0 overflow-hidden">
          {error ? (
            <div className="p-4 text-sm text-destructive">{error}</div>
          ) : (
            preview
          )}
        </div>
      </div>

      {memberPickerOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            className="absolute inset-0 bg-black/50"
            onClick={() => {
              if (generating) return;
              setMemberPickerOpen(false);
            }}
            aria-label="关闭成员选择"
          />
          <div className="relative w-[520px] max-w-[calc(100vw-24px)] rounded-lg border border-border bg-card p-5 shadow-xl">
            <div className="mb-1 text-base font-semibold text-foreground">选择执行成员</div>
            <div className="mb-3 text-xs text-muted-foreground">
              请选择一个团队成员来生成 `${runnerScriptPath}`。
            </div>
            <div className="max-h-72 overflow-y-auto rounded border border-border">
              {projectMembers.map((member) => {
                const active = member.contactId === memberPickerSelectedId;
                return (
                  <button
                    key={member.contactId}
                    type="button"
                    onClick={() => setMemberPickerSelectedId(member.contactId)}
                    className={`w-full border-b border-border px-3 py-2 text-left last:border-b-0 ${active ? 'bg-accent' : 'hover:bg-accent/50'}`}
                  >
                    <div className="text-sm text-foreground truncate">{member.name || member.contactId}</div>
                    <div className="text-[11px] text-muted-foreground truncate">{member.agentId}</div>
                  </button>
                );
              })}
            </div>
            {generationError && (
              <div className="mt-3 text-xs text-destructive">{generationError}</div>
            )}
            <div className="mt-4 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => {
                  if (generating) return;
                  setMemberPickerOpen(false);
                }}
                disabled={generating}
                className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => {
                  if (!selectedMember) return;
                  void handleGenerateForMember(selectedMember).then((success) => {
                    if (success) {
                      setMemberPickerOpen(false);
                    }
                  });
                }}
                disabled={!selectedMember || generating}
                className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {generating ? '提交中...' : '确认并执行'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
