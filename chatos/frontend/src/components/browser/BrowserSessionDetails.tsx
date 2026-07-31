// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type { BrowserSessionCommandPayload } from '../../lib/api/localRuntime/browserSession';
import {
  number,
  record,
  records,
  text,
  type BrowserDetailTab,
} from './browserSessionView';

export type BrowserWebsocketDirection = '' | 'sent' | 'received';

interface BrowserSessionDetailsProps {
  consolePage: Record<string, unknown> | null;
  detailTab: BrowserDetailTab;
  harPage: Record<string, unknown> | null;
  harPath: string;
  harRecording: boolean;
  loading: boolean;
  networkDetailPage: Record<string, unknown> | null;
  networkPage: Record<string, unknown> | null;
  onDetailTabChange: (tab: BrowserDetailTab) => void;
  onHarPathChange: (path: string) => void;
  onWebsocketDirectionChange: (direction: BrowserWebsocketDirection) => void;
  pageUrl: string;
  runCommand: (
    action: BrowserSessionCommandPayload['action'],
    extra?: Partial<BrowserSessionCommandPayload>,
  ) => Promise<void>;
  snapshot: string;
  websocketActive: boolean;
  websocketDirection: BrowserWebsocketDirection;
  websocketPage: Record<string, unknown> | null;
}

const BrowserSessionDetails: React.FC<BrowserSessionDetailsProps> = ({
  consolePage,
  detailTab,
  harPage,
  harPath,
  harRecording,
  loading,
  networkDetailPage,
  networkPage,
  onDetailTabChange,
  onHarPathChange,
  onWebsocketDirectionChange,
  pageUrl,
  runCommand,
  snapshot,
  websocketActive,
  websocketDirection,
  websocketPage,
}) => {
  const { t } = useI18n();
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

  return (
    <>
      <div className="mb-2 mt-4 flex flex-wrap items-center gap-1">
        <button
          type="button"
          onClick={() => onDetailTabChange('snapshot')}
          className={`rounded px-2 py-1 text-xs ${detailTab === 'snapshot' ? 'bg-primary text-primary-foreground' : 'border hover:bg-muted'}`}
        >
          {t('browserSession.snapshotTab')}
        </button>
        <button
          type="button"
          onClick={() => {
            onDetailTabChange('console');
            void runCommand('console');
          }}
          className={`rounded px-2 py-1 text-xs ${detailTab === 'console' ? 'bg-primary text-primary-foreground' : 'border hover:bg-muted'}`}
        >
          {t('browserSession.consoleTab')}
        </button>
        <button
          type="button"
          onClick={() => {
            onDetailTabChange('network');
            void runCommand('network', { limit: 100 });
          }}
          className={`rounded px-2 py-1 text-xs ${detailTab === 'network' ? 'bg-primary text-primary-foreground' : 'border hover:bg-muted'}`}
        >
          {t('browserSession.networkTab')}
        </button>
        <button
          type="button"
          onClick={() => {
            onDetailTabChange('websocket');
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
              onChange={(event) => onHarPathChange(event.target.value)}
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
                onChange={(event) => onWebsocketDirectionChange(event.target.value as BrowserWebsocketDirection)}
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
    </>
  );
};

export default BrowserSessionDetails;
