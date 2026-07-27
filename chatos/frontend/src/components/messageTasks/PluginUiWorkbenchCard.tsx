// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FC,
} from 'react';
import { Download, ExternalLink, Eye, Loader2, RotateCcw, X } from 'lucide-react';

import {
  createPluginUiWorkbenchArtifact,
  createPluginUiWorkbenchSession,
  listPluginUiWorkbenchArtifacts,
  readPluginUiWorkbenchArtifact,
  revokePluginUiWorkbenchSession,
  updatePluginUiWorkbenchArtifact,
  type MessageTaskRunnerLookupOptions,
} from '../../lib/api/client/messages';
import type {
  MessageTaskRunnerRunEvent,
  PluginArtifactCreateRequest,
  PluginArtifactDescriptor,
  PluginArtifactUpdateRequest,
  PluginUiWorkbenchSessionResponse,
} from '../../lib/api/client/types';
import { apiClient } from '../../lib/api/client';
import {
  isPluginUiBridgeMessageSource,
  parsePluginUiBridgeIncoming,
  pluginUiBridgeResponse,
  pluginUiReadySummariesFromEvents,
  validatePluginArtifactListResponse,
  validatePluginArtifactReadResponse,
  validatePluginArtifactWriteResponse,
  validatePluginUiWorkbenchSession,
  type PluginUiReadySummary,
} from './pluginUiWorkbench';

interface PluginUiWorkbenchCardProps {
  events: MessageTaskRunnerRunEvent[];
  messageId: string;
  lookup: MessageTaskRunnerLookupOptions;
}

interface PluginUiWorkbenchPanelProps {
  ready: PluginUiReadySummary;
  messageId: string;
  lookup: MessageTaskRunnerLookupOptions;
}

type PanelStatus = 'closed' | 'issuing' | 'loading' | 'ready' | 'error';

const readableError = (error: unknown): string => (
  error instanceof Error && error.message.trim()
    ? error.message.trim()
    : 'Plugin UI Workbench 暂时不可用'
);

export const pluginArtifactBridgeDescriptor = (artifact: PluginArtifactDescriptor) => ({
  artifact_id: artifact.artifact_id,
  display_name: artifact.display_name,
  media_type: artifact.media_type,
  size_bytes: artifact.size_bytes,
  sha256: artifact.sha256,
  created_at: artifact.created_at,
  downloadable: artifact.downloadable,
  mutable: artifact.mutable,
});

