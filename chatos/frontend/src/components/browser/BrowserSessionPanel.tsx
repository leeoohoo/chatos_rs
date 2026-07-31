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
import BrowserSessionDetails, { type BrowserWebsocketDirection } from './BrowserSessionDetails';
import {
  number,
  pageValue,
  readableError,
  record,
  records,
  text,
  type BrowserDetailTab,
  type BrowserPreviewCursor,
  type BrowserPreviewPoint,
} from './browserSessionView';


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
  const [websocketDirection, setWebsocketDirection] = useState<BrowserWebsocketDirection>('');
  const requestActive = useRef(false);
  const manualCommandPending = useRef(false);
  const previewRequestActive = useRef(false);
  const previewFrameSequence = useRef(0);
  const previewImageRef = useRef<HTMLImageElement | null>(null);
  const previewInputQueue = useRef<Promise<void>>(Promise.resolve());
  const previewWheelSentAt = useRef(0);
  const activeBrowserTabId = useRef('');
  const targetRef = useRef<BrowserSessionUiTarget | null>(null);
  const [previewCursor, setPreviewCursor] = useState<BrowserPreviewCursor | null>(null);

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
    previewInputQueue.current = Promise.resolve();
    setPreviewCursor(null);
    activeBrowserTabId.current = '';
  }), []);

  const previewPointFromEvent = useCallback((
    event: React.MouseEvent<HTMLElement> | React.WheelEvent<HTMLElement>,
  ): BrowserPreviewPoint | null => {
    const image = previewImageRef.current;
    if (!image) {
      return null;
    }
    const rect = image.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
      return null;
    }
    const displayX = event.clientX - rect.left;
    const displayY = event.clientY - rect.top;
    if (displayX < 0 || displayY < 0 || displayX > rect.width || displayY > rect.height) {
      return null;
    }
    const sourceWidth = number(preview?.frame?.width) || image.naturalWidth;
    const sourceHeight = number(preview?.frame?.height) || image.naturalHeight;
    if (sourceWidth <= 0 || sourceHeight <= 0) {
      return null;
    }
    return {
      x: displayX,
      y: displayY,
      browserX: (displayX / rect.width) * sourceWidth,
      browserY: (displayY / rect.height) * sourceHeight,
    };
  }, [preview?.frame]);

  const sendPreviewInputCommand = useCallback((
    action: BrowserSessionCommandPayload['action'],
    extra: Partial<BrowserSessionCommandPayload> = {},
  ) => {
    const currentTarget = targetRef.current;
    if (!currentTarget) {
      return;
    }
    const queued = previewInputQueue.current
      .catch(() => undefined)
      .then(async () => {
        if (targetRef.current?.id !== currentTarget.id) {
          return;
        }
        try {
          await sendBrowserSessionCommand(currentTarget.id, {
            workspace_id: currentTarget.workspaceId,
            action,
            ...extra,
          });
        } catch (commandError) {
          if (targetRef.current?.id === currentTarget.id) {
            setError(readableError(commandError));
          }
        }
      });
    previewInputQueue.current = queued;
  }, []);

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

  const handlePreviewMouseMove = useCallback((event: React.MouseEvent<HTMLElement>) => {
    const point = previewPointFromEvent(event);
    setPreviewCursor(point ? { x: point.x, y: point.y } : null);
  }, [previewPointFromEvent]);

  const handlePreviewMouseLeave = useCallback(() => {
    setPreviewCursor(null);
  }, []);

  const handlePreviewClick = useCallback((event: React.MouseEvent<HTMLElement>) => {
    const point = previewPointFromEvent(event);
    if (!point) {
      return;
    }
    event.preventDefault();
    event.currentTarget.focus();
    sendPreviewInputCommand('click_point', {
      x: point.browserX,
      y: point.browserY,
      button: 'left',
      click_count: Math.min(Math.max(event.detail || 1, 1), 3),
    });
  }, [previewPointFromEvent, sendPreviewInputCommand]);

  const handlePreviewContextMenu = useCallback((event: React.MouseEvent<HTMLElement>) => {
    const point = previewPointFromEvent(event);
    event.preventDefault();
    if (!point) {
      return;
    }
    event.currentTarget.focus();
    sendPreviewInputCommand('click_point', {
      x: point.browserX,
      y: point.browserY,
      button: 'right',
      click_count: 1,
    });
  }, [previewPointFromEvent, sendPreviewInputCommand]);

  const handlePreviewWheel = useCallback((event: React.WheelEvent<HTMLElement>) => {
    const point = previewPointFromEvent(event);
    if (!point) {
      return;
    }
    event.preventDefault();
    event.currentTarget.focus();
    const now = Date.now();
    if (now - previewWheelSentAt.current < 35) {
      return;
    }
    previewWheelSentAt.current = now;
    sendPreviewInputCommand('scroll_delta', {
      x: point.browserX,
      y: point.browserY,
      delta_x: event.deltaX,
      delta_y: event.deltaY,
    });
  }, [previewPointFromEvent, sendPreviewInputCommand]);

  const handlePreviewKeyDown = useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    if (event.nativeEvent.isComposing || event.metaKey || event.ctrlKey || event.altKey) {
      return;
    }
    const specialKeys = new Set([
      'Enter',
      'Escape',
      'Backspace',
      'Delete',
      'Tab',
      'ArrowUp',
      'ArrowDown',
      'ArrowLeft',
      'ArrowRight',
      'Home',
      'End',
      'PageUp',
      'PageDown',
    ]);
    if (event.key.length === 1) {
      event.preventDefault();
      sendPreviewInputCommand('type_text', { text: event.key });
      return;
    }
    if (specialKeys.has(event.key)) {
      event.preventDefault();
      sendPreviewInputCommand('press', { key: event.key });
    }
  }, [sendPreviewInputCommand]);

  const handlePreviewPaste = useCallback((event: React.ClipboardEvent<HTMLElement>) => {
    const pasted = event.clipboardData.getData('text');
    if (!pasted) {
      return;
    }
    event.preventDefault();
    sendPreviewInputCommand('type_text', { text: pasted });
  }, [sendPreviewInputCommand]);

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
                  <div
                    role="application"
                    aria-label={t('browserSession.interactivePreview')}
                    tabIndex={0}
                    className="relative max-h-full max-w-full cursor-none rounded outline-none ring-primary/40 focus-visible:ring-2"
                    onMouseMove={handlePreviewMouseMove}
                    onMouseLeave={handlePreviewMouseLeave}
                    onClick={handlePreviewClick}
                    onContextMenu={handlePreviewContextMenu}
                    onWheel={handlePreviewWheel}
                    onKeyDown={handlePreviewKeyDown}
                    onPaste={handlePreviewPaste}
                  >
                    <img
                      ref={previewImageRef}
                      src={previewDataUrl}
                      alt={pageTitle}
                      draggable={false}
                      className="max-h-full max-w-full select-none rounded border border-white/10 object-contain shadow-xl"
                    />
                    {previewCursor ? (
                      <div
                        className="pointer-events-none absolute z-10"
                        style={{
                          left: previewCursor.x,
                          top: previewCursor.y,
                          transform: 'translate(2px, 2px)',
                        }}
                      >
                        <svg
                          width="20"
                          height="24"
                          viewBox="0 0 20 24"
                          className="drop-shadow-[0_1px_2px_rgba(0,0,0,0.8)]"
                          aria-hidden="true"
                        >
                          <path d="M2 2 L2 19 L7 14 L10 22 L14 20 L11 12 L18 12 Z" fill="white" />
                          <path d="M2 2 L2 19 L7 14 L10 22 L14 20 L11 12 L18 12 Z" fill="none" stroke="rgb(8 145 178)" strokeWidth="1.5" />
                        </svg>
                      </div>
                    ) : null}
                    <span className="absolute left-2 top-2 rounded bg-black/70 px-2 py-1 text-[10px] text-white/75">
                      {t('browserSession.previewControlHint')}
                    </span>
                  </div>
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

            <BrowserSessionDetails
              consolePage={consolePage}
              detailTab={detailTab}
              harPage={harPage}
              harPath={harPath}
              harRecording={harRecording}
              loading={loading}
              networkDetailPage={networkDetailPage}
              networkPage={networkPage}
              onDetailTabChange={setDetailTab}
              onHarPathChange={setHarPath}
              onWebsocketDirectionChange={setWebsocketDirection}
              pageUrl={pageUrl}
              runCommand={runCommand}
              snapshot={snapshot}
              websocketActive={websocketActive}
              websocketDirection={websocketDirection}
              websocketPage={websocketPage}
            />
          </aside>
        </div>
      </section>
    </div>
  );
};

export default BrowserSessionPanel;
