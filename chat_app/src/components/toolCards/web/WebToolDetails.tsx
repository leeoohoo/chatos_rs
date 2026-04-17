import React from 'react';

import { ExtractResultsBriefCard, SearchResultsBriefCard } from '../shared/researchCards';
import { StringListCard } from '../shared/primitives';
import { asArray, asRecord, asString } from '../shared/value';

interface WebToolDetailsProps {
  displayName: string;
  result: unknown;
}

export const WebToolDetails: React.FC<WebToolDetailsProps> = ({
  displayName,
  result,
}) => {
  const record = asRecord(result);
  if (!record) return null;

  const searchRecord = asRecord(record.search);
  const extractRecord = asRecord(record.extract);

  return (
    <div className="tool-detail-stack">
      {(displayName === 'web_research' || displayName === 'web_extract') && (
        <StringListCard
          title="Selected URLs"
          values={asArray(record.selected_urls ?? record.selectedUrls).map((item) => asString(item))}
          linkify
          fullWidth
        />
      )}

      <SearchResultsBriefCard
        title="Search hits"
        items={asArray(searchRecord?.results_brief ?? searchRecord?.resultsBrief ?? record.results_brief ?? record.resultsBrief)}
      />

      <ExtractResultsBriefCard
        title="Extracted sources"
        items={asArray(extractRecord?.results_brief ?? extractRecord?.resultsBrief ?? record.results_brief ?? record.resultsBrief)}
      />
    </div>
  );
};

export default WebToolDetails;

