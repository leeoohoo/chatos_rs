import React from 'react';

import GenericStructuredResultDetails from '../shared/GenericStructuredResultDetails';
import { RowsCard, StringListCard } from '../shared/primitives';
import { asArray, asRecord, asString } from '../shared/value';

interface UiPrompterToolDetailsProps {
  displayName: string;
  result: unknown;
}

const formatLabel = (value: string): string => (
  value
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[_-]+/g, ' ')
    .trim()
    .toLowerCase()
);

const FormValuesCard: React.FC<{ value: unknown }> = ({ value }) => {
  const valuesRecord = asRecord(value);
  if (!valuesRecord) {
    const text = asString(value).trim();
    return text ? (
      <RowsCard
        title="Form values"
        rows={[{ key: 'value', value: text }]}
        fullWidth
      />
    ) : null;
  }

  const primitiveRows: Array<{ key: string; value: string | number | boolean }> = [];
  const complexEntries: Record<string, unknown> = {};

  Object.entries(valuesRecord).forEach(([key, entryValue]) => {
    if (typeof entryValue === 'string') {
      const trimmed = entryValue.trim();
      if (trimmed) {
        primitiveRows.push({ key: formatLabel(key), value: trimmed });
      }
      return;
    }

    if (typeof entryValue === 'number' || typeof entryValue === 'boolean') {
      primitiveRows.push({ key: formatLabel(key), value: entryValue });
      return;
    }

    complexEntries[key] = entryValue;
  });

  return (
    <>
      {primitiveRows.length > 0 && (
        <RowsCard title="Form values" rows={primitiveRows} fullWidth />
      )}
      {Object.keys(complexEntries).length > 0 && (
        <GenericStructuredResultDetails value={complexEntries} />
      )}
    </>
  );
};

export const UiPrompterToolDetails: React.FC<UiPrompterToolDetailsProps> = ({
  displayName,
  result,
}) => {
  const record = asRecord(result);
  if (!record) return null;

  const selectionValue = record.selection;
  const selectionStrings = Array.isArray(selectionValue)
    ? asArray(selectionValue).map((item) => asString(item)).filter(Boolean)
    : [];
  const resultTitle = displayName === 'prompt_choices'
    ? 'Choice result'
    : displayName === 'prompt_mixed_form'
      ? 'Mixed form result'
      : 'Form result';
  const selectionTitle = displayName === 'prompt_choices'
    ? (selectionStrings.length > 1 ? 'Chosen options' : 'Chosen option')
    : 'Selection';

  return (
    <div className="tool-detail-stack">
      <RowsCard
        title={resultTitle}
        rows={[
          { key: 'status', value: asString(record.status).trim() },
        ]}
      />
      {record.values !== null && record.values !== undefined ? (
        <FormValuesCard value={record.values} />
      ) : null}
      {selectionStrings.length > 0 && (
        <StringListCard title={selectionTitle} values={selectionStrings} fullWidth />
      )}
      {typeof selectionValue === 'string' && selectionValue.trim() && (
        <RowsCard title={selectionTitle} rows={[{ key: 'value', value: selectionValue.trim() }]} fullWidth />
      )}
    </div>
  );
};

export default UiPrompterToolDetails;
