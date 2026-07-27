// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { localRuntimeBridgeAvailable } from '../../../lib/api/localRuntime';
import { openBrowserSessionPanel } from '../../../lib/browserSessionUi';
import { translateToolTitle } from '../../../i18n/toolText';
import { stringifyJsonPreview } from '../../toolDetails/textPreview';
import { ExtractResultsBriefCard, SearchResultsBriefCard } from '../shared/researchCards';
import { RowsCard, StringListCard, TextBlockCard, formatToolCardCount, renderCardHeader } from '../shared/primitives';
import { asArray, asBoolean, asNumber, asRecord, asString } from '../shared/value';

const isMeaningfulBrowserUrl = (url: string): boolean => {
  const normalized = url.trim().toLowerCase();
  if (!normalized) {
    return false;
  }

  return ![
    'about:blank',
    'about:srcdoc',
    'about:newtab',
    'data:,',
    'chrome://newtab/',
    'chrome://new-tab-page/',
    'edge://newtab/',
  ].includes(normalized);
};

const PageStateCard: React.FC<{ record: Record<string, unknown> }> = ({ record }) => {
  const { locale, t } = useI18n();
  const title = asString(record.title).trim();
  const rawUrl = asString(record.url).trim();
  const url = isMeaningfulBrowserUrl(rawUrl) ? rawUrl : '';
  const warning = asString(record.page_state_warning ?? record.pageStateWarning).trim();
  const pageStateAvailable = asBoolean(record.page_state_available ?? record.pageStateAvailable);
  const state = !title && !url && pageStateAvailable === false ? t('toolSummary.noOpenPage') : '';

  if (!title && !url && !warning && !state) return null;

  return (
    <RowsCard
      title={translateToolTitle('Page state', locale)}
      rows={[
        { key: 'state', value: state },
        { key: 'title', value: title },
        { key: 'url', value: url },
        { key: 'warning', value: warning },
      ]}
    />
  );
};

const BrowserSessionCard: React.FC<{ record: Record<string, unknown> }> = ({ record }) => {
  const { locale, t } = useI18n();
  const session = asRecord(record.browser_session ?? record.browserSession);
  if (!session) return null;
  const sessionId = asString(session.id).trim();
  const mode = asString(session.mode).trim();
  const status = asString(session.status).trim();
  const workspaceId = asString(session.workspace_id ?? session.workspaceId).trim();
  const deviceId = asString(session.device_id ?? session.deviceId).trim();
  const projectId = asString(session.project_id ?? session.projectId).trim();
  const canOpen = Boolean(
    sessionId
    && mode === 'managed'
    && workspaceId
    && localRuntimeBridgeAvailable(),
  );

  return (
    <>
      <RowsCard
        title={translateToolTitle('Browser session', locale)}
        rows={[
          { key: 'id', value: sessionId },
          { key: 'mode', value: mode },
          { key: 'status', value: status },
          { key: 'event', value: asString(session.event).trim() },
          { key: 'workspace', value: workspaceId },
          { key: 'device', value: deviceId },
        ]}
        fullWidth
      />
      {canOpen ? (
        <div className="tool-detail-card tool-detail-card--full">
          <button
            type="button"
            onClick={() => openBrowserSessionPanel({
              id: sessionId,
              workspaceId,
              deviceId: deviceId || null,
              projectId: projectId || null,
              mode: 'managed',
              status: status || null,
              url: asString(record.url).trim() || null,
              title: asString(record.title).trim() || null,
            })}
            className="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90"
          >
            {t('browserSession.openPanel')}
          </button>
        </div>
      ) : null}
    </>
  );
};

