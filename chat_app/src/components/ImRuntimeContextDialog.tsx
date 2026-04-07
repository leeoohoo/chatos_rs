import React from 'react';

import { apiClient } from '../lib/api/client';
import { cn } from '../lib/utils';
import type {
  TurnRuntimeSnapshotLookupResponse,
  TurnRuntimeSnapshotResponse,
} from '../lib/api/client/types';

interface ImRuntimeContextDialogProps {
  open: boolean;
  sessionId: string;
  turnId?: string | null;
  onClose: () => void;
}

const normalizeText = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const prettyJson = (value: unknown): string => {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value ?? '');
  }
};

const ContextSection: React.FC<{
  title: string;
  children: React.ReactNode;
  className?: string;
}> = ({ title, children, className }) => (
  <section className={cn('rounded-2xl border border-slate-200 bg-white p-4 shadow-sm dark:border-slate-800 dark:bg-slate-950', className)}>
    <div className="mb-3 text-sm font-semibold text-slate-900 dark:text-slate-100">{title}</div>
    {children}
  </section>
);

const KeyValueRow: React.FC<{
  label: string;
  value: React.ReactNode;
}> = ({ label, value }) => (
  <div className="grid grid-cols-[120px_minmax(0,1fr)] gap-3 border-b border-slate-100 py-2 text-sm last:border-b-0 dark:border-slate-800">
    <div className="text-slate-500 dark:text-slate-400">{label}</div>
    <div className="min-w-0 break-words text-slate-900 dark:text-slate-100">{value}</div>
  </div>
);

const RawBlock: React.FC<{
  value: string;
}> = ({ value }) => (
  <pre className="max-h-[320px] overflow-auto rounded-xl bg-slate-950 px-4 py-3 text-xs leading-6 text-slate-100 whitespace-pre-wrap break-words">
    {value}
  </pre>
);

const renderSnapshot = (snapshot: TurnRuntimeSnapshotResponse) => {
  const runtime = snapshot.runtime || null;
  const systemMessages = Array.isArray(snapshot.system_messages) ? snapshot.system_messages : [];
  const tools = Array.isArray(snapshot.tools) ? snapshot.tools : [];
  const enabledMcpIds = Array.isArray(runtime?.enabled_mcp_ids) ? runtime?.enabled_mcp_ids : [];
  const selectedCommands = Array.isArray(runtime?.selected_commands)
    ? runtime?.selected_commands
    : [];

  return (
    <div className="space-y-4">
      <ContextSection title="快照信息">
        <div className="space-y-0">
          <KeyValueRow label="Turn ID" value={snapshot.turn_id || '-'} />
          <KeyValueRow label="状态" value={snapshot.status || '-'} />
          <KeyValueRow label="来源" value={snapshot.snapshot_source || '-'} />
          <KeyValueRow label="捕获时间" value={snapshot.captured_at || '-'} />
        </div>
      </ContextSection>

      <ContextSection title={`System Messages (${systemMessages.length})`}>
        <div className="space-y-4">
          {systemMessages.length === 0 ? (
            <div className="text-sm text-slate-500 dark:text-slate-400">没有记录到 system message。</div>
          ) : systemMessages.map((item) => (
            <div key={item.id} className="rounded-xl border border-slate-200 p-3 dark:border-slate-800">
              <div className="mb-2 flex flex-wrap items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
                <span className="rounded-full bg-slate-100 px-2 py-1 dark:bg-slate-800">{item.id}</span>
                <span>{item.source}</span>
              </div>
              <RawBlock value={item.content || ''} />
            </div>
          ))}
        </div>
      </ContextSection>

      <ContextSection title="运行时参数">
        <div className="space-y-0">
          <KeyValueRow label="模型" value={runtime?.model || '-'} />
          <KeyValueRow label="Provider" value={runtime?.provider || '-'} />
          <KeyValueRow label="联系人 Agent" value={runtime?.contact_agent_id || '-'} />
          <KeyValueRow label="项目 ID" value={runtime?.project_id || '-'} />
          <KeyValueRow label="项目目录" value={runtime?.project_root || '-'} />
          <KeyValueRow label="工作目录" value={runtime?.workspace_root || '-'} />
          <KeyValueRow label="远程连接" value={runtime?.remote_connection_id || '-'} />
          <KeyValueRow
            label="MCP 开关"
            value={typeof runtime?.mcp_enabled === 'boolean' ? (runtime.mcp_enabled ? '开启' : '关闭') : '-'}
          />
          <KeyValueRow
            label="启用 MCP"
            value={enabledMcpIds.length > 0 ? enabledMcpIds.join(', ') : '-'}
          />
        </div>
      </ContextSection>

      <ContextSection title={`模型可见工具 (${tools.length})`}>
        <div className="space-y-3">
          {tools.length === 0 ? (
            <div className="text-sm text-slate-500 dark:text-slate-400">没有工具快照。</div>
          ) : tools.map((tool) => (
            <div key={`${tool.server_name}:${tool.name}`} className="rounded-xl border border-slate-200 p-3 dark:border-slate-800">
              <div className="text-sm font-medium text-slate-900 dark:text-slate-100">{tool.name}</div>
              <div className="mt-1 text-xs text-slate-500 dark:text-slate-400">
                {tool.server_name} · {tool.server_type}
              </div>
              {normalizeText(tool.description) ? (
                <div className="mt-2 text-sm text-slate-700 dark:text-slate-300">{tool.description}</div>
              ) : null}
            </div>
          ))}
        </div>
      </ContextSection>

      <ContextSection title={`已选命令上下文 (${selectedCommands.length})`}>
        <div className="space-y-3">
          {selectedCommands.length === 0 ? (
            <div className="text-sm text-slate-500 dark:text-slate-400">没有额外命令上下文。</div>
          ) : selectedCommands.map((item, index) => (
            <div key={`${item.plugin_source}:${item.source_path}:${index}`} className="rounded-xl border border-slate-200 p-3 dark:border-slate-800">
              <div className="space-y-0">
                <KeyValueRow label="名称" value={item.name || '-'} />
                <KeyValueRow label="命令引用" value={item.command_ref || '-'} />
                <KeyValueRow label="插件来源" value={item.plugin_source} />
                <KeyValueRow label="来源路径" value={item.source_path} />
                <KeyValueRow label="触发方式" value={item.trigger || '-'} />
              </div>
              {normalizeText(item.arguments) ? (
                <div className="mt-3">
                  <div className="mb-2 text-xs text-slate-500 dark:text-slate-400">参数</div>
                  <RawBlock value={item.arguments || ''} />
                </div>
              ) : null}
            </div>
          ))}
        </div>
      </ContextSection>
    </div>
  );
};

