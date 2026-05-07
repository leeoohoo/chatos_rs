import React from 'react';
import { getToolDisplayName } from '../lib/tools/displayName';
import { shouldHideArgumentKey } from './toolArgumentsDetails/argumentVisibility';
import {
  renderObjectCard,
  renderObjectListCard,
  renderRowsCard,
  renderStringListCard,
  renderTextBlock,
} from './toolArgumentsDetails/cards';
import {
  asRecord,
  asString,
  formatCardTitle,
  formatLabel,
  formatPrimitive,
  isPrimitive,
  isUrlLike,
  shouldRenderAsLongText,
} from './toolArgumentsDetails/valueUtils';

interface ToolArgumentsDetailsProps {
  argumentsValue: unknown;
  rawToolName?: string;
}

const renderPatchInput = (argumentsValue: Record<string, unknown>) => {
  const patchText = asString(argumentsValue.patch).trim();
  if (!patchText) {
    return null;
  }

  return (
    <div className="tool-detail-stack">
      {renderTextBlock('Patch payload', patchText)}
    </div>
  );
};

export const ToolArgumentsDetails: React.FC<ToolArgumentsDetailsProps> = ({
  argumentsValue,
  rawToolName,
}) => {
  const displayName = rawToolName ? getToolDisplayName(rawToolName) : '';

  if (typeof argumentsValue === 'string') {
    return (
      <div className="tool-detail-stack">
        {renderTextBlock('Input payload', argumentsValue)}
      </div>
    );
  }

  if (Array.isArray(argumentsValue)) {
    const primitiveValues = argumentsValue.filter((item) => isPrimitive(item));
    if (primitiveValues.length === argumentsValue.length) {
      return (
        <div className="tool-detail-stack">
          {renderStringListCard(
            'Input items',
            primitiveValues.map((item) => formatPrimitive(item)),
            false,
            true,
          )}
        </div>
      );
    }

    return (
      <div className="tool-detail-stack">
        {renderObjectListCard('Input items', argumentsValue)}
      </div>
    );
  }

  const record = asRecord(argumentsValue);
  if (!record) {
    return null;
  }

  if (displayName === 'apply_patch' || displayName === 'patch') {
    return renderPatchInput(record);
  }

  const summaryRows: Array<{ key: string; value: string }> = [];
  const sections: React.ReactNode[] = [];
  let visibleEntryCount = 0;

  Object.entries(record).forEach(([key, value]) => {
    if (shouldHideArgumentKey(rawToolName, key)) {
      return;
    }

    visibleEntryCount += 1;

    const label = formatLabel(key);
    const sectionTitle = formatCardTitle(key);

    if (isPrimitive(value)) {
      if (typeof value === 'string') {
        const trimmed = value.trim();
        if (!trimmed) {
          return;
        }
        if (shouldRenderAsLongText(key, trimmed)) {
          sections.push(renderTextBlock(sectionTitle, trimmed));
          return;
        }
        summaryRows.push({ key: label, value: trimmed });
        return;
      }

      summaryRows.push({ key: label, value: formatPrimitive(value) });
      return;
    }

    if (Array.isArray(value)) {
      const stringValues = value
        .filter((item): item is string => typeof item === 'string')
        .map((item) => item.trim())
        .filter(Boolean);

      if (stringValues.length === value.length) {
        sections.push(
          renderStringListCard(
            sectionTitle,
            stringValues,
            stringValues.every((item) => isUrlLike(item)),
            true,
          ),
        );
        return;
      }

      sections.push(renderObjectListCard(sectionTitle, value));
      return;
    }

    const nestedRecord = asRecord(value);
    if (nestedRecord) {
      sections.push(renderObjectCard(sectionTitle, nestedRecord));
      return;
    }

    sections.push(renderTextBlock(sectionTitle, String(value)));
  });

  const summaryCard = renderRowsCard('Input summary', summaryRows);
  const validSections = sections.filter(Boolean);

  if (!summaryCard && validSections.length === 0) {
    if (visibleEntryCount === 0) {
      return null;
    }

    return (
      <div className="tool-detail-stack">
        {renderTextBlock('Input payload', JSON.stringify(argumentsValue, null, 2))}
      </div>
    );
  }

  return (
    <div className="tool-detail-stack">
      {summaryCard}
      {validSections}
    </div>
  );
};

export default ToolArgumentsDetails;
