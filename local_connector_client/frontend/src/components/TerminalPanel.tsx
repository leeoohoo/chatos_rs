// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { ListChecks, Play, RefreshCw, Terminal, Trash2 } from 'lucide-react';

import { api, type CommandHistoryEntry, type ConnectorStatus } from '../api';
import {
  formatHistoryTime,
  formatTerminalResult,
  historyStatusClass,
  sourceGroup,
  sourceLabel,
  splitArgs,
  statusLabel,
} from '../utils/terminalFormat';

export function TerminalPanel({ status }: { status: ConnectorStatus }) {
  const [workspaceId, setWorkspaceId] = React.useState(status.workspaces[0]?.id || '');
  const [command, setCommand] = React.useState('pwd');
  const [args, setArgs] = React.useState('');
  const [output, setOutput] = React.useState('');
  const [running, setRunning] = React.useState(false);
  const [history, setHistory] = React.useState<CommandHistoryEntry[]>([]);
  const [sourceFilter, setSourceFilter] = React.useState('all');
  const [historyLoading, setHistoryLoading] = React.useState(false);
  const [historyError, setHistoryError] = React.useState<string | null>(null);

  React.useEffect(() => {
    if (!workspaceId && status.workspaces[0]?.id) {
      setWorkspaceId(status.workspaces[0].id);
    }
  }, [status.workspaces, workspaceId]);

  const refreshHistory = React.useCallback(async () => {
    setHistoryLoading(true);
    setHistoryError(null);
    try {
      const result = await api.commandHistory({
        limit: 200,
      });
      setHistory(result.entries);
    } catch (err) {
      setHistoryError(err instanceof Error ? err.message : '读取命令历史失败');
    } finally {
      setHistoryLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void refreshHistory();
  }, [refreshHistory]);

  React.useEffect(() => {
    const interval = window.setInterval(() => {
      void refreshHistory();
    }, 5000);
    return () => window.clearInterval(interval);
  }, [refreshHistory]);

  const run = async () => {
    if (!workspaceId || !command.trim()) {
      return;
    }
    setRunning(true);
    try {
      const result = await api.terminalExec({
        workspace_id: workspaceId,
        command: command.trim(),
        args: splitArgs(args),
      });
      setOutput(formatTerminalResult(result));
      await refreshHistory();
    } catch (err) {
      setOutput(err instanceof Error ? err.message : '执行失败');
    } finally {
      setRunning(false);
    }
  };

  const clearHistory = async () => {
    setHistoryError(null);
    try {
      const result = await api.clearCommandHistory();
      setHistory(result.entries);
    } catch (err) {
      setHistoryError(err instanceof Error ? err.message : '清空命令历史失败');
    }
  };

  const visibleHistory = React.useMemo(
    () =>
      history.filter((entry) => {
        if (sourceFilter === 'all') {
          return true;
        }
        return sourceGroup(entry.source) === sourceFilter;
      }),
    [history, sourceFilter],
  );

  return (
    <section className="terminalPage">
      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><Terminal size={18} />终端执行</h2>
            <p>这里通过云端 relay 回到本机执行，用来验证 ChatOS 侧终端链路。</p>
          </div>
        </div>
        <div className="terminalForm">
          <select value={workspaceId} onChange={(event) => setWorkspaceId(event.target.value)}>
            {status.workspaces.map((workspace) => (
              <option value={workspace.id} key={workspace.id}>{workspace.alias}</option>
            ))}
          </select>
          <input value={command} onChange={(event) => setCommand(event.target.value)} placeholder="command" />
          <input value={args} onChange={(event) => setArgs(event.target.value)} placeholder="args, e.g. check -p app" />
          <button className="primaryButton compact" disabled={running || !workspaceId} onClick={() => void run()}>
            <Play size={16} />{running ? '执行中' : '执行'}
          </button>
        </div>
        <pre className="output">{output || '暂无输出'}</pre>
      </section>

      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><ListChecks size={18} />命令历史</h2>
            <p>展示 ChatOS、Task Runner 和当前页面触发过的本机执行记录。</p>
          </div>
          <div className="headerActions terminalHistoryActions">
            <select value={sourceFilter} onChange={(event) => setSourceFilter(event.target.value)}>
              <option value="all">全部来源</option>
              <option value="chatos_terminal">ChatOS 终端</option>
              <option value="task_runner">Task Runner</option>
              <option value="local_connector_ui">Local Connector 页面</option>
            </select>
            <button className="iconButton" onClick={() => void refreshHistory()} title="刷新命令历史">
              <RefreshCw size={17} />
            </button>
            <button className="iconButton danger" onClick={() => void clearHistory()} title="清空命令历史">
              <Trash2 size={17} />
            </button>
          </div>
        </div>
        {historyError ? <div className="formError">{historyError}</div> : null}
        <div className="commandHistoryList">
          {visibleHistory.map((entry) => (
            <details className="commandHistoryRow" key={entry.id}>
              <summary>
                <div className="commandHistoryMain">
                  <div className="historyMetaLine">
                    <span className="historySource">{sourceLabel(entry.source)}</span>
                    <span className={historyStatusClass(entry.status)}>{statusLabel(entry.status)}</span>
                    {typeof entry.exit_code === 'number' ? <span className="historyExit">exit {entry.exit_code}</span> : null}
                    {entry.tool_name ? <span className="historyTool">{entry.tool_name}</span> : null}
                  </div>
                  <strong className="commandDisplay">{entry.display || entry.command}</strong>
                  <span className="historySubline">
                    {formatHistoryTime(entry.started_at)}
                    {entry.workspace_alias ? ` · ${entry.workspace_alias}` : ''}
                    {entry.cwd ? ` · ${entry.cwd}` : ''}
                  </span>
                </div>
              </summary>
              <div className="historyDetails">
                {entry.request_id ? <div><span>request</span><code>{entry.request_id}</code></div> : null}
                {entry.terminal_session_id ? <div><span>session</span><code>{entry.terminal_session_id}</code></div> : null}
                {entry.sandbox_id ? <div><span>sandbox</span><code>{entry.sandbox_id}</code></div> : null}
                {entry.error ? <pre className="historyPreview errorPreview">{entry.error}</pre> : null}
                {entry.stdout_preview ? <pre className="historyPreview">{entry.stdout_preview}</pre> : null}
                {entry.stderr_preview ? <pre className="historyPreview errorPreview">{entry.stderr_preview}</pre> : null}
                {!entry.error && !entry.stdout_preview && !entry.stderr_preview ? (
                  <div className="emptyState compactEmpty">暂无输出预览</div>
                ) : null}
              </div>
            </details>
          ))}
          {!visibleHistory.length ? (
            <div className="emptyState">{historyLoading ? '正在读取命令历史...' : '还没有命令历史'}</div>
          ) : null}
        </div>
      </section>
    </section>
  );
}
