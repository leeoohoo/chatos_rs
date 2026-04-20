import React from 'react';

import { TextBlockCard } from '../shared/primitives';
import { asNumber, asRecord, asString, buildLineRangeLabel } from '../shared/value';

interface ReadFileDetailsProps {
  result: unknown;
}

export const ReadFileDetails: React.FC<ReadFileDetailsProps> = ({ result }) => {
  const record = asRecord(result);
  if (!record) return null;

  const content = asString(record.content).trim();
  const startLine = asNumber(record.start_line ?? record.startLine);
  const endLine = asNumber(record.end_line ?? record.endLine);

  return (
    <TextBlockCard
      title="File content"
      content={content}
      meta={buildLineRangeLabel(startLine, endLine) || undefined}
    />
  );
};

export default ReadFileDetails;