const PluginUiWorkbenchPanel: FC<PluginUiWorkbenchPanelProps> = ({
  ready,
  messageId,
  lookup,
}) => {
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const seenRequestIdsRef = useRef<Set<string>>(new Set());
  const bridgeReadyRef = useRef(false);
  const pendingRevokeRef = useRef<{ sessionId: string; timer: number } | null>(null);
  const [session, setSession] = useState<PluginUiWorkbenchSessionResponse | null>(null);
  const [status, setStatus] = useState<PanelStatus>('closed');
  const [error, setError] = useState<string | null>(null);
  const [artifacts, setArtifacts] = useState<PluginArtifactDescriptor[]>([]);
  const [artifactError, setArtifactError] = useState<string | null>(null);
  const [artifactPreview, setArtifactPreview] = useState<{
    name: string;
    text: string;
  } | null>(null);

  const loadArtifacts = useCallback(async (
    current: PluginUiWorkbenchSessionResponse,
  ): Promise<PluginArtifactDescriptor[]> => {
    const response = await listPluginUiWorkbenchArtifacts(
      apiClient.getRequestFn(),
      messageId,
      ready.runId,
      ready.eventId,
      current.session_id,
    );
    const validated = validatePluginArtifactListResponse(response, ready);
    if (!validated) {
      throw new Error('Plugin Artifact list 响应未通过安全校验');
    }
    setArtifacts(validated.artifacts);
    setArtifactError(null);
    return validated.artifacts;
  }, [messageId, ready]);

  const triggerArtifactDownload = useCallback((
    current: PluginUiWorkbenchSessionResponse,
    artifactId: string,
  ) => {
    const anchor = document.createElement('a');
    anchor.href = `/api/plugin-artifacts/workbench/${encodeURIComponent(current.session_id)}/${encodeURIComponent(artifactId)}/download`;
    anchor.rel = 'noopener';
    anchor.style.display = 'none';
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
  }, []);

  const readArtifact = useCallback(async (
    current: PluginUiWorkbenchSessionResponse,
    artifactId: string,
  ) => {
    const response = await readPluginUiWorkbenchArtifact(
      apiClient.getRequestFn(),
      messageId,
      ready.runId,
      ready.eventId,
      current.session_id,
      artifactId,
    );
    const validated = validatePluginArtifactReadResponse(response, ready, artifactId);
    if (!validated) {
      throw new Error('Plugin Artifact read 响应未通过安全校验');
    }
    return validated;
  }, [messageId, ready]);

  const createArtifact = useCallback(async (
    current: PluginUiWorkbenchSessionResponse,
    payload: PluginArtifactCreateRequest,
  ) => {
    if (!ready.artifactMimeTypes.includes(payload.media_type)) {
      throw new Error('Plugin UI 无权创建该 MIME type 的 Artifact');
    }
    const response = await createPluginUiWorkbenchArtifact(
      apiClient.getRequestFn(),
      messageId,
      ready.runId,
      ready.eventId,
      current.session_id,
      payload,
    );
    const validated = validatePluginArtifactWriteResponse(
      response,
      ready,
      'create',
      null,
      payload,
      payload.body_base64,
    );
    if (!validated) {
      throw new Error('Plugin Artifact create 响应未通过安全校验');
    }
    await loadArtifacts(current);
    return validated;
  }, [loadArtifacts, messageId, ready]);

  const updateArtifact = useCallback(async (
    current: PluginUiWorkbenchSessionResponse,
    artifactId: string,
    payload: PluginArtifactUpdateRequest,
  ) => {
    const response = await updatePluginUiWorkbenchArtifact(
      apiClient.getRequestFn(),
      messageId,
      ready.runId,
      ready.eventId,
      current.session_id,
      artifactId,
      payload,
    );
    const validated = validatePluginArtifactWriteResponse(
      response,
      ready,
      'update',
      artifactId,
      null,
      payload.body_base64,
    );
    if (!validated) {
      throw new Error('Plugin Artifact update 响应未通过安全校验');
    }
    await loadArtifacts(current);
    return validated;
  }, [loadArtifacts, messageId, ready]);

  const revoke = useCallback((current: PluginUiWorkbenchSessionResponse) => {
    void revokePluginUiWorkbenchSession(
      apiClient.getRequestFn(),
      messageId,
      ready.runId,
      ready.eventId,
      current.session_id,
    ).catch(() => undefined);
  }, [messageId, ready.eventId, ready.runId]);

  useEffect(() => {
    const pending = pendingRevokeRef.current;
    if (pending && pending.sessionId === session?.session_id) {
      window.clearTimeout(pending.timer);
      pendingRevokeRef.current = null;
    }
    return () => {
      if (!session) {
        return;
      }
      const timer = window.setTimeout(() => {
        revoke(session);
        if (pendingRevokeRef.current?.timer === timer) {
          pendingRevokeRef.current = null;
        }
      }, 0);
      pendingRevokeRef.current = { sessionId: session.session_id, timer };
    };
  }, [revoke, session]);

  useEffect(() => {
    if (!session || status !== 'loading') {
      return undefined;
    }
    const readyTimeout = window.setTimeout(() => {
      setStatus('error');
      setError('Plugin UI 未在 10 秒内完成安全握手');
    }, 10_000);
    return () => window.clearTimeout(readyTimeout);
  }, [session, status]);

  useEffect(() => {
    if (!session) {
      return undefined;
    }
    const expiryTimeout = window.setTimeout(() => {
      setError('Plugin UI Workbench 短期会话已过期，请重新打开');
      setStatus('error');
      setSession(null);
    }, Math.max(1, session.expires_in) * 1000);
    return () => window.clearTimeout(expiryTimeout);
  }, [session]);

  useEffect(() => {
    if (!session || !session.bridge_capabilities.includes('artifact.list')) {
      setArtifacts([]);
      setArtifactError(null);
      return;
    }
    void loadArtifacts(session).catch((nextError) => {
      setArtifactError(readableError(nextError));
    });
  }, [loadArtifacts, session]);

  useEffect(() => {
    if (!session) {
      return undefined;
    }
    const handleMessage = (event: MessageEvent<unknown>) => {
      const iframeWindow = iframeRef.current?.contentWindow;
      if (
        !iframeWindow
        || !isPluginUiBridgeMessageSource(event.origin, event.source, iframeWindow)
      ) {
        return;
      }
      const incoming = parsePluginUiBridgeIncoming(event.data);
      if (
        !incoming
        || incoming.adapterSessionId !== session.adapter_session_id
        || incoming.hostSessionNonce !== session.host_session_nonce
      ) {
        return;
      }
      if (incoming.kind === 'ready') {
        bridgeReadyRef.current = true;
        setError(null);
        setStatus('ready');
        return;
      }
      if (seenRequestIdsRef.current.has(incoming.requestId)) {
        iframeWindow.postMessage(
          pluginUiBridgeResponse(session, incoming.requestId, false, null, 'duplicate_request'),
          '*',
        );
        return;
      }
      seenRequestIdsRef.current.add(incoming.requestId);
      if (seenRequestIdsRef.current.size > 256) {
        const oldest = seenRequestIdsRef.current.values().next().value;
        if (typeof oldest === 'string') {
          seenRequestIdsRef.current.delete(oldest);
        }
      }
      if (!bridgeReadyRef.current) {
        iframeWindow.postMessage(
          pluginUiBridgeResponse(session, incoming.requestId, false, null, 'bridge_not_ready'),
          '*',
        );
        return;
      }
      if (!session.bridge_capabilities.includes(incoming.method)) {
        iframeWindow.postMessage(
          pluginUiBridgeResponse(session, incoming.requestId, false, null, 'method_not_allowed'),
          '*',
        );
        return;
      }
      void (async () => {
        try {
          let result: unknown;
          if (incoming.method === 'host.context.read') {
            result = session.host_context;
          } else if (incoming.method === 'artifact.list') {
            result = {
              artifacts: (await loadArtifacts(session)).map(pluginArtifactBridgeDescriptor),
            };
          } else if (incoming.method === 'artifact.read') {
            const artifact = await readArtifact(
              session,
              incoming.payload.artifact_id as string,
            );
            result = {
              artifact: pluginArtifactBridgeDescriptor(artifact.artifact),
              body_base64: artifact.body_base64,
            };
          } else if (incoming.method === 'artifact.download') {
            const artifactId = incoming.payload.artifact_id as string;
            const available = await loadArtifacts(session);
            if (!available.some((artifact) => artifact.artifact_id === artifactId)) {
              throw new Error('Plugin Artifact 不存在');
            }
            triggerArtifactDownload(session, artifactId);
            result = { started: true, artifact_id: artifactId };
          } else if (incoming.method === 'artifact.create') {
            const payload = incoming.payload as unknown as PluginArtifactCreateRequest;
            const response = await createArtifact(session, payload);
            result = {
              operation: response.operation,
              artifact: pluginArtifactBridgeDescriptor(response.artifact),
            };
          } else if (incoming.method === 'artifact.update') {
            const payload = incoming.payload as unknown as PluginArtifactUpdateRequest & {
              artifact_id: string;
            };
            const response = await updateArtifact(session, payload.artifact_id, {
              expected_sha256: payload.expected_sha256,
              body_base64: payload.body_base64,
            });
            result = {
              operation: response.operation,
              artifact: pluginArtifactBridgeDescriptor(response.artifact),
            };
          } else {
            iframeWindow.postMessage(
              pluginUiBridgeResponse(
                session,
                incoming.requestId,
                false,
                null,
                'method_not_implemented',
              ),
              '*',
            );
            return;
          }
          if (iframeRef.current?.contentWindow !== iframeWindow) {
            return;
          }
          iframeWindow.postMessage(
            pluginUiBridgeResponse(session, incoming.requestId, true, result, null),
            '*',
          );
        } catch {
          if (iframeRef.current?.contentWindow !== iframeWindow) {
            return;
          }
          iframeWindow.postMessage(
            pluginUiBridgeResponse(session, incoming.requestId, false, null, 'host_request_failed'),
            '*',
          );
        }
      })();
    };
    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, [createArtifact, loadArtifacts, readArtifact, session, triggerArtifactDownload, updateArtifact]);

  const open = useCallback(async () => {
    if (status === 'issuing') {
      return;
    }
    seenRequestIdsRef.current.clear();
    bridgeReadyRef.current = false;
    setError(null);
    setArtifacts([]);
    setArtifactError(null);
    setArtifactPreview(null);
    setStatus('issuing');
    try {
      const issued = await createPluginUiWorkbenchSession(
        apiClient.getRequestFn(),
        messageId,
        ready.runId,
        ready.eventId,
        lookup,
      );
      const validated = validatePluginUiWorkbenchSession(issued, ready);
      if (!validated) {
        throw new Error('Plugin UI Workbench session 响应未通过安全校验');
      }
      setSession(validated);
      setStatus('loading');
    } catch (nextError) {
      setSession(null);
      setStatus('error');
      setError(readableError(nextError));
    }
  }, [lookup, messageId, ready, status]);

  const close = useCallback(() => {
    seenRequestIdsRef.current.clear();
    bridgeReadyRef.current = false;
    setSession(null);
    setStatus('closed');
    setError(null);
    setArtifacts([]);
    setArtifactError(null);
    setArtifactPreview(null);
  }, []);

  const previewArtifact = useCallback(async (artifact: PluginArtifactDescriptor) => {
    if (!session) {
      return;
    }
    try {
      const response = await readArtifact(session, artifact.artifact_id);
      const binary = window.atob(response.body_base64);
      const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
      setArtifactPreview({
        name: artifact.display_name,
        text: new TextDecoder('utf-8', { fatal: true }).decode(bytes),
      });
      setArtifactError(null);
    } catch (nextError) {
      setArtifactPreview(null);
      setArtifactError(readableError(nextError));
    }
  }, [readArtifact, session]);

  return (
    <div className="rounded-md border border-violet-500/25 bg-background p-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-foreground">{ready.title}</div>
          <div className="mt-1 break-all font-mono text-[10px] text-muted-foreground">
            {ready.pluginId} / {ready.componentKey} · {ready.surface}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className="rounded border border-border bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
            {status === 'ready' ? 'ready' : status}
          </span>
          {session ? (
            <button
              type="button"
              className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs hover:bg-accent"
              onClick={close}
            >
              <X className="h-3.5 w-3.5" />
              关闭
            </button>
          ) : (
            <button
              type="button"
              className="inline-flex items-center gap-1 rounded-md border border-violet-500/30 bg-violet-500/10 px-2 py-1 text-xs text-violet-700 hover:bg-violet-500/15 disabled:opacity-60 dark:text-violet-300"
              disabled={status === 'issuing'}
              onClick={() => void open()}
            >
              {status === 'issuing' ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : status === 'error' ? (
                <RotateCcw className="h-3.5 w-3.5" />
              ) : (
                <ExternalLink className="h-3.5 w-3.5" />
              )}
              {status === 'error' ? '重试' : '打开'}
            </button>
          )}
        </div>
      </div>
      {error ? (
        <div className="mt-2 rounded bg-red-500/5 px-2 py-1.5 text-xs text-red-700 dark:text-red-300">
          {error}
        </div>
      ) : null}
      {session ? (
        <div className="mt-3 space-y-3">
          <div className="overflow-hidden rounded-md border border-border bg-white dark:bg-slate-950">
            <iframe
              ref={iframeRef}
              title={ready.title}
              src={session.iframe_path}
              sandbox="allow-scripts"
              referrerPolicy="no-referrer"
              allow="camera 'none'; microphone 'none'; geolocation 'none'; display-capture 'none'; clipboard-read 'none'; clipboard-write 'none'"
              className="h-[420px] w-full border-0"
            />
          </div>
          {session.bridge_capabilities.includes('artifact.list') ? (
            <div className="rounded-md border border-border bg-muted/20 p-2.5">
              <div className="flex items-center justify-between gap-2">
                <div className="text-xs font-medium">Plugin Artifacts</div>
                <button
                  type="button"
                  className="text-[11px] text-muted-foreground hover:text-foreground"
                  onClick={() => void loadArtifacts(session).catch((nextError) => {
                    setArtifactError(readableError(nextError));
                  })}
                >
                  刷新
                </button>
              </div>
              {artifactError ? (
                <div className="mt-2 text-xs text-red-700 dark:text-red-300">{artifactError}</div>
              ) : null}
              {artifacts.length ? (
                <div className="mt-2 space-y-1.5">
                  {artifacts.map((artifact) => (
                    <div
                      key={artifact.artifact_id}
                      className="flex items-center justify-between gap-2 rounded border border-border bg-background px-2 py-1.5"
                    >
                      <div className="min-w-0">
                        <div className="truncate text-xs">{artifact.display_name}</div>
                        <div className="truncate font-mono text-[10px] text-muted-foreground">
                          {artifact.media_type} · {artifact.size_bytes.toLocaleString()} bytes
                        </div>
                      </div>
                      <div className="flex shrink-0 items-center gap-1">
                        {session.bridge_capabilities.includes('artifact.read')
                          && ['text/plain', 'text/csv', 'application/json'].includes(artifact.media_type)
                          && artifact.size_bytes <= 160 * 1024 ? (
                            <button
                              type="button"
                              className="rounded border border-border p-1 hover:bg-accent"
                              title="预览"
                              onClick={() => void previewArtifact(artifact)}
                            >
                              <Eye className="h-3.5 w-3.5" />
                            </button>
                          ) : null}
                        {session.bridge_capabilities.includes('artifact.download') ? (
                          <button
                            type="button"
                            className="rounded border border-border p-1 hover:bg-accent"
                            title="下载"
                            onClick={() => triggerArtifactDownload(session, artifact.artifact_id)}
                          >
                            <Download className="h-3.5 w-3.5" />
                          </button>
                        ) : null}
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="mt-2 text-xs text-muted-foreground">当前 Run 暂无可访问 Artifact</div>
              )}
              {artifactPreview ? (
                <div className="mt-2 rounded border border-border bg-background p-2">
                  <div className="mb-1 flex items-center justify-between gap-2 text-[11px] font-medium">
                    <span className="truncate">{artifactPreview.name}</span>
                    <button type="button" onClick={() => setArtifactPreview(null)}>关闭</button>
                  </div>
                  <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words text-[11px] leading-5">
                    {artifactPreview.text}
                  </pre>
                </div>
              ) : null}
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
};

export const PluginUiWorkbenchCard: FC<PluginUiWorkbenchCardProps> = ({
  events,
  messageId,
  lookup,
}) => {
  const readyItems = useMemo(
    () => pluginUiReadySummariesFromEvents(events).slice(-16),
    [events],
  );
  if (!readyItems.length) {
    return null;
  }
  return (
    <div className="mb-4 rounded-lg border border-violet-500/30 bg-violet-500/5 p-3">
      <div className="text-sm font-medium">Plugin UI Workbench</div>
      <div className="mb-3 mt-1 text-[11px] leading-5 text-muted-foreground">
        第三方界面运行在无同源权限的脚本沙箱中。Host 仅接受来源窗口、opaque origin、会话 nonce 和协议版本全部匹配的消息；已声明的 host.context.read 与 Artifact list/read/download 会按 Run、Release、owner、设备和工作区重新校验。
      </div>
      <div className="space-y-2">
        {readyItems.map((ready) => (
          <PluginUiWorkbenchPanel
            key={ready.eventId}
            ready={ready}
            messageId={messageId}
            lookup={lookup}
          />
        ))}
      </div>
    </div>
  );
};
