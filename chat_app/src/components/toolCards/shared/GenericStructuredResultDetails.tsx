import React from 'react';

import { asRecord, asString } from './value';
import { RowsCard, StringListCard, TextBlockCard, renderCardHeader } from './primitives';

const isPrimitive = (value: unknown): value is string | number | boolean | null => (
  value === null
  || typeof value === 'string'
  || typeof value === 'number'
  || typeof value === 'boolean'
);

const formatLabel = (value: string): string => (
  value
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[_-]+/g, ' ')
    .trim()
    .toLowerCase()
);

const TITLE_CASE_OVERRIDES: Record<string, string> = {
  api: 'API',
  html: 'HTML',
  id: 'ID',
  js: 'JS',
  json: 'JSON',
  url: 'URL',
  urls: 'URLs',
};

const formatCardTitle = (value: string): string => (
  formatLabel(value)
    .split(' ')
    .filter(Boolean)
    .map((part) => TITLE_CASE_OVERRIDES[part] || `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
    .join(' ')
);

const formatPrimitive = (value: string | number | boolean | null): string => {
  if (typeof value === 'boolean') {
    return value ? 'yes' : 'no';
  }
  if (value === null) {
    return 'null';
  }
  return String(value);
};

const isUrlLike = (value: string): boolean => /^https?:\/\//i.test(value.trim());

const shouldRenderAsLongText = (key: string, value: string): boolean => (
  value.includes('\n')
  || value.length > 160
  || /(content|text|prompt|script|code|patch|diff|html|markdown|body|message|instruction|analysis|query|output|stderr|stdout)/i.test(key)
);

const truncateText = (value: string, maxLength: number = 240): string => {
  const trimmed = value.trim();
  if (trimmed.length <= maxLength) {
    return trimmed;
  }
  return `${trimmed.slice(0, maxLength - 1)}...`;
};

const buildObjectItemMeta = (record: Record<string, unknown>): string => {
  const segments: string[] = [];
  const url = asString(record.url).trim();
  const path = asString(record.path).trim();
  const type = asString(record.type).trim();
  const status = asString(record.status).trim();
  const line = record.line;

  if (url) segments.push(url);
  if (path) segments.push(path);
  if (type) segments.push(type);
  if (status) segments.push(status);
  if (typeof line === 'number') segments.push(`line ${line}`);

  return segments.join(' · ');
};

const buildObjectItemBody = (record: Record<string, unknown>): string => {
  const candidates = [
    record.description,
    record.description_preview,
    record.descriptionPreview,
    record.text,
    record.text_preview,
    record.textPreview,
    record.value,
    record.content,
    record.content_preview,
    record.contentPreview,
    record.selector,
    record.query,
    record.output,
    record.stderr,
    record.stdout,
  ];

  for (const candidate of candidates) {
    const text = asString(candidate).trim();
    if (text) {
      return truncateText(text, 320);
    }
  }

  const compactRecord = Object.fromEntries(
    Object.entries(record).filter(([key]) => !['title', 'name', 'path', 'url'].includes(key)),
  );

  try {
    const serialized = JSON.stringify(compactRecord, null, 2);
    return serialized === '{}' ? '' : truncateText(serialized, 320);
  } catch {
    return '';
  }
};

const ObjectListCard: React.FC<{ title: string; values: unknown[] }> = ({
  title,
  values,
}) => {
  const items = values
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (items.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader(title, `${items.length} 条`)}
      <div className="tool-detail-list">
        {items.map((item, index) => {
          const itemTitle = (
            asString(item.title).trim()
            || asString(item.name).trim()
            || asString(item.path).trim()
            || asString(item.url).trim()
            || asString(item.selector).trim()
            || asString(item.id).trim()
            || `${title} ${index + 1}`
          );

          const meta = buildObjectItemMeta(item);
          const body = buildObjectItemBody(item);

          return (
            <div key={`${title}-${index}`} className="tool-detail-item">
              <div className="tool-detail-item-title">{itemTitle}</div>
              {meta && <div className="tool-detail-item-meta">{meta}</div>}
              {body && <div className="tool-detail-item-body">{body}</div>}
            </div>
          );
        })}
      </div>
    </div>
  );
};

const ObjectCard: React.FC<{ title: string; value: Record<string, unknown> }> = ({
  title,
  value,
}) => {
  const entries = Object.entries(value);
  if (entries.length === 0) return null;

  const primitiveRows = entries.flatMap(([key, entryValue]) => (
    isPrimitive(entryValue)
      ? [{
        key: formatLabel(key),
        value: formatPrimitive(entryValue),
      }]
      : []
  ));

  if (primitiveRows.length === entries.length) {
    return <RowsCard title={title} rows={primitiveRows} fullWidth />;
  }

  return <TextBlockCard title={title} content={JSON.stringify(value, null, 2)} />;
};

interface GenericStructuredResultDetailsProps {
  value: unknown;
}

export const GenericStructuredResultDetails: React.FC<GenericStructuredResultDetailsProps> = ({
  value,
}) => {
  if (typeof value === 'string') {
    return (
      <div className="tool-detail-stack">
        <TextBlockCard title="Result payload" content={value} />
      </div>
    );
  }

  if (Array.isArray(value)) {
    const primitiveValues = value.filter((item) => isPrimitive(item));
    if (primitiveValues.length === value.length) {
      return (
        <div className="tool-detail-stack">
          <StringListCard
            title="Result items"
            values={primitiveValues.map((item) => formatPrimitive(item))}
            fullWidth
          />
        </div>
      );
    }

    return (
      <div className="tool-detail-stack">
        <ObjectListCard title="Result items" values={value} />
      </div>
    );
  }

  const record = asRecord(value);
  if (!record) {
    return null;
  }

  const summaryRows: Array<{ key: string; value: string }> = [];
  const sections: React.ReactNode[] = [];

  Object.entries(record).forEach(([key, entryValue]) => {
    const label = formatLabel(key);
    const sectionTitle = formatCardTitle(key);

    if (isPrimitive(entryValue)) {
      if (typeof entryValue === 'string') {
        const trimmed = entryValue.trim();
        if (!trimmed) {
          return;
        }
        if (shouldRenderAsLongText(key, trimmed)) {
          sections.push(<TextBlockCard key={key} title={sectionTitle} content={trimmed} />);
          return;
        }
        summaryRows.push({ key: label, value: trimmed });
        return;
      }

      summaryRows.push({ key: label, value: formatPrimitive(entryValue) });
      return;
    }

    if (Array.isArray(entryValue)) {
      const stringValues = entryValue
        .filter((item): item is string => typeof item === 'string')
        .map((item) => item.trim())
        .filter(Boolean);

      if (stringValues.length === entryValue.length) {
        sections.push(
          <StringListCard
            key={key}
            title={sectionTitle}
            values={stringValues}
            linkify={stringValues.every((item) => isUrlLike(item))}
            fullWidth
          />,
        );
        return;
      }

      sections.push(<ObjectListCard key={key} title={sectionTitle} values={entryValue} />);
      return;
    }

    const nestedRecord = asRecord(entryValue);
    if (nestedRecord) {
      sections.push(<ObjectCard key={key} title={sectionTitle} value={nestedRecord} />);
      return;
    }

    sections.push(<TextBlockCard key={key} title={sectionTitle} content={String(entryValue)} />);
  });

  const summaryCard = summaryRows.length > 0
    ? <RowsCard title="Result summary" rows={summaryRows} />
    : null;

  return (
    <div className="tool-detail-stack">
      {summaryCard}
      {sections.filter(Boolean)}
    </div>
  );
};

export default GenericStructuredResultDetails;
