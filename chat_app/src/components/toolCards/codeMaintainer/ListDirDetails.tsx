import React from 'react';

import { renderCardHeader } from '../shared/primitives';
import { asArray, asNumber, asRecord, asString, formatDateTime } from '../shared/value';

interface ListDirDetailsProps {
  result: unknown;
}

export const ListDirDetails: React.FC<ListDirDetailsProps> = ({ result }) => {
  const record = asRecord(result);
  if (!record) return null;

  const entries = asArray(record.entries)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (entries.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader('Directory entries', `${entries.length} 项`)}
      <div className="tool-detail-list">
        {entries.map((entry, index) => {
          const name = asString(entry.name).trim() || `entry ${index + 1}`;
          const path = asString(entry.path).trim();
          const type = asString(entry.type).trim();
          const size = asNumber(entry.size);
          const modified = formatDateTime(entry.mtime_ms ?? entry.mtimeMs);

          return (
            <div key={`dir-entry-${index}`} className="tool-detail-item">
              <div className="tool-detail-item-title">{name}</div>
              <div className="tool-detail-item-meta">
                {[path, type, size !== null ? `${size} B` : '', modified].filter(Boolean).join(' · ')}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

export default ListDirDetails;

