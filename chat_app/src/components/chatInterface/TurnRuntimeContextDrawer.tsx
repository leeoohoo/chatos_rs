import React from 'react';
import type {
  TurnRuntimeSnapshotContextItem,
  TurnRuntimeSnapshotLookupResponse,
  TurnRuntimeSnapshotRuntime,
  TurnRuntimeSnapshotSystemMessage,
  TurnRuntimeSnapshotTool,
} from '../../lib/api/client/types';

interface TurnRuntimeContextDrawerProps {
  open: boolean;
  sessionId: string | null;
  loading: boolean;
  error: string | null;
  data: TurnRuntimeSnapshotLookupResponse | null;
  onRefresh: () => void;
  onClose: () => void;
}

const renderValue = (value?: string | null): string => {
  const normalized = value?.trim();
  return normalized ? normalized : '-';
};

const getPreviewItemTone = (role?: string | null, type?: string | null): string => {
  if (type === 'tool') {
    return 'border-amber-500/40 bg-amber-500/10';
  }
  if (role === 'system') {
    return 'border-sky-500/40 bg-sky-500/10';
  }
  if (role === 'assistant') {
    return 'border-border bg-background/80';
  }
  return 'border-border bg-background/70';
};

const getSystemMessageTone = (messageId?: string | null): string => {
  const normalized = typeof messageId === 'string' ? messageId.trim() : '';
  if (normalized === 'task_board') {
    return 'border-emerald-500/40 bg-emerald-500/10';
  }
  if (normalized === 'builtin_mcp') {
    return 'border-violet-500/30 bg-violet-500/10';
  }
  return 'border-sky-500/30 bg-sky-500/10';
};

const buildItemSummary = (item: TurnRuntimeSnapshotContextItem): string => {
  const parts = [
    item.role ? `role=${item.role}` : '',
    item.type ? `type=${item.type}` : '',
    item.source ? `source=${item.source}` : '',
  ].filter(Boolean);
  return parts.join(' · ') || '上下文项';
};

const buildSystemMessageSummary = (item: TurnRuntimeSnapshotSystemMessage): string => {
  const parts = [
    item.id ? `id=${item.id}` : '',
    item.source ? `source=${item.source}` : '',
  ].filter(Boolean);
  return parts.join(' · ') || '系统消息';
};

const buildToolSummary = (tool: TurnRuntimeSnapshotTool): string => {
  const parts = [
    tool.name ? `name=${tool.name}` : '',
    tool.server_name ? `server=${tool.server_name}` : '',
    tool.server_type ? `type=${tool.server_type}` : '',
  ].filter(Boolean);
  return parts.join(' · ') || '工具';
};

const buildRequestMetaPayload = (
  runtime: TurnRuntimeSnapshotRuntime | null,
  tools: TurnRuntimeSnapshotTool[],
): Record<string, unknown> => {
  return {
    model: runtime?.model || null,
    provider: runtime?.provider || null,
    mcp_enabled: runtime?.mcp_enabled ?? null,
    enabled_mcp_ids: Array.isArray(runtime?.enabled_mcp_ids) ? runtime?.enabled_mcp_ids : [],
    tools: tools.map((tool) => ({
      name: tool.name,
      server_name: tool.server_name,
      server_type: tool.server_type,
      description: tool.description ?? null,
    })),
    selected_commands: Array.isArray(runtime?.selected_commands) ? runtime?.selected_commands : [],
    unavailable_builtin_tools: Array.isArray(runtime?.unavailable_builtin_tools)
      ? runtime?.unavailable_builtin_tools
      : [],
    builtin_mcp_prompt: runtime?.builtin_mcp_prompt || null,
  };
};

