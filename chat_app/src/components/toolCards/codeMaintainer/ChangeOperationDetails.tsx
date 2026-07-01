// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { translateToolTitle } from '../../../i18n/toolText';
import { StringListCard, TextBlockCard } from '../shared/primitives';
import { asArray, asRecord, asString } from '../shared/value';

interface ChangeOperationDetailsProps {
  displayName: string;
  result: unknown;
}

export const ChangeOperationDetails: React.FC<ChangeOperationDetailsProps> = ({
  displayName,
  result,
}) => {
  const { locale } = useI18n();
  const record = asRecord(result);
  if (!record) return null;

  const operationResult = asRecord(record.result);
  const change = asRecord(record.change);
  const files = asArray(record.files)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  const explicitPaths = files
    .map((item) => asString(item.path).trim())
    .filter(Boolean);

  const fallbackPaths = [
    asString(change?.path).trim(),
    asString(operationResult?.path).trim(),
  ].filter(Boolean);

  const touchedFiles = Array.from(new Set(
    (explicitPaths.length > 0 ? explicitPaths : fallbackPaths),
  ));

  const isPatchTool = displayName === 'apply_patch' || displayName === 'patch';

  return (
    <>
      {!isPatchTool && <StringListCard title={translateToolTitle('Touched files', locale)} values={touchedFiles} fullWidth />}
      <TextBlockCard title={translateToolTitle('Diff preview', locale)} content={asString(change?.diff)} />
      <TextBlockCard title={translateToolTitle('Message', locale)} content={asString(record.message)} />
      <TextBlockCard title={translateToolTitle('Hint', locale)} content={asString(record.hint)} fullWidth={false} />
    </>
  );
};

export default ChangeOperationDetails;
