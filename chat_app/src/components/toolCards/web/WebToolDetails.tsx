// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { translateToolTitle } from '../../../i18n/toolText';
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
  const { locale } = useI18n();
  const record = asRecord(result);
  if (!record) return null;

  const searchRecord = asRecord(record.search);
  const extractRecord = asRecord(record.extract);

  return (
    <div className="tool-detail-stack">
      {(displayName === 'web_research' || displayName === 'web_extract') && (
        <StringListCard
          title={translateToolTitle('Selected URLs', locale)}
          values={asArray(record.selected_urls ?? record.selectedUrls).map((item) => asString(item))}
          linkify
          fullWidth
        />
      )}

      <SearchResultsBriefCard
        title={translateToolTitle('Search hits', locale)}
        items={asArray(searchRecord?.results_brief ?? searchRecord?.resultsBrief ?? record.results_brief ?? record.resultsBrief)}
      />

      <ExtractResultsBriefCard
        title={translateToolTitle('Extracted sources', locale)}
        items={asArray(extractRecord?.results_brief ?? extractRecord?.resultsBrief ?? record.results_brief ?? record.resultsBrief)}
      />
    </div>
  );
};

export default WebToolDetails;