const ConsolePreviewCards: React.FC<{ record: Record<string, unknown> }> = ({ record }) => {
  const { locale, t } = useI18n();
  const messages = asArray(record.messages_brief ?? record.messagesBrief)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);
  const errors = asArray(record.errors_brief ?? record.errorsBrief)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  return (
    <>
      {messages.length > 0 && (
        <div className="tool-detail-card tool-detail-card--full">
          {renderCardHeader(
            translateToolTitle('Console messages', locale),
            formatToolCardCount(t, 'messages', messages.length),
          )}
          <div className="tool-detail-list">
            {messages.map((item, index) => (
              <div key={`console-msg-${index}`} className="tool-detail-item">
                <div className="tool-detail-item-meta">
                  {asString(item.type).trim() || 'log'}
                </div>
                <div className="tool-detail-item-body">
                  {asString(item.text_preview ?? item.textPreview).trim()}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {errors.length > 0 && (
        <div className="tool-detail-card tool-detail-card--full">
          {renderCardHeader(
            translateToolTitle('JavaScript errors', locale),
            formatToolCardCount(t, 'errors', errors.length),
          )}
          <div className="tool-detail-list">
            {errors.map((item, index) => (
              <div key={`console-err-${index}`} className="tool-detail-item">
                <div className="tool-detail-item-body">
                  {asString(item.message_preview ?? item.messagePreview).trim()}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </>
  );
};

const BrowserTabsCard: React.FC<{ record: Record<string, unknown> }> = ({ record }) => {
  const { locale, t } = useI18n();
  const tabs = asArray(record.tabs)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);
  if (tabs.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader(
        translateToolTitle('Browser tabs', locale),
        formatToolCardCount(t, 'tabs', tabs.length),
      )}
      <div className="tool-detail-list">
        {tabs.map((tab, index) => {
          const tabId = asString(tab.tab_id ?? tab.tabId).trim();
          const title = asString(tab.title).trim();
          const url = asString(tab.url).trim();
          const active = asBoolean(tab.active) === true;
          return (
            <div key={`browser-tab-${tabId || index}`} className="tool-detail-item">
              <div className="tool-detail-item-title break-all">
                {active ? '● ' : ''}{title || url || tabId}
              </div>
              <div className="tool-detail-item-meta break-all">
                {[tabId, url].filter(Boolean).join(' · ')}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

const NetworkPreviewCards: React.FC<{ record: Record<string, unknown> }> = ({ record }) => {
  const { locale, t } = useI18n();
  const requests = asArray(record.requests)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);
  const resources = asArray(record.resources)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);
  const navigation = asRecord(record.navigation);

  return (
    <>
      <RowsCard
        title={translateToolTitle('Network summary', locale)}
        rows={[
          { key: 'requests', value: asNumber(record.request_count ?? record.requestCount ?? record.resource_count ?? record.resourceCount) },
          { key: 'returned', value: asNumber(record.returned_count ?? record.returnedCount) },
          { key: 'omitted', value: asNumber(record.omitted_count ?? record.omittedCount) },
          { key: 'truncated', value: asBoolean(record.truncated) },
        ]}
        fullWidth
      />
      {navigation ? (
        <RowsCard
          title={translateToolTitle('Navigation timing', locale)}
          rows={[
            { key: 'url', value: asString(navigation.url).trim() },
            { key: 'type', value: asString(navigation.type).trim() },
            { key: 'duration ms', value: asNumber(navigation.duration_ms ?? navigation.durationMs) },
            { key: 'transfer bytes', value: asNumber(navigation.transfer_size ?? navigation.transferSize) },
          ]}
          fullWidth
        />
      ) : null}
      {resources.length > 0 ? (
        <div className="tool-detail-card tool-detail-card--full">
          {renderCardHeader(
            translateToolTitle('Network resources', locale),
            formatToolCardCount(t, 'entries', resources.length),
          )}
          <div className="tool-detail-list">
            {resources.map((item, index) => (
              <div key={`network-resource-${index}`} className="tool-detail-item">
                <div className="tool-detail-item-title break-all">
                  {asString(item.url).trim()}
                </div>
                <div className="tool-detail-item-meta">
                  {[
                    asString(item.initiator_type ?? item.initiatorType).trim() || 'other',
                    `${asNumber(item.duration_ms ?? item.durationMs) ?? 0} ms`,
                    `${asNumber(item.transfer_size ?? item.transferSize) ?? 0} B`,
                  ].join(' · ')}
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : null}
      {requests.length > 0 ? (
        <div className="tool-detail-card tool-detail-card--full">
          {renderCardHeader(
            translateToolTitle('Network requests', locale),
            formatToolCardCount(t, 'entries', requests.length),
          )}
          <div className="tool-detail-list">
            {requests.map((item, index) => (
              <div key={`network-request-${asString(item.request_id ?? item.requestId).trim() || index}`} className="tool-detail-item">
                <div className="tool-detail-item-title break-all">
                  {asString(item.url).trim()}
                </div>
                <div className="tool-detail-item-meta">
                  {[
                    asString(item.method).trim() || 'GET',
                    asNumber(item.status) ?? 0,
                    asString(item.resource_type ?? item.resourceType).trim() || 'Other',
                    asString(item.request_id ?? item.requestId).trim(),
                  ].filter(Boolean).join(' · ')}
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </>
  );
};

const NetworkRequestDetailCards: React.FC<{ record: Record<string, unknown> }> = ({ record }) => {
  const { locale } = useI18n();
  const request = asRecord(record.request);
  if (!request) return null;
  const requestBody = asRecord(request.request_body ?? request.requestBody);
  const responseBody = asRecord(request.response_body ?? request.responseBody);
  return (
    <>
      <RowsCard
        title={translateToolTitle('Network request detail', locale)}
        rows={[
          { key: 'request id', value: asString(request.request_id ?? request.requestId).trim() },
          { key: 'method', value: asString(request.method).trim() },
          { key: 'status', value: asNumber(request.status) },
          { key: 'type', value: asString(request.resource_type ?? request.resourceType).trim() },
          { key: 'url', value: asString(request.url).trim() },
        ]}
        fullWidth
      />
      <TextBlockCard
        title={translateToolTitle('Request headers', locale)}
        content={JSON.stringify(asRecord(request.request_headers ?? request.requestHeaders) || {}, null, 2)}
      />
      <TextBlockCard
        title={translateToolTitle('Response headers', locale)}
        content={JSON.stringify(asRecord(request.response_headers ?? request.responseHeaders) || {}, null, 2)}
      />
      <TextBlockCard title={translateToolTitle('Request body', locale)} content={asString(requestBody?.text)} />
      <TextBlockCard title={translateToolTitle('Response body', locale)} content={asString(responseBody?.text)} />
    </>
  );
};

const FileTransferCards: React.FC<{ displayName: string; record: Record<string, unknown> }> = ({ displayName, record }) => {
  const { locale } = useI18n();
  if (displayName === 'browser_download') {
    return (
      <RowsCard
        title={translateToolTitle('Downloaded file', locale)}
        rows={[
          { key: 'path', value: asString(record.path).trim() },
          { key: 'size bytes', value: asNumber(record.bytes) },
          { key: 'overwrote existing', value: asBoolean(record.overwrote_existing ?? record.overwroteExisting) },
        ]}
        fullWidth
      />
    );
  }
  if (displayName === 'chrome_tab_download') {
    return (
      <RowsCard
        title={translateToolTitle('Downloaded file', locale)}
        rows={[
          { key: 'path', value: asString(record.path).trim() },
          { key: 'size bytes', value: asNumber(record.size_bytes ?? record.sizeBytes) },
          { key: 'sha256', value: asString(record.sha256).trim() },
          { key: 'chunk count', value: asNumber(record.chunk_count ?? record.chunkCount) },
          { key: 'mime type', value: asString(record.mime_type ?? record.mimeType).trim() },
          { key: 'source kind', value: asString(record.source_kind ?? record.sourceKind).trim() },
          { key: 'source url', value: asString(record.source_url ?? record.sourceUrl).trim() },
          { key: 'overwrote existing', value: asBoolean(record.overwritten) },
        ]}
        fullWidth
      />
    );
  }
  if (displayName === 'browser_upload') {
    return (
      <RowsCard
        title={translateToolTitle('Uploaded files', locale)}
        rows={[
          { key: 'files', value: asNumber(record.file_count ?? record.fileCount) },
          { key: 'size bytes', value: asNumber(record.total_bytes ?? record.totalBytes) },
        ]}
        fullWidth
      />
    );
  }
  return null;
};

const HarCaptureCards: React.FC<{ displayName: string; record: Record<string, unknown> }> = ({ displayName, record }) => {
  const { locale } = useI18n();
  if (displayName === 'browser_har_start') {
    return (
      <RowsCard
        title={translateToolTitle('HAR capture', locale)}
        rows={[
          { key: 'status', value: asString(record.status).trim() },
          { key: 'request bodies included', value: asBoolean(record.request_bodies_included ?? record.requestBodiesIncluded) },
          { key: 'response bodies included', value: asBoolean(record.response_bodies_included ?? record.responseBodiesIncluded) },
        ]}
        fullWidth
      />
    );
  }
  if (displayName === 'browser_har_stop') {
    const sanitization = asRecord(record.sanitization);
    return (
      <RowsCard
        title={translateToolTitle('Sanitized HAR export', locale)}
        rows={[
          { key: 'path', value: asString(record.path).trim() },
          { key: 'size bytes', value: asNumber(record.bytes) },
          { key: 'exported entries', value: asNumber(sanitization?.exported_entries ?? sanitization?.exportedEntries) },
          { key: 'raw capture deleted', value: asBoolean(record.raw_capture_deleted ?? record.rawCaptureDeleted) },
          { key: 'request bodies included', value: asBoolean(record.request_bodies_included ?? record.requestBodiesIncluded) },
          { key: 'response bodies included', value: asBoolean(record.response_bodies_included ?? record.responseBodiesIncluded) },
          { key: 'overwrote existing', value: asBoolean(record.overwrote_existing ?? record.overwroteExisting) },
        ]}
        fullWidth
      />
    );
  }
  return null;
};

const WebSocketCaptureCards: React.FC<{ displayName: string; record: Record<string, unknown> }> = ({ displayName, record }) => {
  const { locale, t } = useI18n();
  if (!displayName.startsWith('browser_websocket_')) return null;
  const frames = asArray(record.frames)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);
  return (
    <>
      <RowsCard
        title={translateToolTitle('WebSocket observation', locale)}
        rows={[
          { key: 'status', value: asString(record.status).trim() },
          { key: 'active', value: asBoolean(record.active) },
          { key: 'sockets', value: asNumber(record.socket_count ?? record.socketCount) },
          { key: 'open sockets', value: asNumber(record.open_socket_count ?? record.openSocketCount) },
          { key: 'total frames', value: asNumber(record.total_frame_count ?? record.totalFrameCount) },
          { key: 'returned frames', value: asNumber(record.returned_count ?? record.returnedCount) },
          { key: 'dropped frames', value: asNumber(record.dropped_frame_count ?? record.droppedFrameCount) },
          { key: 'protocol errors', value: asNumber(record.protocol_error_count ?? record.protocolErrorCount) },
          { key: 'text payloads included', value: asBoolean(record.text_payloads_included ?? record.textPayloadsIncluded) },
          { key: 'binary payloads included', value: asBoolean(record.binary_payloads_included ?? record.binaryPayloadsIncluded) },
        ]}
        fullWidth
      />
      {frames.length > 0 ? (
        <div className="tool-detail-card tool-detail-card--full">
          {renderCardHeader(
            translateToolTitle('WebSocket frames', locale),
            formatToolCardCount(t, 'entries', frames.length),
          )}
          <div className="tool-detail-list">
            {frames.map((frame, index) => {
              const requestId = asString(frame.request_id ?? frame.requestId).trim();
              const payload = asString(frame.text_payload ?? frame.textPayload);
              return (
                <div key={`websocket-frame-${asNumber(frame.sequence) ?? index}`} className="tool-detail-item">
                  <div className="tool-detail-item-title break-all">
                    {asString(frame.url).trim() || requestId}
                  </div>
                  <div className="tool-detail-item-meta">
                    {[
                      asString(frame.direction).trim(),
                      asString(frame.frame_type ?? frame.frameType).trim(),
                      `${asNumber(frame.payload_bytes ?? frame.payloadBytes) ?? 0} B`,
                      requestId,
                    ].filter(Boolean).join(' · ')}
                  </div>
                  {payload ? (
                    <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 p-2 text-[10px]">
                      {payload}
                    </pre>
                  ) : null}
                </div>
              );
            })}
          </div>
        </div>
      ) : null}
    </>
  );
};

const RouteInterceptionCards: React.FC<{ displayName: string; record: Record<string, unknown> }> = ({ displayName, record }) => {
  const { locale, t } = useI18n();
  if (!displayName.startsWith('browser_route_')) return null;
  const routes = asArray(record.routes)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);
  const addedRoute = asRecord(record.route);
  const visibleRoutes = addedRoute ? [addedRoute] : routes;
  return (
    <>
      <RowsCard
        title={translateToolTitle('Browser interception rules', locale)}
        rows={[
          { key: 'route count', value: asNumber(record.route_count ?? record.routeCount) },
          { key: 'cleared count', value: asNumber(record.cleared_count ?? record.clearedCount) },
          { key: 'scope', value: asString(record.scope).trim() },
          { key: 'ttl seconds', value: asNumber(record.ttl_seconds ?? record.ttlSeconds) },
          { key: 'approval required', value: asBoolean(record.approval_required ?? record.approvalRequired) },
        ]}
        fullWidth
      />
      {visibleRoutes.length > 0 ? (
        <div className="tool-detail-card tool-detail-card--full">
          {renderCardHeader(
            translateToolTitle('Browser interception rules', locale),
            formatToolCardCount(t, 'entries', visibleRoutes.length),
          )}
          <div className="tool-detail-list">
            {visibleRoutes.map((route, index) => (
              <div key={`browser-route-${asString(route.route_id ?? route.routeId).trim() || index}`} className="tool-detail-item">
                <div className="tool-detail-item-title break-all">
                  {asString(route.pattern).trim()}
                </div>
                <div className="tool-detail-item-meta break-all">
                  {[
                    asString(route.route_id ?? route.routeId).trim(),
                    asString(route.action).trim(),
                    `${asNumber(route.body_bytes ?? route.bodyBytes) ?? 0} B`,
                    asString(route.expires_at ?? route.expiresAt).trim(),
                  ].filter(Boolean).join(' · ')}
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </>
  );
};

const CdpDeveloperCard: React.FC<{ displayName: string; record: Record<string, unknown> }> = ({ displayName, record }) => {
  const { locale } = useI18n();
  if (displayName !== 'browser_cdp_command') return null;
  return (
    <>
      <RowsCard
        title={translateToolTitle('CDP developer command', locale)}
        rows={[
          { key: 'target', value: asString(record.target).trim() },
          { key: 'method', value: asString(record.method).trim() },
          { key: 'params bytes', value: asNumber(record.params_bytes ?? record.paramsBytes) },
          { key: 'params sha256', value: asString(record.params_sha256 ?? record.paramsSha256).trim() },
          { key: 'result bytes', value: asNumber(record.result_bytes ?? record.resultBytes) },
          { key: 'approval required', value: asBoolean(record.approval_required ?? record.approvalRequired) },
        ]}
        fullWidth
      />
      <TextBlockCard
        title={translateToolTitle('Result payload', locale)}
        content={stringifyJsonPreview(record.result).content}
      />
    </>
  );
};

interface BrowserToolDetailsProps {
  displayName: string;
  result: unknown;
}

export const BrowserToolDetails: React.FC<BrowserToolDetailsProps> = ({
  displayName,
  result,
}) => {
  const { locale, t } = useI18n();
  const record = asRecord(result);
  if (!record) return null;

  const images = asArray(record.images)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);
  const resultRecord = asRecord(record.result);
  const searchRecord = asRecord(record.search);
  const extractRecord = asRecord(record.extract);

  return (
    <div className="tool-detail-stack">
      <BrowserSessionCard record={record} />
      <PageStateCard record={record} />
      <BrowserTabsCard record={record} />
      <ConsolePreviewCards record={record} />
      {displayName === 'browser_network' ? <NetworkPreviewCards record={record} /> : null}
      {displayName === 'browser_network_request' ? <NetworkRequestDetailCards record={record} /> : null}
      <HarCaptureCards displayName={displayName} record={record} />
      <WebSocketCaptureCards displayName={displayName} record={record} />
      <RouteInterceptionCards displayName={displayName} record={record} />
      <CdpDeveloperCard displayName={displayName} record={record} />
      <FileTransferCards displayName={displayName} record={record} />

      {displayName === 'browser_console' && (
        <>
          <RowsCard
            title={translateToolTitle('JavaScript result', locale)}
            rows={[
              { key: 'preview', value: asString(record.result_preview ?? record.resultPreview).trim() },
            ]}
            fullWidth
          />
          {resultRecord && (() => {
            const preview = stringifyJsonPreview(resultRecord);
            return (
              <TextBlockCard
                title={translateToolTitle('Result payload', locale)}
                content={preview.content}
                meta={preview.meta}
              />
            );
          })()}
        </>
      )}

      {(displayName === 'browser_vision' || displayName === 'browser_inspect' || displayName === 'browser_research') && (
        <TextBlockCard title={translateToolTitle('Vision analysis', locale)} content={asString(record.analysis)} />
      )}

      {displayName === 'browser_get_images' && images.length > 0 && (
        <div className="tool-detail-card tool-detail-card--full">
          {renderCardHeader(
            translateToolTitle('Images', locale),
            formatToolCardCount(t, 'images', images.length),
          )}
          <div className="tool-detail-list">
            {images.map((item, index) => (
              <div key={`image-${index}`} className="tool-detail-item">
                <div className="tool-detail-item-title">
                  <a href={asString(item.src).trim()} target="_blank" rel="noreferrer" className="tool-detail-link">
                    {asString(item.alt).trim() || asString(item.src).trim() || `image ${index + 1}`}
                  </a>
                </div>
                <div className="tool-detail-item-meta">
                  {[asNumber(item.width), asNumber(item.height)].filter((value) => value !== null).join(' x ')}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      <TextBlockCard title={translateToolTitle('Inspection warning', locale)} content={asString(record.inspection_warning ?? record.inspectionWarning)} fullWidth={false} />
      <TextBlockCard title={translateToolTitle('Research warning', locale)} content={asString(record.research_warning ?? record.researchWarning)} fullWidth={false} />

        <StringListCard
        title={translateToolTitle('Selected URLs', locale)}
        values={asArray(record.selected_urls ?? record.selectedUrls).map((item) => asString(item))}
        linkify
        fullWidth
      />

      <SearchResultsBriefCard
        title={translateToolTitle('Search hits', locale)}
        items={asArray(searchRecord?.results_brief ?? searchRecord?.resultsBrief ?? record.results_brief ?? record.resultsBrief)}
      />

      <ExtractResultsBriefCard
        title={translateToolTitle('Extracted sources', locale)}
        items={asArray(extractRecord?.results_brief ?? extractRecord?.resultsBrief)}
      />
    </div>
  );
};

export default BrowserToolDetails;