const ImRuntimeContextDialog: React.FC<ImRuntimeContextDialogProps> = ({
  open,
  sessionId,
  turnId,
  onClose,
}) => {
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [payload, setPayload] = React.useState<TurnRuntimeSnapshotLookupResponse | null>(null);

  React.useEffect(() => {
    if (!open || !sessionId) {
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);
    setPayload(null);

    const load = async () => {
      try {
        const response = normalizeText(turnId)
          ? await apiClient.getSessionTurnRuntimeContextByTurn(sessionId, turnId!.trim())
          : await apiClient.getSessionTurnRuntimeContextLatest(sessionId);
        if (!cancelled) {
          setPayload(response);
        }
      } catch (err) {
        if (!cancelled) {
          setPayload(null);
          setError(err instanceof Error ? err.message : '加载运行时上下文失败');
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, [open, sessionId, turnId]);

  React.useEffect(() => {
    if (!open) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open, onClose]);

  if (!open) {
    return null;
  }

  const snapshot = payload?.snapshot || null;

  return (
    <div className="fixed inset-0 z-[90] flex justify-end bg-slate-950/45 backdrop-blur-[1px]" onClick={onClose}>
      <div
        className="h-full w-full max-w-[920px] overflow-hidden border-l border-slate-200 bg-slate-50 shadow-2xl dark:border-slate-800 dark:bg-slate-900"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4 dark:border-slate-800">
          <div className="min-w-0">
            <div className="text-base font-semibold text-slate-900 dark:text-slate-100">本次模型上下文</div>
            <div className="mt-1 text-xs text-slate-500 dark:text-slate-400">
              session: {sessionId}
              {normalizeText(turnId) ? ` · turn: ${turnId}` : ''}
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-slate-200 px-3 py-1.5 text-sm text-slate-600 hover:bg-slate-100 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-800"
          >
            关闭
          </button>
        </div>

        <div className="h-[calc(100%-73px)] overflow-y-auto px-5 py-5">
          {loading ? (
            <ContextSection title="加载中">
              <div className="text-sm text-slate-500 dark:text-slate-400">正在读取这次运行实际提交给模型的上下文…</div>
            </ContextSection>
          ) : error ? (
            <ContextSection title="加载失败">
              <div className="text-sm text-rose-600 dark:text-rose-300">{error}</div>
            </ContextSection>
          ) : snapshot ? (
            renderSnapshot(snapshot)
          ) : (
            <ContextSection title="暂无快照">
              <div className="space-y-3 text-sm text-slate-500 dark:text-slate-400">
                <div>这次运行暂时没有找到 runtime snapshot。</div>
                {payload ? <RawBlock value={prettyJson(payload)} /> : null}
              </div>
            </ContextSection>
          )}
        </div>
      </div>
    </div>
  );
};

export default ImRuntimeContextDialog;
