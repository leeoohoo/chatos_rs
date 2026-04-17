import React from 'react';

import { renderCardHeader } from '../shared/primitives';
import { asArray, asNumber, asRecord, asString } from '../shared/value';

interface SearchMatchesDetailsProps {
  result: unknown;
}

export const SearchMatchesDetails: React.FC<SearchMatchesDetailsProps> = ({ result }) => {
  const record = asRecord(result);
  if (!record) return null;

  const matches = asArray(record.results)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (matches.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader('Matches', `${matches.length} 条`)}
      <div className="tool-detail-list">
        {matches.map((item, index) => (
          <div key={`search-match-${index}`} className="tool-detail-item">
            <div className="tool-detail-item-title">
              {asString(item.path).trim() || `match ${index + 1}`}
            </div>
            <div className="tool-detail-item-meta">
              line {asNumber(item.line) ?? '?'}
            </div>
            <div className="tool-detail-item-body">{asString(item.text).trim()}</div>
          </div>
        ))}
      </div>
    </div>
  );
};

export default SearchMatchesDetails;

