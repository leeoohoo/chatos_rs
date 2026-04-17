import React from 'react';

import { ExtractResultsBriefCard, SearchResultsBriefCard } from '../shared/researchCards';
import { RowsCard, StringListCard, TextBlockCard, renderCardHeader } from '../shared/primitives';
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
  const title = asString(record.title).trim();
  const rawUrl = asString(record.url).trim();
  const url = isMeaningfulBrowserUrl(rawUrl) ? rawUrl : '';
  const warning = asString(record.page_state_warning ?? record.pageStateWarning).trim();
  const pageStateAvailable = asBoolean(record.page_state_available ?? record.pageStateAvailable);
  const state = !title && !url && pageStateAvailable === false ? '未打开页面' : '';

  if (!title && !url && !warning && !state) return null;

  return (
    <RowsCard
      title="Page state"
      rows={[
        { key: 'state', value: state },
        { key: 'title', value: title },
        { key: 'url', value: url },
        { key: 'warning', value: warning },
      ]}
    />
  );
};

const ConsolePreviewCards: React.FC<{ record: Record<string, unknown> }> = ({ record }) => {
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
          {renderCardHeader('Console messages', `${messages.length} 条`)}
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
          {renderCardHeader('JavaScript errors', `${errors.length} 条`)}
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

interface BrowserToolDetailsProps {
  displayName: string;
  result: unknown;
}

export const BrowserToolDetails: React.FC<BrowserToolDetailsProps> = ({
  displayName,
  result,
}) => {
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
      <PageStateCard record={record} />
      <ConsolePreviewCards record={record} />

      {displayName === 'browser_console' && (
        <>
          <RowsCard
            title="JavaScript result"
            rows={[
              { key: 'preview', value: asString(record.result_preview ?? record.resultPreview).trim() },
            ]}
            fullWidth
          />
          {resultRecord && (
            <TextBlockCard title="Result payload" content={JSON.stringify(resultRecord, null, 2)} />
          )}
        </>
      )}

      {(displayName === 'browser_vision' || displayName === 'browser_inspect' || displayName === 'browser_research') && (
        <TextBlockCard title="Vision analysis" content={asString(record.analysis)} />
      )}

      {displayName === 'browser_get_images' && images.length > 0 && (
        <div className="tool-detail-card tool-detail-card--full">
          {renderCardHeader('Images', `${images.length} 张`)}
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

      <TextBlockCard title="Inspection warning" content={asString(record.inspection_warning ?? record.inspectionWarning)} fullWidth={false} />
      <TextBlockCard title="Research warning" content={asString(record.research_warning ?? record.researchWarning)} fullWidth={false} />

      <StringListCard
        title="Selected URLs"
        values={asArray(record.selected_urls ?? record.selectedUrls).map((item) => asString(item))}
        linkify
        fullWidth
      />

      <SearchResultsBriefCard
        title="Search hits"
        items={asArray(searchRecord?.results_brief ?? searchRecord?.resultsBrief ?? record.results_brief ?? record.resultsBrief)}
      />

      <ExtractResultsBriefCard
        title="Extracted sources"
        items={asArray(extractRecord?.results_brief ?? extractRecord?.resultsBrief)}
      />
    </div>
  );
};

export default BrowserToolDetails;
