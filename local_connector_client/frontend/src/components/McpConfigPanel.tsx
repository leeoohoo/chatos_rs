// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  CheckCircle2,
  CircleOff,
  CloudUpload,
  FlaskConical,
  Plus,
  Power,
  RefreshCw,
  Settings2,
  Trash2,
  X,
} from 'lucide-react';

import {
  api,
  type LocalMcpConfig,
  type LocalMcpConfigDraft,
  type LocalMcpTransport,
} from '../api';

interface KeyValueRow {
  id: string;
  key: string;
  value: string;
}

interface McpEditorState {
  manifestId?: string;
  displayName: string;
  description: string;
  transport: LocalMcpTransport;
  enabled: boolean;
  command: string;
  args: string[];
  env: KeyValueRow[];
  url: string;
  headers: KeyValueRow[];
  timeoutMs: number;
}

export function McpConfigPanel() {
  const [items, setItems] = React.useState<LocalMcpConfig[]>([]);
  const [editor, setEditor] = React.useState<McpEditorState | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [busyId, setBusyId] = React.useState<string | null>(null);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setError(null);
    try {
      setItems(await api.mcpConfigs());
    } catch (err) {
      setError(errorMessage(err, '读取 MCP 配置失败'));
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  const openCreate = () => {
    setMessage(null);
    setError(null);
    setEditor(emptyEditor());
  };

  const openEdit = (item: LocalMcpConfig) => {
    setMessage(null);
    setError(null);
    setEditor(editorFromConfig(item));
  };

  const save = async () => {
    if (!editor) return;
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const payload = editorPayload(editor);
      const saved = editor.manifestId
        ? await api.updateMcpConfig(editor.manifestId, payload)
        : await api.saveMcpConfig(payload);
      setMessage(
        saved.last_check_status === 'available'
          ? `MCP 已保存，发现 ${saved.tool_count} 个工具。`
          : 'MCP 已保存，但本机连通性检查未通过。',
      );
      setEditor(null);
      await load();
    } catch (err) {
      setError(errorMessage(err, '保存 MCP 配置失败'));
    } finally {
      setSaving(false);
    }
  };

  const runItemAction = async (
    item: LocalMcpConfig,
    action: () => Promise<LocalMcpConfig>,
    success: (next: LocalMcpConfig) => string,
  ) => {
    setBusyId(item.manifest_id);
    setMessage(null);
    setError(null);
    try {
      const next = await action();
      setMessage(success(next));
      await load();
    } catch (err) {
      setError(errorMessage(err, '操作 MCP 配置失败'));
    } finally {
      setBusyId(null);
    }
  };

  const remove = async (item: LocalMcpConfig) => {
    if (!window.confirm(`确定删除 MCP“${item.display_name}”吗？本机配置和云端描述都会被移除。`)) {
      return;
    }
    setBusyId(item.manifest_id);
    setMessage(null);
    setError(null);
    try {
      await api.deleteMcpConfig(item.manifest_id);
      setMessage(`已删除 MCP：${item.display_name}`);
      await load();
    } catch (err) {
      setError(errorMessage(err, '删除 MCP 配置失败'));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <section className="mcpPage">
      <section className="panel mcpPanel">
        <div className="panelHeader mcpToolbar">
          <div>
            <h2>本机 MCP</h2>
            <p>完整运行配置只保存在本机；云端仅同步当前用户可见的私有描述和工具清单。</p>
          </div>
          <div className="mcpToolbarActions">
            <button className="iconButton" onClick={() => void load()} title="刷新 MCP 列表">
              <RefreshCw size={17} />
            </button>
            <button className="primaryButton compact" onClick={openCreate}>
              <Plus size={16} />
              添加 MCP
            </button>
          </div>
        </div>

        {message ? <div className="banner">{message}</div> : null}
        {error ? <div className="formError">{error}</div> : null}

        <div className="mcpList" aria-busy={loading}>
          {items.map((item) => {
            const busy = busyId === item.manifest_id;
            return (
              <article className="mcpRow" key={item.manifest_id}>
                <div className="mcpRowMain">
                  <div className="mcpTitleLine">
                    <strong>{item.display_name}</strong>
                    <span className={item.enabled ? 'status ok' : 'status warn'}>
                      {item.enabled ? '已启用' : '已停用'}
                    </span>
                    <StatusBadge status={item.last_check_status} />
                  </div>
                  <p>{item.description || '未填写说明'}</p>
                  <div className="mcpMetaLine">
                    <span>{item.transport === 'stdio' ? 'stdio 进程' : '本机 HTTP'}</span>
                    <span>{item.tool_count} 个工具</span>
                    <span>{syncLabel(item.sync_status)}</span>
                  </div>
                  {item.last_error ? <div className="mcpInlineError">{item.last_error}</div> : null}
                </div>
                <div className="mcpActions">
                  <button
                    className="iconButton"
                    title={item.enabled ? '停用' : '启用'}
                    disabled={busy}
                    onClick={() => void runItemAction(
                      item,
                      () => api.setMcpConfigEnabled(item.manifest_id, !item.enabled),
                      (next) => next.enabled ? 'MCP 已启用并完成检查。' : 'MCP 已停用。',
                    )}
                  >
                    {item.enabled ? <CircleOff size={16} /> : <Power size={16} />}
                  </button>
                  <button
                    className="iconButton"
                    title="测试连接"
                    disabled={busy}
                    onClick={() => void runItemAction(
                      item,
                      () => api.testMcpConfig(item.manifest_id),
                      (next) => `检查完成，发现 ${next.tool_count} 个工具。`,
                    )}
                  >
                    <FlaskConical size={16} />
                  </button>
                  <button
                    className="iconButton"
                    title="重新同步"
                    disabled={busy}
                    onClick={() => void runItemAction(
                      item,
                      () => api.syncMcpConfig(item.manifest_id),
                      () => 'MCP 描述和工具状态已同步。',
                    )}
                  >
                    <CloudUpload size={16} />
                  </button>
                  <button className="iconButton" title="编辑" disabled={busy} onClick={() => openEdit(item)}>
                    <Settings2 size={16} />
                  </button>
                  <button className="iconButton danger" title="删除" disabled={busy} onClick={() => void remove(item)}>
                    <Trash2 size={16} />
                  </button>
                </div>
              </article>
            );
          })}
          {!items.length ? (
            <div className="mcpEmpty">
              <div className="mcpEmptyIcon"><Power size={20} /></div>
              <strong>{loading ? '正在读取 MCP 配置...' : '还没有本机 MCP'}</strong>
              {!loading ? <span>添加后，启用且在线的工具会自动出现在 Task Runner 的可选工具中。</span> : null}
            </div>
          ) : null}
        </div>
      </section>

      {editor ? (
        <McpEditorModal
          editor={editor}
          saving={saving}
          onChange={setEditor}
          onClose={() => setEditor(null)}
          onSave={() => void save()}
        />
      ) : null}
    </section>
  );
}

function McpEditorModal({
  editor,
  saving,
  onChange,
  onClose,
  onSave,
}: {
  editor: McpEditorState;
  saving: boolean;
  onChange: (next: McpEditorState) => void;
  onClose: () => void;
  onSave: () => void;
}) {
  const patch = (next: Partial<McpEditorState>) => onChange({ ...editor, ...next });
  return (
    <div className="mcpModalBackdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}>
      <section className="mcpModal" role="dialog" aria-modal="true" aria-labelledby="mcp-editor-title">
        <header className="mcpModalHeader">
          <div>
            <span className="pageEyebrow">LOCAL MCP</span>
            <h2 id="mcp-editor-title">{editor.manifestId ? '编辑 MCP' : '添加 MCP'}</h2>
          </div>
          <button className="iconButton" type="button" title="关闭" onClick={onClose}>
            <X size={17} />
          </button>
        </header>

        <div className="mcpModalBody">
          <div className="mcpFormGrid">
            <label className="mcpFieldWide">
              名称
              <input
                value={editor.displayName}
                placeholder="例如：本地知识库"
                onChange={(event) => patch({ displayName: event.target.value })}
              />
            </label>
            <label className="mcpFieldWide">
              说明
              <input
                value={editor.description}
                placeholder="这个 MCP 提供什么能力"
                onChange={(event) => patch({ description: event.target.value })}
              />
            </label>
          </div>

          <div className="mcpFormSection">
            <div className="mcpSectionHeader">
              <div>
                <strong>运行方式</strong>
                <span>HTTP 仅允许访问本机回环地址。</span>
              </div>
              <div className="mcpSegmented" role="group" aria-label="MCP 运行方式">
                <button
                  type="button"
                  className={editor.transport === 'stdio' ? 'active' : ''}
                  onClick={() => patch({ transport: 'stdio' })}
                >
                  stdio
                </button>
                <button
                  type="button"
                  className={editor.transport === 'http' ? 'active' : ''}
                  onClick={() => patch({ transport: 'http' })}
                >
                  HTTP
                </button>
              </div>
            </div>

            {editor.transport === 'stdio' ? (
              <div className="mcpFormGrid">
                <label className="mcpFieldWide">
                  启动命令
                  <input
                    value={editor.command}
                    placeholder="例如：npx"
                    onChange={(event) => patch({ command: event.target.value })}
                  />
                </label>
                <div className="mcpFieldWide">
                  <ArgumentEditor values={editor.args} onChange={(args) => patch({ args })} />
                </div>
                <div className="mcpFieldWide">
                  <KeyValueEditor title="环境变量" values={editor.env} onChange={(env) => patch({ env })} secret />
                </div>
              </div>
            ) : (
              <div className="mcpFormGrid">
                <label className="mcpFieldWide">
                  本机 MCP 地址
                  <input
                    value={editor.url}
                    placeholder="http://127.0.0.1:3000/mcp"
                    onChange={(event) => patch({ url: event.target.value })}
                  />
                </label>
                <label>
                  超时时间（毫秒）
                  <input
                    type="number"
                    min={300}
                    max={120000}
                    value={editor.timeoutMs}
                    onChange={(event) => patch({ timeoutMs: Number(event.target.value) || 15000 })}
                  />
                </label>
                <div className="mcpFieldWide">
                  <KeyValueEditor title="请求头" values={editor.headers} onChange={(headers) => patch({ headers })} secret />
                </div>
              </div>
            )}
          </div>

          <label className="mcpEnableRow">
            <span>
              <strong>保存后启用</strong>
              <small>启用时会立即执行一次 `tools/list` 检查并同步可选工具。</small>
            </span>
            <input type="checkbox" checked={editor.enabled} onChange={(event) => patch({ enabled: event.target.checked })} />
          </label>
        </div>

        <footer className="mcpModalFooter">
          <button className="ghostButton" type="button" onClick={onClose}>取消</button>
          <button className="primaryButton" type="button" disabled={saving} onClick={onSave}>
            <CheckCircle2 size={16} />
            {saving ? '保存中...' : editor.enabled ? '测试并保存' : '保存配置'}
          </button>
        </footer>
      </section>
    </div>
  );
}

function ArgumentEditor({ values, onChange }: { values: string[]; onChange: (values: string[]) => void }) {
  return (
    <div className="mcpRowEditor">
      <div className="mcpRowEditorHeader">
        <span>启动参数</span>
        <button type="button" className="ghostButton compact" onClick={() => onChange([...values, ''])}>
          <Plus size={14} />添加参数
        </button>
      </div>
      {values.map((value, index) => (
        <div className="mcpSingleValueRow" key={`arg-${index}`}>
          <input
            value={value}
            placeholder={`参数 ${index + 1}`}
            onChange={(event) => onChange(values.map((entry, entryIndex) => entryIndex === index ? event.target.value : entry))}
          />
          <button type="button" className="iconButton danger" title="删除参数" onClick={() => onChange(values.filter((_, entryIndex) => entryIndex !== index))}>
            <Trash2 size={15} />
          </button>
        </div>
      ))}
      {!values.length ? <span className="mcpEditorEmpty">没有启动参数</span> : null}
    </div>
  );
}

function KeyValueEditor({
  title,
  values,
  onChange,
  secret,
}: {
  title: string;
  values: KeyValueRow[];
  onChange: (values: KeyValueRow[]) => void;
  secret?: boolean;
}) {
  return (
    <div className="mcpRowEditor">
      <div className="mcpRowEditorHeader">
        <span>{title}</span>
        <button type="button" className="ghostButton compact" onClick={() => onChange([...values, newKeyValueRow()])}>
          <Plus size={14} />添加一项
        </button>
      </div>
      {values.map((row) => (
        <div className="mcpKeyValueRow" key={row.id}>
          <input
            value={row.key}
            placeholder="名称"
            onChange={(event) => onChange(values.map((entry) => entry.id === row.id ? { ...entry, key: event.target.value } : entry))}
          />
          <input
            value={row.value}
            type={secret && row.value !== '********' ? 'password' : 'text'}
            placeholder={secret ? '值仅保存在本机' : '值'}
            onChange={(event) => onChange(values.map((entry) => entry.id === row.id ? { ...entry, value: event.target.value } : entry))}
          />
          <button type="button" className="iconButton danger" title="删除" onClick={() => onChange(values.filter((entry) => entry.id !== row.id))}>
            <Trash2 size={15} />
          </button>
        </div>
      ))}
      {!values.length ? <span className="mcpEditorEmpty">没有配置{title}</span> : null}
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const label = status === 'available'
    ? '可用'
    : status === 'invalid'
      ? '检查失败'
      : status === 'unavailable'
        ? '不可用'
        : '待检查';
  const className = status === 'available' ? 'status ok' : status === 'unknown' ? 'status warn' : 'status bad';
  return <span className={className}>{label}</span>;
}

function syncLabel(status: string): string {
  if (status === 'synced') return '已同步';
  if (status === 'syncing') return '同步中';
  if (status === 'sync_error') return '同步失败';
  return '待同步';
}

function emptyEditor(): McpEditorState {
  return {
    displayName: '',
    description: '',
    transport: 'stdio',
    enabled: true,
    command: '',
    args: [],
    env: [],
    url: '',
    headers: [],
    timeoutMs: 15000,
  };
}

function editorFromConfig(item: LocalMcpConfig): McpEditorState {
  return {
    manifestId: item.manifest_id,
    displayName: item.display_name,
    description: item.description || '',
    transport: item.transport,
    enabled: item.enabled,
    command: item.command || '',
    args: [...item.args],
    env: mapToRows(item.env),
    url: item.url || '',
    headers: mapToRows(item.headers),
    timeoutMs: item.timeout_ms || 15000,
  };
}

function editorPayload(editor: McpEditorState): LocalMcpConfigDraft {
  if (!editor.displayName.trim()) throw new Error('请输入 MCP 名称。');
  if (editor.transport === 'stdio' && !editor.command.trim()) throw new Error('请输入 stdio 启动命令。');
  if (editor.transport === 'http' && !editor.url.trim()) throw new Error('请输入本机 HTTP MCP 地址。');
  return {
    manifest_id: editor.manifestId,
    display_name: editor.displayName.trim(),
    description: editor.description.trim() || null,
    transport: editor.transport,
    enabled: editor.enabled,
    command: editor.transport === 'stdio' ? editor.command.trim() : null,
    args: editor.transport === 'stdio' ? editor.args.map((value) => value.trim()).filter(Boolean) : [],
    env: editor.transport === 'stdio' ? rowsToMap(editor.env) : {},
    url: editor.transport === 'http' ? editor.url.trim() : null,
    headers: editor.transport === 'http' ? rowsToMap(editor.headers) : {},
    timeout_ms: editor.transport === 'http' ? editor.timeoutMs : null,
  };
}

function mapToRows(values: Record<string, string>): KeyValueRow[] {
  return Object.entries(values).map(([key, value]) => ({ id: rowId(), key, value }));
}

function rowsToMap(rows: KeyValueRow[]): Record<string, string> {
  return Object.fromEntries(
    rows
      .map((row) => [row.key.trim(), row.value] as const)
      .filter(([key]) => Boolean(key)),
  );
}

function newKeyValueRow(): KeyValueRow {
  return { id: rowId(), key: '', value: '' };
}

function rowId(): string {
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
