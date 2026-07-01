// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { useI18n } from '../i18n/I18nProvider';
import { translateToolTitle } from '../i18n/toolText';
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
import { stringifyJsonPreview } from './toolDetails/textPreview';

interface ToolArgumentsDetailsProps {
  argumentsValue: unknown;
  rawToolName?: string;
}

const renderPatchInput = (
  argumentsValue: Record<string, unknown>,
  locale: 'zh-CN' | 'en-US',
) => {
  const patchText = asString(argumentsValue.patch).trim();
  if (!patchText) {
    return null;
  }

  return (
    <div className="tool-detail-stack">
      {renderTextBlock(translateToolTitle('Patch payload', locale), patchText)}
    </div>
  );
};

export const ToolArgumentsDetails: React.FC<ToolArgumentsDetailsProps> = ({
  argumentsValue,
  rawToolName,
}) => {
  const { locale } = useI18n();
  const displayName = rawToolName ? getToolDisplayName(rawToolName) : '';

  if (typeof argumentsValue === 'string') {
    return (
      <div className="tool-detail-stack">
        {renderTextBlock(translateToolTitle('Input payload', locale), argumentsValue)}
      </div>
    );
  }

  if (Array.isArray(argumentsValue)) {
    const primitiveValues = argumentsValue.filter((item) => isPrimitive(item));
    if (primitiveValues.length === argumentsValue.length) {
      return (
        <div className="tool-detail-stack">
          {renderStringListCard(
            translateToolTitle('Input items', locale),
            primitiveValues.map((item) => formatPrimitive(item, locale)),
            false,
            true,
            locale,
          )}
        </div>
      );
    }

    return (
      <div className="tool-detail-stack">
        {renderObjectListCard(translateToolTitle('Input items', locale), argumentsValue, locale)}
      </div>
    );
  }

  const record = asRecord(argumentsValue);
  if (!record) {
    return null;
  }

  if (displayName === 'apply_patch' || displayName === 'patch') {
    return renderPatchInput(record, locale);
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

      summaryRows.push({ key: label, value: formatPrimitive(value, locale) });
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
            locale,
          ),
        );
        return;
      }

      sections.push(renderObjectListCard(sectionTitle, value, locale));
      return;
    }

    const nestedRecord = asRecord(value);
    if (nestedRecord) {
      sections.push(renderObjectCard(sectionTitle, nestedRecord));
      return;
    }

    sections.push(renderTextBlock(sectionTitle, String(value)));
  });

  const summaryCard = renderRowsCard(
    translateToolTitle('Input summary', locale),
    summaryRows,
    false,
    locale,
  );
  const validSections = sections.filter(Boolean);

  if (!summaryCard && validSections.length === 0) {
    if (visibleEntryCount === 0) {
      return null;
    }

    const preview = stringifyJsonPreview(argumentsValue);
    return (
      <div className="tool-detail-stack">
        {renderTextBlock(translateToolTitle('Input payload', locale), preview.content, true, preview.meta)}
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
