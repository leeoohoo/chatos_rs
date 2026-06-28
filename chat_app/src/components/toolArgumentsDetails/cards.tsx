import {
  asRecord,
  asString,
  formatLabel,
  formatPrimitive,
  isPrimitive,
  truncateText,
} from './valueUtils';
import { UI_MESSAGES, type UiLocale } from '../../i18n/messages';
import { formatToolDetailText, stringifyJsonPreview } from '../toolDetails/textPreview';

const renderCardHeader = (title: string, meta?: string) => (
  <div className="tool-card-header">
    <div className="tool-detail-title">{title}</div>
    {meta && <span className="tool-card-badge">{meta}</span>}
  </div>
);

const formatMessage = (
  template: string,
  params: Record<string, string | number>,
) => template.replace(/\{(\w+)\}/g, (_match, key: string) => (
  Object.prototype.hasOwnProperty.call(params, key) ? String(params[key]) : `{${key}}`
));

const formatCount = (locale: UiLocale, key: string, count: number): string => {
  const dictionary = UI_MESSAGES[locale] || UI_MESSAGES['zh-CN'];
  const fallback = UI_MESSAGES['zh-CN'];
  return formatMessage(dictionary[key] || fallback[key] || '{count}', { count });
};

export const renderRowsCard = (
  title: string,
  rows: Array<{ key: string; value: string }>,
  fullWidth: boolean = false,
  locale: UiLocale = 'zh-CN',
) => {
  const filtered = rows.filter((row) => row.value.trim().length > 0);
  if (filtered.length === 0) return null;

  return (
    <div className={`tool-detail-card${fullWidth ? ' tool-detail-card--full' : ''}`}>
      {renderCardHeader(title, formatCount(locale, 'toolCard.count.rows', filtered.length))}
      <div className="tool-detail-rows">
        {filtered.map((row) => (
          <div key={`${title}-${row.key}`} className="tool-detail-row">
            <span className="tool-detail-key">{row.key}</span>
            <span className="tool-detail-value">{row.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
};

export const renderTextBlock = (
  title: string,
  content: string,
  fullWidth: boolean = true,
  meta?: string,
) => {
  const preview = formatToolDetailText(content);
  if (!preview.content) return null;

  return (
    <div className={`tool-detail-card${fullWidth ? ' tool-detail-card--full' : ''}`}>
      {renderCardHeader(title, meta || preview.meta)}
      <pre className="tool-detail-code">{preview.content}</pre>
    </div>
  );
};

export const renderStringListCard = (
  title: string,
  values: string[],
  linkify: boolean = false,
  fullWidth: boolean = false,
  locale: UiLocale = 'zh-CN',
) => {
  const filtered = values.map((item) => item.trim()).filter(Boolean);
  if (filtered.length === 0) return null;

  return (
    <div className={`tool-detail-card${fullWidth ? ' tool-detail-card--full' : ''}`}>
      {renderCardHeader(title, formatCount(locale, 'toolCard.count.items', filtered.length))}
      <div className="tool-detail-list">
        {filtered.map((item, index) => (
          <div key={`${title}-${index}`} className="tool-detail-item">
            {linkify ? (
              <a href={item} target="_blank" rel="noreferrer" className="tool-detail-link">
                {item}
              </a>
            ) : (
              <div className="tool-detail-item-body">{item}</div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
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
  ];

  for (const candidate of candidates) {
    const text = asString(candidate).trim();
    if (text) {
      return truncateText(text);
    }
  }

  const compactRecord = Object.fromEntries(
    Object.entries(record).filter(([key]) => !['title', 'name', 'path', 'url'].includes(key)),
  );

  try {
    const serialized = stringifyJsonPreview(compactRecord, 640).content;
    return serialized === '{}' ? '' : truncateText(serialized, 320);
  } catch {
    return '';
  }
};

export const renderObjectListCard = (
  title: string,
  values: unknown[],
  locale: UiLocale = 'zh-CN',
) => {
  const items = values
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (items.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader(title, formatCount(locale, 'toolCard.count.items', items.length))}
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

export const renderObjectCard = (title: string, value: Record<string, unknown>) => {
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
    return renderRowsCard(title, primitiveRows, true);
  }

  const preview = stringifyJsonPreview(value);
  return renderTextBlock(title, preview.content, true, preview.meta);
};
