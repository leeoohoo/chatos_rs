import React from 'react';

import { renderCardHeader } from './primitives';
import { asRecord, asString } from './value';

interface ResultCardProps {
  title: string;
  items: unknown[];
}

export const SearchResultsBriefCard: React.FC<ResultCardProps> = ({
  title,
  items,
}) => {
  const cards = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null)
    .map((item, index) => {
      const entryTitle = asString(item.title).trim() || asString(item.url).trim() || `result ${index + 1}`;
      const url = asString(item.url).trim();
      const body = asString(item.description_preview ?? item.descriptionPreview).trim();
      if (!entryTitle && !url && !body) {
        return null;
      }

      return (
        <div key={`${title}-${index}`} className="tool-detail-item">
          <div className="tool-detail-item-title">
            {url ? (
              <a href={url} target="_blank" rel="noreferrer" className="tool-detail-link">
                {entryTitle}
              </a>
            ) : (
              entryTitle
            )}
          </div>
          {url && <div className="tool-detail-item-meta">{url}</div>}
          {body && <div className="tool-detail-item-body">{body}</div>}
        </div>
      );
    })
    .filter(Boolean) as React.ReactNode[];

  if (cards.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader(title, `${cards.length} 条`)}
      <div className="tool-detail-list">{cards}</div>
    </div>
  );
};

export const ExtractResultsBriefCard: React.FC<ResultCardProps> = ({
  title,
  items,
}) => {
  const cards = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null)
    .map((item, index) => {
      const entryTitle = asString(item.title).trim() || asString(item.url).trim() || `source ${index + 1}`;
      const url = asString(item.url).trim();
      const status = asString(item.status).trim();
      const body = asString(item.content_preview ?? item.contentPreview).trim();
      if (!entryTitle && !url && !status && !body) {
        return null;
      }

      return (
        <div key={`${title}-${index}`} className="tool-detail-item">
          <div className="tool-detail-item-title">
            {url ? (
              <a href={url} target="_blank" rel="noreferrer" className="tool-detail-link">
                {entryTitle}
              </a>
            ) : (
              entryTitle
            )}
          </div>
          {(url || status) && (
            <div className="tool-detail-item-meta">
              {[url, status].filter(Boolean).join(' · ')}
            </div>
          )}
          {body && <div className="tool-detail-item-body">{body}</div>}
        </div>
      );
    })
    .filter(Boolean) as React.ReactNode[];

  if (cards.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader(title, `${cards.length} 条`)}
      <div className="tool-detail-list">{cards}</div>
    </div>
  );
};

