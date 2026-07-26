// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useCallback, useEffect, useRef, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import {
  sendBrowserSessionCommand,
  type BrowserSessionCommandPayload,
  type BrowserSessionCommandResponse,
} from '../../lib/api/localRuntime/browserSession';
import {
  subscribeBrowserSessionPanel,
  type BrowserSessionUiTarget,
} from '../../lib/browserSessionUi';
import BrowserPdfPreview from './BrowserPdfPreview';

const text = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');
const number = (value: unknown): number => (typeof value === 'number' && Number.isFinite(value) ? value : 0);
const record = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);
const records = (value: unknown): Record<string, unknown>[] => (
  Array.isArray(value)
    ? value.map(record).filter((item): item is Record<string, unknown> => item !== null)
    : []
);

type BrowserDetailTab = 'snapshot' | 'console' | 'network' | 'websocket';

const pageValue = (response: BrowserSessionCommandResponse | null, key: string): string => (
  text(response?.page?.[key])
);

const readableError = (error: unknown): string => (
  error instanceof Error ? error.message : String(error || 'Unknown error')
);

const BrowserSessionPanel: React.FC = () => {
  const { t } = useI18n();
  const [target, setTarget] = useState<BrowserSessionUiTarget | null>(null);
  const [response, setResponse] = useState<BrowserSessionCommandResponse | null>(null);
  const [preview, setPreview] = useState<BrowserSessionCommandResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [address, setAddress] = useState('');
  const [elementRef, setElementRef] = useState('');
  const [inputText, setInputText] = useState('');
  const [uploadPath, setUploadPath] = useState('');
  const [downloadPath, setDownloadPath] = useState('browser-download.bin');
  const [harPath, setHarPath] = useState('browser-network.har');
  const [harRecording, setHarRecording] = useState(false);
  const [detailTab, setDetailTab] = useState<BrowserDetailTab>('snapshot');
  const [consolePage, setConsolePage] = useState<Record<string, unknown> | null>(null);
  const [networkPage, setNetworkPage] = useState<Record<string, unknown> | null>(null);
  const [networkDetailPage, setNetworkDetailPage] = useState<Record<string, unknown> | null>(null);
  const [harPage, setHarPage] = useState<Record<string, unknown> | null>(null);
  const [websocketPage, setWebsocketPage] = useState<Record<string, unknown> | null>(null);
  const [websocketActive, setWebsocketActive] = useState(false);
  const [websocketDirection, setWebsocketDirection] = useState<'' | 'sent' | 'received'>('');
  const requestActive = useRef(false);
  const manualCommandPending = useRef(false);
  const previewRequestActive = useRef(false);
  const previewFrameSequence = useRef(0);
  const activeBrowserTabId = useRef('');
  const targetRef = useRef<BrowserSessionUiTarget | null>(null);

  useEffect(() => subscribeBrowserSessionPanel((nextTarget) => {
    targetRef.current = nextTarget;
    setTarget(nextTarget);
    setResponse(null);
    setPreview(null);
    setError(null);
    setAddress(nextTarget.url || '');
    setDetailTab('snapshot');
    setConsolePage(null);
    setNetworkPage(null);
    setNetworkDetailPage(null);
    setHarPage(null);
    setHarRecording(false);
    setWebsocketPage(null);
    setWebsocketActive(false);
    setWebsocketDirection('');
    previewFrameSequence.current = 0;
    activeBrowserTabId.current = '';
  }), []);

  const refreshPreview = useCallback(async () => {
    const currentTarget = targetRef.current;
    if (!currentTarget || previewRequestActive.current) {
      return;
    }
    previewRequestActive.current = true;
    try {
      const next = await sendBrowserSessionCommand(currentTarget.id, {
        workspace_id: currentTarget.workspaceId,
        action: 'stream_frame',
        after_frame_sequence: previewFrameSequence.current || undefined,
      });
      if (targetRef.current?.id === currentTarget.id && next.frame_data_url) {
        const nextSequence = number(next.frame?.sequence);
        if (nextSequence > previewFrameSequence.current) {
          previewFrameSequence.current = nextSequence;
        }
        setPreview(next);
      }
    } catch {
      // The slower snapshot path remains visible when a live frame is unavailable.
    } finally {
      previewRequestActive.current = false;
    }
  }, []);

  const runCommand = useCallback(async (
    action: BrowserSessionCommandPayload['action'],
    extra: Partial<BrowserSessionCommandPayload> = {},
    quiet = false,
  ) => {
    const currentTarget = targetRef.current;
    if (!currentTarget) {
      return;
    }
    if (quiet) {
      if (requestActive.current || manualCommandPending.current) {
        return;
      }
    } else {
      if (manualCommandPending.current) {
        return;
      }
      manualCommandPending.current = true;
      setLoading(true);
      setError(null);
      while (requestActive.current && targetRef.current?.id === currentTarget.id) {
        await new Promise<void>((resolve) => window.setTimeout(resolve, 25));
      }
      if (targetRef.current?.id !== currentTarget.id) {
        manualCommandPending.current = false;
        setLoading(false);
        return;
      }
    }
    requestActive.current = true;
    try {
      const next = await sendBrowserSessionCommand(currentTarget.id, {
        workspace_id: currentTarget.workspaceId,
        action,
        ...extra,
      });
      if (targetRef.current?.id !== currentTarget.id) {
        return;
      }
      if (action === 'console') {
        setConsolePage(next.page || null);
      } else if (action === 'network') {
        setNetworkPage(next.page || null);
      } else if (action === 'network_request') {
        setNetworkDetailPage(next.page || null);
      } else if (action === 'har_start' || action === 'har_stop') {
        setHarPage(next.page || null);
        if (next.success !== false) {
          setHarRecording(action === 'har_start');
        }
      } else if (action === 'websocket_start' || action === 'websocket_frames' || action === 'websocket_stop') {
        setWebsocketPage(next.page || null);
        if (next.success !== false) {
          if (action === 'websocket_start') {
            setWebsocketActive(true);
          } else if (action === 'websocket_stop') {
            setWebsocketActive(false);
          } else if (typeof next.page?.active === 'boolean') {
            setWebsocketActive(next.page.active);
          }
        }
      } else {
        setResponse(next);
        if (next.screenshot_data_url) {
          setPreview({
            status: next.status,
            action: next.action,
            frame_data_url: next.screenshot_data_url,
            frame: { source: 'action_screenshot' },
            captured_at: next.captured_at,
          });
        }
      }
      const nextPage = record(next.page);
      const nextActiveTab = records(nextPage?.tabs).find((tab) => tab.active === true);
      const nextActiveTabId = text(nextActiveTab?.tab_id ?? nextActiveTab?.tabId);
      if (
        nextActiveTabId
        && activeBrowserTabId.current
        && activeBrowserTabId.current !== nextActiveTabId
      ) {
        previewFrameSequence.current = 0;
        try {
          await sendBrowserSessionCommand(currentTarget.id, {
            workspace_id: currentTarget.workspaceId,
            action: 'stream_stop',
          });
        } catch {
          // The next frame request can still establish a fresh bounded stream.
        }
      }
      if (nextActiveTabId) {
        activeBrowserTabId.current = nextActiveTabId;
      }
      const nextUrl = pageValue(next, 'url') || text(nextActiveTab?.url);
      if (nextUrl) {
        setAddress(nextUrl);
      } else if (action === 'tabs' || action === 'tab_new' || action === 'tab_switch' || action === 'tab_close') {
        setAddress('');
      }
      if (next.status === 'closed') {
        targetRef.current = null;
        setTarget(null);
      }
    } catch (commandError) {
      if (targetRef.current?.id === currentTarget.id) {
        setError(readableError(commandError));
      }
    } finally {
      requestActive.current = false;
      if (!quiet) {
        manualCommandPending.current = false;
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    if (!target) {
      return;
    }
    void runCommand('tabs');
    const timer = window.setInterval(() => {
      void runCommand('tabs', {}, true);
    }, 2500);
    return () => window.clearInterval(timer);
  }, [runCommand, target]);

  useEffect(() => {
    if (!target) {
      return;
    }
    let cancelled = false;
    let timer: number | null = null;
    const pump = async () => {
      await refreshPreview();
      if (!cancelled) {
        timer = window.setTimeout(() => { void pump(); }, 50);
      }
    };
    void pump();
    return () => {
      cancelled = true;
      if (timer !== null) {
        window.clearTimeout(timer);
      }
      previewFrameSequence.current = 0;
      void sendBrowserSessionCommand(target.id, {
        workspace_id: target.workspaceId,
        action: 'stream_stop',
      }).catch(() => undefined);
    };
  }, [refreshPreview, target]);

  if (!target) {
    return null;
  }

  const browserTabs = records(response?.page?.tabs);
  const activeBrowserTab = browserTabs.find((tab) => tab.active === true) || null;
  const activeBrowserTabTitle = text(activeBrowserTab?.title);
  const activeBrowserTabUrl = text(activeBrowserTab?.url);
  const pageTitle = pageValue(response, 'title') || activeBrowserTabTitle || target.title || t('browserSession.untitled');
  const pageUrl = pageValue(response, 'url') || activeBrowserTabUrl || target.url || '';
  const snapshot = pageValue(response, 'snapshot');
  const consoleMessages = records(consolePage?.console_messages ?? consolePage?.consoleMessages);
  const consoleErrors = records(consolePage?.js_errors ?? consolePage?.jsErrors);
  const networkResources = records(networkPage?.resources);
  const networkRequests = records(networkPage?.requests);
  const navigation = record(networkPage?.navigation);
  const networkDetail = record(networkDetailPage?.request);
  const requestHeaders = record(networkDetail?.request_headers ?? networkDetail?.requestHeaders);
  const responseHeaders = record(networkDetail?.response_headers ?? networkDetail?.responseHeaders);
  const requestBody = record(networkDetail?.request_body ?? networkDetail?.requestBody);
  const responseBody = record(networkDetail?.response_body ?? networkDetail?.responseBody);
  const harSanitization = record(harPage?.sanitization);
  const websocketFrames = records(websocketPage?.frames);
  const websocketTextPayloadsIncluded = websocketPage?.text_payloads_included === true
    || websocketPage?.textPayloadsIncluded === true;
  const previewSource = text(preview?.frame?.source);
  const previewMediaType = text(preview?.frame?.media_type);
  const previewDataUrl = preview?.frame_data_url || response?.screenshot_data_url;

  return (
    <div className="fixed inset-0 z-[70] flex bg-black/55 backdrop-blur-sm">
      <button
        type="button"
        className="flex-1 cursor-default"
        onClick={() => {
          targetRef.current = null;
          setTarget(null);
        }}
        aria-label={t('browserSession.closePanel')}
      />
      <section className="flex h-full w-full max-w-[1180px] flex-col border-l bg-background shadow-2xl">
        <header className="flex items-center gap-3 border-b px-4 py-3">
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold">{pageTitle}</div>
            <div className="truncate text-xs text-muted-foreground">
              {target.id} · {target.workspaceId}
            </div>
          </div>
          <span className="rounded-full bg-emerald-500/10 px-2 py-1 text-xs text-emerald-600 dark:text-emerald-400">
            {response?.status || target.status || 'active'}
          </span>
          <button
            type="button"
            onClick={() => { void runCommand('close'); }}
            disabled={loading}
            className="rounded-md border px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 disabled:opacity-50"
          >
            {t('browserSession.closeBrowser')}
          </button>
          <button
            type="button"
            onClick={() => {
              targetRef.current = null;
              setTarget(null);
            }}
            className="rounded-md p-2 text-muted-foreground hover:bg-muted hover:text-foreground"
            aria-label={t('browserSession.closePanel')}
          >
            ×
          </button>
        </header>

        <div className="flex items-stretch gap-1 overflow-x-auto border-b bg-muted/20 px-3 pt-2">
          {browserTabs.map((tab) => {
            const tabId = text(tab.tab_id ?? tab.tabId);
            const tabTitle = text(tab.title) || text(tab.url) || tabId || t('browserSession.untitled');
            const active = tab.active === true;
            return (
              <div
                key={tabId}
                className={`flex min-w-[140px] max-w-[240px] items-center rounded-t-md border border-b-0 ${active ? 'bg-background text-foreground' : 'bg-muted/40 text-muted-foreground'}`}
              >
                <button
                  type="button"
                  onClick={() => { if (!active && tabId) void runCommand('tab_switch', { tab_id: tabId }); }}
                  disabled={loading || !tabId}
                  className="min-w-0 flex-1 truncate px-3 py-2 text-left text-xs disabled:opacity-50"
                  aria-label={t('browserSession.switchToTab', { title: tabTitle })}
                  title={tabTitle}
                >
                  {tabTitle}
                </button>
                <button
                  type="button"
                  onClick={() => { if (tabId) void runCommand('tab_close', { tab_id: tabId }); }}
                  disabled={loading || !tabId || browserTabs.length <= 1}
                  className="mr-1 rounded px-1.5 py-1 text-xs hover:bg-destructive/10 hover:text-destructive disabled:opacity-30"
                  aria-label={t('browserSession.closeTab', { title: tabTitle })}
                  title={t('browserSession.closeTab', { title: tabTitle })}
                >
                  ×
                </button>
              </div>
            );
          })}
          <button
            type="button"
            onClick={() => { void runCommand('tab_new'); }}
            disabled={loading}
            className="mb-px shrink-0 rounded-t-md border px-3 py-2 text-xs hover:bg-muted disabled:opacity-50"
            aria-label={t('browserSession.newTab')}
            title={t('browserSession.newTab')}
          >
            +
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-2 border-b p-3">
          <button type="button" onClick={() => { void runCommand('back'); }} disabled={loading} className="rounded border px-2 py-1 text-xs hover:bg-muted disabled:opacity-50">←</button>
          <button type="button" onClick={() => { void runCommand('refresh'); }} disabled={loading} className="rounded border px-2 py-1 text-xs hover:bg-muted disabled:opacity-50">↻</button>
          <button type="button" onClick={() => { void runCommand('scroll', { direction: 'up' }); }} disabled={loading} className="rounded border px-2 py-1 text-xs hover:bg-muted disabled:opacity-50">↑</button>
          <button type="button" onClick={() => { void runCommand('scroll', { direction: 'down' }); }} disabled={loading} className="rounded border px-2 py-1 text-xs hover:bg-muted disabled:opacity-50">↓</button>
          <form
            className="flex min-w-[280px] flex-1 gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              if (address.trim()) {
                void runCommand('navigate', { url: address.trim() });
              }
            }}
          >
            <input
              value={address}
              onChange={(event) => setAddress(event.target.value)}
              placeholder={t('browserSession.addressPlaceholder')}
              className="min-w-0 flex-1 rounded-md border bg-background px-3 py-1.5 text-sm outline-none focus:ring-1 focus:ring-primary"
            />
            <button type="submit" disabled={loading || !address.trim()} className="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground disabled:opacity-50">
              {t('browserSession.go')}
            </button>
          </form>
        </div>

        {error ? <div className="border-b bg-destructive/10 px-4 py-2 text-xs text-destructive">{error}</div> : null}
        {response?.screenshot_error ? <div className="border-b bg-amber-500/10 px-4 py-2 text-xs text-amber-700 dark:text-amber-300">{response.screenshot_error}</div> : null}

        <div className="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[minmax(0,1fr)_360px]">
          <div className="relative flex min-h-0 items-center justify-center overflow-auto bg-neutral-950 p-3">
            {previewDataUrl ? (
              <>
                {previewMediaType === 'application/pdf' ? (
                  <BrowserPdfPreview
                    dataUrl={previewDataUrl}
                    width={number(preview?.frame?.width) || 1280}
                    height={number(preview?.frame?.height) || 720}
                    cropOffsetY={number(preview?.frame?.crop_offset_y)}
                    label={pageTitle}
                    loadingLabel={t('browserSession.loadingPreview')}
                    errorLabel={t('browserSession.previewFailed')}
                  />
                ) : (
                  <img
                    src={previewDataUrl}
                    alt={pageTitle}
                    className="max-h-full max-w-full rounded border border-white/10 object-contain shadow-xl"
                  />
                )}
                {previewSource ? (
                  <span className="absolute bottom-5 left-5 rounded bg-black/70 px-2 py-1 text-[10px] text-white/75">
                    {previewSource === 'screencast'
                      ? t('browserSession.liveScreencast')
                      : t('browserSession.livePreview')}
                  </span>
                ) : null}
              </>
            ) : (
              <div className="text-sm text-neutral-400">
                {loading ? t('browserSession.loading') : t('browserSession.noScreenshot')}
              </div>
            )}
          </div>

          <aside className="min-h-0 overflow-y-auto border-l bg-muted/20 p-3">
            <div className="mb-2 text-xs font-medium">{t('browserSession.manualControl')}</div>
            <div className="space-y-2 rounded-lg border bg-background p-3">
              <input
                value={elementRef}
                onChange={(event) => setElementRef(event.target.value)}
                placeholder="@e12"
                className="w-full rounded border bg-background px-2 py-1.5 text-sm"
              />
              <div className="flex gap-2">
                <button type="button" onClick={() => { void runCommand('click', { ref: elementRef }); }} disabled={loading || !elementRef.trim()} className="rounded border px-2 py-1 text-xs hover:bg-muted disabled:opacity-50">
                  {t('browserSession.clickRef')}
                </button>
                <button type="button" onClick={() => { void runCommand('press', { key: 'Enter' }); }} disabled={loading} className="rounded border px-2 py-1 text-xs hover:bg-muted disabled:opacity-50">Enter</button>
                <button type="button" onClick={() => { void runCommand('press', { key: 'Escape' }); }} disabled={loading} className="rounded border px-2 py-1 text-xs hover:bg-muted disabled:opacity-50">Esc</button>
              </div>
              <textarea
                value={inputText}
                onChange={(event) => setInputText(event.target.value)}
                placeholder={t('browserSession.typePlaceholder')}
                rows={3}
                className="w-full resize-y rounded border bg-background px-2 py-1.5 text-sm"
              />
              <button type="button" onClick={() => { void runCommand('type', { ref: elementRef, text: inputText }); }} disabled={loading || !elementRef.trim()} className="rounded bg-primary px-3 py-1.5 text-xs text-primary-foreground disabled:opacity-50">
                {t('browserSession.typeIntoRef')}
              </button>
              <div className="border-t pt-2">
                <div className="mb-1 text-[10px] font-medium text-muted-foreground">{t('browserSession.fileTransfer')}</div>
                <input
                  value={uploadPath}
                  onChange={(event) => setUploadPath(event.target.value)}
                  placeholder={t('browserSession.uploadPathPlaceholder')}
                  className="w-full rounded border bg-background px-2 py-1.5 text-sm"
                />
                <button type="button" onClick={() => { void runCommand('upload', { ref: elementRef, paths: [uploadPath.trim()] }); }} disabled={loading || !elementRef.trim() || !uploadPath.trim()} className="mt-2 rounded border px-2 py-1 text-xs hover:bg-muted disabled:opacity-50">
                  {t('browserSession.uploadToRef')}
                </button>
                <input
                  value={downloadPath}
                  onChange={(event) => setDownloadPath(event.target.value)}
                  placeholder={t('browserSession.downloadPathPlaceholder')}
                  className="mt-2 w-full rounded border bg-background px-2 py-1.5 text-sm"
                />
                <button type="button" onClick={() => { void runCommand('download', { ref: elementRef, path: downloadPath.trim() }); }} disabled={loading || !elementRef.trim() || !downloadPath.trim()} className="mt-2 rounded border px-2 py-1 text-xs hover:bg-muted disabled:opacity-50">
                  {t('browserSession.downloadFromRef')}
                </button>
              </div>
            </div>

            <div className="mb-2 mt-4 flex flex-wrap items-center gap-1">
              <button
                type="button"
                onClick={() => setDetailTab('snapshot')}
                className={`rounded px-2 py-1 text-xs ${detailTab === 'snapshot' ? 'bg-primary text-primary-foreground' : 'border hover:bg-muted'}`}
              >
                {t('browserSession.snapshotTab')}
              </button>
              <button
                type="button"
                onClick={() => {
                  setDetailTab('console');
                  void runCommand('console');
                }}
                className={`rounded px-2 py-1 text-xs ${detailTab === 'console' ? 'bg-primary text-primary-foreground' : 'border hover:bg-muted'}`}
              >
                {t('browserSession.consoleTab')}
              </button>
              <button
                type="button"
                onClick={() => {
                  setDetailTab('network');
                  void runCommand('network', { limit: 100 });
                }}
                className={`rounded px-2 py-1 text-xs ${detailTab === 'network' ? 'bg-primary text-primary-foreground' : 'border hover:bg-muted'}`}
              >
                {t('browserSession.networkTab')}
              </button>
              <button
                type="button"
                onClick={() => {
                  setDetailTab('websocket');
                  if (websocketActive) {
                    void runCommand('websocket_frames', {
                      limit: 100,
                      direction: websocketDirection || undefined,
                    });
                  }
                }}
                className={`rounded px-2 py-1 text-xs ${detailTab === 'websocket' ? 'bg-primary text-primary-foreground' : 'border hover:bg-muted'}`}
              >
                {t('browserSession.websocketTab')}
              </button>
            </div>

            {detailTab === 'snapshot' ? (
              <>
                <div className="mb-2 truncate text-[10px] text-muted-foreground">{pageUrl}</div>
                <pre className="max-h-[45vh] overflow-auto whitespace-pre-wrap rounded-lg border bg-background p-3 text-[11px] leading-5 text-muted-foreground">
                  {snapshot || t('browserSession.noSnapshot')}
                </pre>
              </>
            ) : null}

            {detailTab === 'console' ? (
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-[10px] text-muted-foreground">
                    {t('browserSession.consoleCounts', { messages: consoleMessages.length, errors: consoleErrors.length })}
                  </span>
                  <button type="button" onClick={() => { void runCommand('console', { clear: true }); }} disabled={loading} className="rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50">
                    {t('browserSession.clear')}
                  </button>
                </div>
                {consoleErrors.map((item, index) => (
                  <div key={`browser-console-error-${index}`} className="rounded border border-destructive/30 bg-destructive/5 p-2 text-[11px] text-destructive">
                    {text(item.message) || t('browserSession.unknownConsoleError')}
                  </div>
                ))}
                {consoleMessages.map((item, index) => (
                  <div key={`browser-console-message-${index}`} className="rounded border bg-background p-2 text-[11px]">
                    <div className="mb-1 text-[10px] font-medium uppercase text-muted-foreground">{text(item.type) || 'log'}</div>
                    <div className="whitespace-pre-wrap break-words">{text(item.text)}</div>
                  </div>
                ))}
                {consoleMessages.length === 0 && consoleErrors.length === 0 ? (
                  <div className="rounded border bg-background p-3 text-[11px] text-muted-foreground">{t('browserSession.noConsole')}</div>
                ) : null}
              </div>
            ) : null}

            {detailTab === 'network' ? (
              <div className="space-y-2">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-[10px] text-muted-foreground">
                    {t('browserSession.networkCounts', {
                      returned: number(networkPage?.returned_count ?? networkPage?.returnedCount),
                      total: number(networkPage?.request_count ?? networkPage?.requestCount ?? networkPage?.resource_count ?? networkPage?.resourceCount),
                    })}
                  </span>
                  <button type="button" onClick={() => { void runCommand('network', { clear: true, limit: 100 }); }} disabled={loading} className="rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50">
                    {t('browserSession.clear')}
                  </button>
                </div>
                <div className="space-y-2 rounded border bg-background p-2">
                  <div className="flex items-center justify-between gap-2 text-[10px] text-muted-foreground">
                    <span>{t('browserSession.harCapture')}</span>
                    <span>{harRecording ? t('browserSession.harRecording') : t('browserSession.harStopped')}</span>
                  </div>
                  <input
                    value={harPath}
                    onChange={(event) => setHarPath(event.target.value)}
                    placeholder={t('browserSession.harPathPlaceholder')}
                    className="w-full rounded border bg-background px-2 py-1.5 text-[11px]"
                  />
                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() => { void runCommand('har_start'); }}
                      disabled={loading}
                      className="rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50"
                    >
                      {t('browserSession.startHar')}
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        void runCommand('har_stop', {
                          path: harPath.trim(),
                          include_request_bodies: false,
                          include_response_bodies: false,
                          max_entries: 500,
                        });
                      }}
                      disabled={loading || !harPath.trim()}
                      className="rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50"
                    >
                      {t('browserSession.stopHar')}
                    </button>
                  </div>
                  {harPage ? (
                    <div className="text-[10px] leading-4 text-muted-foreground">
                      {text(harPage.path)
                        ? t('browserSession.harExported', {
                          path: text(harPage.path),
                          entries: number(harSanitization?.exported_entries ?? harSanitization?.exportedEntries),
                          bytes: number(harPage.bytes),
                        })
                        : text(harPage.status)}
                    </div>
                  ) : null}
                  <div className="text-[10px] leading-4 text-muted-foreground">{t('browserSession.harPrivacy')}</div>
                </div>
                {navigation ? (
                  <div className="rounded border bg-background p-2 text-[11px]">
                    <div className="mb-1 text-[10px] font-medium text-muted-foreground">{t('browserSession.navigation')}</div>
                    <div className="break-all">{text(navigation.url)}</div>
                    <div className="mt-1 text-[10px] text-muted-foreground">
                      {text(navigation.type)} · {number(navigation.duration_ms ?? navigation.durationMs).toFixed(1)} ms
                    </div>
                  </div>
                ) : null}
                {networkRequests.map((item, index) => {
                  const requestId = text(item.request_id ?? item.requestId);
                  return (
                    <div key={`browser-network-request-${requestId || index}`} className="rounded border bg-background p-2 text-[11px]">
                      <div className="flex items-start justify-between gap-2">
                        <div className="min-w-0 flex-1">
                          <div className="break-all">{text(item.url)}</div>
                          <div className="mt-1 text-[10px] text-muted-foreground">
                            {text(item.method) || 'GET'} · {number(item.status)} · {text(item.resource_type ?? item.resourceType) || 'Other'}
                          </div>
                        </div>
                        <button
                          type="button"
                          onClick={() => { void runCommand('network_request', { request_id: requestId }); }}
                          disabled={loading || !requestId}
                          className="rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50"
                        >
                          {t('browserSession.requestDetail')}
                        </button>
                      </div>
                    </div>
                  );
                })}
                {networkResources.map((item, index) => (
                  <div key={`browser-network-resource-${index}`} className="rounded border bg-background p-2 text-[11px]">
                    <div className="break-all">{text(item.url)}</div>
                    <div className="mt-1 text-[10px] text-muted-foreground">
                      {text(item.initiator_type ?? item.initiatorType) || 'other'} · {number(item.duration_ms ?? item.durationMs).toFixed(1)} ms · {Math.round(number(item.transfer_size ?? item.transferSize))} B
                    </div>
                  </div>
                ))}
                {networkRequests.length === 0 && networkResources.length === 0 ? (
                  <div className="rounded border bg-background p-3 text-[11px] text-muted-foreground">{t('browserSession.noNetwork')}</div>
                ) : null}
                {networkDetail ? (
                  <div className="space-y-2 rounded border bg-background p-2 text-[11px]">
                    <div className="font-medium">{t('browserSession.requestDetail')}</div>
                    <div className="break-all">{text(networkDetail.method)} · {number(networkDetail.status)} · {text(networkDetail.url)}</div>
                    <div className="grid grid-cols-1 gap-2">
                      <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 p-2 text-[10px]">{JSON.stringify(requestHeaders || {}, null, 2)}</pre>
                      <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 p-2 text-[10px]">{JSON.stringify(responseHeaders || {}, null, 2)}</pre>
                    </div>
                    {text(requestBody?.text) ? <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 p-2 text-[10px]">{text(requestBody?.text)}</pre> : null}
                    {text(responseBody?.text) ? <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 p-2 text-[10px]">{text(responseBody?.text)}</pre> : null}
                    <button
                      type="button"
                      onClick={() => {
                        const requestId = text(networkDetail.request_id ?? networkDetail.requestId);
                        void runCommand('network_request', {
                          request_id: requestId,
                          include_request_body: true,
                          include_response_body: true,
                          max_body_chars: 16384,
                        });
                      }}
                      disabled={loading || !text(networkDetail.request_id ?? networkDetail.requestId)}
                      className="rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50"
                    >
                      {t('browserSession.loadRedactedBodies')}
                    </button>
                  </div>
                ) : null}
                <div className="text-[10px] leading-4 text-muted-foreground">{t('browserSession.networkPrivacy')}</div>
              </div>
            ) : null}

            {detailTab === 'websocket' ? (
              <div className="space-y-2">
                <div className="rounded border bg-background p-2">
                  <div className="flex items-center justify-between gap-2 text-[10px] text-muted-foreground">
                    <span>{t('browserSession.websocketObservation')}</span>
                    <span>{websocketActive ? t('browserSession.websocketActive') : t('browserSession.websocketStopped')}</span>
                  </div>
                  <div className="mt-2 flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() => { void runCommand('websocket_start'); }}
                      disabled={loading || websocketActive}
                      className="rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50"
                    >
                      {t('browserSession.startWebsocket')}
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        void runCommand('websocket_frames', {
                          limit: 100,
                          direction: websocketDirection || undefined,
                        });
                      }}
                      disabled={loading}
                      className="rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50"
                    >
                      {t('browserSession.refreshWebsocket')}
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        void runCommand('websocket_frames', {
                          clear: true,
                          limit: 100,
                          direction: websocketDirection || undefined,
                        });
                      }}
                      disabled={loading || !websocketActive}
                      className="rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50"
                    >
                      {t('browserSession.clearWebsocket')}
                    </button>
                    <button
                      type="button"
                      onClick={() => { void runCommand('websocket_stop'); }}
                      disabled={loading}
                      className="rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50"
                    >
                      {t('browserSession.stopWebsocket')}
                    </button>
                  </div>
                  <div className="mt-2 flex items-center gap-2">
                    <label className="text-[10px] text-muted-foreground" htmlFor="browser-websocket-direction">
                      {t('browserSession.websocketDirection')}
                    </label>
                    <select
                      id="browser-websocket-direction"
                      value={websocketDirection}
                      onChange={(event) => setWebsocketDirection(event.target.value as '' | 'sent' | 'received')}
                      className="rounded border bg-background px-2 py-1 text-[10px]"
                    >
                      <option value="">{t('browserSession.websocketAllDirections')}</option>
                      <option value="sent">{t('browserSession.websocketSent')}</option>
                      <option value="received">{t('browserSession.websocketReceived')}</option>
                    </select>
                  </div>
                  <div className="mt-2 text-[10px] leading-4 text-muted-foreground">
                    {t('browserSession.websocketCounts', {
                      returned: number(websocketPage?.returned_count ?? websocketPage?.returnedCount),
                      total: number(websocketPage?.total_frame_count ?? websocketPage?.totalFrameCount),
                      sockets: number(websocketPage?.socket_count ?? websocketPage?.socketCount),
                    })}
                  </div>
                  <button
                    type="button"
                    onClick={() => {
                      void runCommand('websocket_frames', {
                        limit: 100,
                        direction: websocketDirection || undefined,
                        include_text_payloads: true,
                        max_payload_chars: 1024,
                      });
                    }}
                    disabled={loading || !websocketActive}
                    className="mt-2 rounded border px-2 py-1 text-[10px] hover:bg-muted disabled:opacity-50"
                  >
                    {t('browserSession.loadRedactedWebsocketPayloads')}
                  </button>
                </div>
                {websocketFrames.map((frame, index) => {
                  const requestId = text(frame.request_id ?? frame.requestId);
                  const payload = text(frame.text_payload ?? frame.textPayload);
                  return (
                    <div key={`browser-websocket-frame-${number(frame.sequence) || index}`} className="rounded border bg-background p-2 text-[11px]">
                      <div className="break-all">{text(frame.url) || requestId}</div>
                      <div className="mt-1 text-[10px] text-muted-foreground">
                        {[text(frame.direction), text(frame.frame_type ?? frame.frameType), `${number(frame.payload_bytes ?? frame.payloadBytes)} B`, requestId].filter(Boolean).join(' · ')}
                      </div>
                      {payload ? (
                        <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 p-2 text-[10px]">{payload}</pre>
                      ) : null}
                    </div>
                  );
                })}
                {websocketFrames.length === 0 ? (
                  <div className="rounded border bg-background p-3 text-[11px] text-muted-foreground">
                    {websocketActive ? t('browserSession.noWebsocketFrames') : t('browserSession.websocketStartHint')}
                  </div>
                ) : null}
                <div className="text-[10px] leading-4 text-muted-foreground">
                  {websocketTextPayloadsIncluded
                    ? t('browserSession.websocketPayloadsVisible')
                    : t('browserSession.websocketPrivacy')}
                </div>
              </div>
            ) : null}
          </aside>
        </div>
      </section>
    </div>
  );
};

export default BrowserSessionPanel;
