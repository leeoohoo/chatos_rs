// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { translateToolTitle } from '../../../i18n/toolText';
import { RowsCard, StringListCard, TextBlockCard } from '../shared/primitives';
import { asArray, asBoolean, asNumber, asRecord, asString } from '../shared/value';

interface ChangeOperationDetailsProps {
  result: unknown;
}

export const ChangeOperationDetails: React.FC<ChangeOperationDetailsProps> = ({
  result,
}) => {
  const { locale } = useI18n();
  const record = asRecord(result);
  if (!record) return null;

  const operationResult = asRecord(record.result);
  const pendingPaths = asArray(operationResult?.pending_paths)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);
  const committedPaths = asArray(operationResult?.committed_paths)
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);
  const structuredPaths = [...pendingPaths, ...committedPaths]
    .map((item) => asString(item.path).trim())
    .filter(Boolean);
  const batchChangedPaths = asArray(operationResult?.batch_changed_paths)
    .map((item) => asString(item).trim())
    .filter(Boolean);
  const touchedFiles = Array.from(new Set([...structuredPaths, ...batchChangedPaths]));
  const diffPreview = committedPaths
    .map((item) => {
      const path = asString(item.path).trim();
      const change = asRecord(item.change);
      const diff = asString(change?.diff).trim();
      return diff ? `${path ? `${path}\n` : ''}${diff}` : '';
    })
    .filter(Boolean)
    .join('\n\n');

  return (
    <>
      <RowsCard
        title={translateToolTitle('Session summary', locale)}
        rows={[
          { key: 'session id', value: asString(operationResult?.session_id) },
          { key: 'batch operations', value: asNumber(operationResult?.batch_operation_count) },
          { key: 'staged operations', value: asNumber(operationResult?.staged_operation_count) },
          { key: 'pending targets', value: asNumber(operationResult?.pending_target_count) },
          { key: 'discarded targets', value: asNumber(operationResult?.discarded_target_count) },
          { key: 'session closed', value: asBoolean(operationResult?.session_closed) },
        ]}
        fullWidth
      />
      <StringListCard title={translateToolTitle('Touched files', locale)} values={touchedFiles} fullWidth />
      <TextBlockCard title={translateToolTitle('Diff preview', locale)} content={diffPreview} />
      <TextBlockCard title={translateToolTitle('Message', locale)} content={asString(record.message)} />
      <TextBlockCard title={translateToolTitle('Hint', locale)} content={asString(record.hint)} fullWidth={false} />
    </>
  );
};

export default ChangeOperationDetails;