const TurnRuntimeContextDrawer: React.FC<TurnRuntimeContextDrawerProps> = ({
  open,
  sessionId,
  loading,
  error,
  data,
  onRefresh,
  onClose,
}) => {
  if (!open) {
    return null;
  }

  const snapshot = data?.snapshot || null;
  const runtime = snapshot?.runtime || null;
  const systemMessages = Array.isArray(snapshot?.system_messages)
    ? snapshot.system_messages
    : [];
  const tools = Array.isArray(snapshot?.tools) ? snapshot.tools : [];
  const actualPreviewItems = Array.isArray(runtime?.actual_context_items)
    ? runtime.actual_context_items
    : [];
  const lastModelRequestPayload = runtime?.last_model_request_payload || null;
  const requestMetaPayload = buildRequestMetaPayload(runtime, tools);
  const status = data?.status || 'unknown';
  const snapshotSource = data?.snapshot_source || 'missing';

  return (
    <div className="fixed inset-0 z-50">
      <button
        type="button"
        aria-label="关闭上下文快照抽屉"
        className="absolute inset-0 bg-black/35"
        onClick={onClose}
      />
      <div className="absolute right-0 top-0 h-full w-full max-w-2xl border-l border-border bg-card shadow-xl">
        <div className="flex h-full flex-col">
          <div className="flex items-center justify-between border-b border-border px-4 py-3">
            <div>
              <div className="text-sm font-semibold text-foreground">轮次运行上下文</div>
              <div className="text-xs text-muted-foreground">
                {sessionId ? `会话 ${sessionId}` : '未选择会话'}
              </div>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60"
                disabled={!sessionId || loading}
                onClick={onRefresh}
              >
                {loading ? '刷新中...' : '刷新'}
              </button>
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent"
                onClick={onClose}
              >
                关闭
              </button>
            </div>
          </div>

          <div className="custom-scrollbar flex-1 overflow-y-scroll px-4 py-3 [scrollbar-gutter:stable]">
            {error ? (
              <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs text-destructive">
                {error}
              </div>
            ) : null}

            <div className="mb-3 rounded-md border border-border bg-background/70 p-3 text-xs text-muted-foreground">
              <div>{`turn_id: ${data?.turn_id || '-'}`}</div>
              <div>{`status: ${status}`}</div>
              <div>{`snapshot_source: ${snapshotSource}`}</div>
              <div>{`captured_at: ${snapshot?.captured_at || '-'}`}</div>
              <div>{`mode: ${renderValue(runtime?.actual_context_mode)}`}</div>
              <div>{`system_message_count: ${systemMessages.length}`}</div>
              <div>{`actual_item_count: ${actualPreviewItems.length}`}</div>
              <div>{`tool_count: ${tools.length}`}</div>
            </div>

            <div className="mb-3 rounded-md border border-sky-500/30 bg-sky-500/10 p-3 text-xs text-sky-950 dark:text-sky-100">
              本面板展示的是当前轮快照里的关键信息；“完整请求（payload）”始终指向最近一次真正发送给模型的那次请求。
            </div>

            <div className="mb-2 text-xs font-medium text-foreground">系统消息快照</div>
            {systemMessages.length === 0 ? (
              <div className="mb-3 rounded-md border border-border bg-background/70 px-3 py-2 text-xs text-muted-foreground">
                当前快照里没有系统消息
              </div>
            ) : (
              <div className="mb-4 space-y-2">
                {systemMessages.map((item, index) => (
                  <details
                    key={`system-message:${item.id || '-'}:${index}`}
                    className={`rounded-md border p-3 ${getSystemMessageTone(item.id)}`}
                  >
                    <summary className="cursor-pointer list-none text-xs text-foreground">
                      <span className="font-medium">{`${index + 1}. ${buildSystemMessageSummary(item)}`}</span>
                      <span className="ml-2 text-muted-foreground">默认折叠，点击展开内容</span>
                    </summary>
                    <pre className="mt-2 whitespace-pre-wrap break-words text-xs text-foreground">
{item.content}
                    </pre>
                  </details>
                ))}
              </div>
            )}

            <div className="mb-2 text-xs font-medium text-foreground">最近一次实际请求内容</div>
            {actualPreviewItems.length === 0 ? (
              <div className="rounded-md border border-border bg-background/70 px-3 py-2 text-xs text-muted-foreground">
                当前快照里还没有记录实际发送上下文
              </div>
            ) : (
              <div className="space-y-2">
                {actualPreviewItems.map((item, index) => (
                  <details
                    key={`actual-preview:${index}:${item.role || '-'}:${item.type || '-'}:${item.source || '-'}`}
                    className={`rounded-md border p-3 ${getPreviewItemTone(item.role, item.type)}`}
                  >
                    <summary className="cursor-pointer list-none text-xs text-foreground">
                      <span className="font-medium">{`${index + 1}. ${buildItemSummary(item)}`}</span>
                      <span className="ml-2 text-muted-foreground">默认折叠，点击展开内容</span>
                    </summary>
                    <pre className="mt-2 whitespace-pre-wrap break-words text-xs text-foreground">
{item.content}
                    </pre>
                  </details>
                ))}
              </div>
            )}

            <div className="mb-2 mt-4 text-xs font-medium text-foreground">本轮可用工具列表（tools）</div>
            {tools.length === 0 ? (
              <div className="rounded-md border border-border bg-background/70 px-3 py-2 text-xs text-muted-foreground">
                当前快照里没有记录工具
              </div>
            ) : (
              <div className="space-y-2">
                {tools.map((tool, index) => (
                  <details
                    key={`tool:${tool.name}:${tool.server_name}:${index}`}
                    className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3"
                  >
                    <summary className="cursor-pointer list-none text-xs text-foreground">
                      <span className="font-medium">{`${index + 1}. ${buildToolSummary(tool)}`}</span>
                      <span className="ml-2 text-muted-foreground">默认折叠，点击展开内容</span>
                    </summary>
                    <pre className="mt-2 whitespace-pre-wrap break-words text-xs text-foreground">
{JSON.stringify(tool, null, 2)}
                    </pre>
                  </details>
                ))}
              </div>
            )}

            <div className="mb-2 mt-4 text-xs font-medium text-foreground">最近一次发送给模型的完整请求（payload）</div>
            {lastModelRequestPayload ? (
              <pre className="rounded-md border border-emerald-500/40 bg-emerald-500/10 p-3 text-xs text-foreground whitespace-pre-wrap break-words">
{JSON.stringify(lastModelRequestPayload, null, 2)}
              </pre>
            ) : (
              <div className="rounded-md border border-border bg-background/70 px-3 py-2 text-xs text-muted-foreground">
                当前快照里还没有记录完整请求 payload
              </div>
            )}

            <div className="mb-2 mt-4 text-xs font-medium text-foreground">请求级元信息（快照结构化）</div>
            <pre className="rounded-md border border-border bg-background/70 p-3 text-xs text-foreground whitespace-pre-wrap break-words">
{JSON.stringify(requestMetaPayload, null, 2)}
            </pre>
          </div>
        </div>
      </div>
    </div>
  );
};

export default TurnRuntimeContextDrawer;
