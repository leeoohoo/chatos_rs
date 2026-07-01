// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { formatToolLineRangeLabel, translateToolTitle } from '../../../i18n/toolText';
import { TextBlockCard } from '../shared/primitives';
import { asNumber, asRecord, asString } from '../shared/value';

interface ReadFileDetailsProps {
  result: unknown;
}

export const ReadFileDetails: React.FC<ReadFileDetailsProps> = ({ result }) => {
  const { locale } = useI18n();
  const record = asRecord(result);
  if (!record) return null;

  const content = asString(record.content).trim();
  const startLine = asNumber(record.start_line ?? record.startLine);
  const endLine = asNumber(record.end_line ?? record.endLine);

  return (
    <TextBlockCard
      title={translateToolTitle('File content', locale)}
      content={content}
      meta={formatToolLineRangeLabel(startLine, endLine, locale) || undefined}
    />
  );
};

export default ReadFileDetails;
