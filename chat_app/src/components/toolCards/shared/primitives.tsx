import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import {
  formatToolPrimitive,
  translateToolTitle,
} from '../../../i18n/toolText';

export const renderCardHeader = (title: string, meta?: string) => (
  <div className="tool-card-header">
    <div className="tool-detail-title">{title}</div>
    {meta && <span className="tool-card-badge">{meta}</span>}
  </div>
);

interface TextBlockCardProps {
  title: string;
  content: string;
  fullWidth?: boolean;
  meta?: string;
}

export const TextBlockCard: React.FC<TextBlockCardProps> = ({
  title,
  content,
  fullWidth = true,
  meta,
}) => {
  const { locale } = useI18n();
  const trimmed = content.trim();
  if (!trimmed) return null;

  return (
    <div className={`tool-detail-card${fullWidth ? ' tool-detail-card--full' : ''}`}>
      {renderCardHeader(translateToolTitle(title, locale), meta)}
      <pre className="tool-detail-code">{trimmed}</pre>
    </div>
  );
};

interface RowsCardProps {
  title: string;
  rows: Array<{ key: string; value: string | number | boolean | null | undefined }>;
  fullWidth?: boolean;
}

export const RowsCard: React.FC<RowsCardProps> = ({
  title,
  rows,
  fullWidth = false,
}) => {
  const { locale } = useI18n();
  const formatValue = (value: string | number | boolean | null | undefined): string => {
    if (value === undefined) {
      return '';
    }
    if (typeof value === 'boolean') {
      return formatToolPrimitive(value, locale);
    }
    if (value === null) {
      return formatToolPrimitive(value, locale);
    }
    if (typeof value === 'string') {
      return formatToolPrimitive(value, locale);
    }
    return String(value);
  };

  const filtered = rows.filter((row) => (
    row.value !== null
    && row.value !== undefined
    && formatValue(row.value).trim() !== ''
  ));

  if (filtered.length === 0) return null;

  return (
    <div className={`tool-detail-card${fullWidth ? ' tool-detail-card--full' : ''}`}>
      {renderCardHeader(
        translateToolTitle(title, locale),
        locale === 'zh-CN' ? `${filtered.length} 项` : `${filtered.length} items`,
      )}
      <div className="tool-detail-rows">
        {filtered.map((row) => (
          <div key={`${title}-${row.key}`} className="tool-detail-row">
            <span className="tool-detail-key">{row.key}</span>
            <span className="tool-detail-value">{formatValue(row.value)}</span>
          </div>
        ))}
      </div>
    </div>
  );
};

interface StringListCardProps {
  title: string;
  values: string[];
  linkify?: boolean;
  fullWidth?: boolean;
}

export const StringListCard: React.FC<StringListCardProps> = ({
  title,
  values,
  linkify = false,
  fullWidth = false,
}) => {
  const { locale } = useI18n();
  const filtered = values.map((item) => item.trim()).filter(Boolean);
  if (filtered.length === 0) return null;

  return (
    <div className={`tool-detail-card${fullWidth ? ' tool-detail-card--full' : ''}`}>
      {renderCardHeader(
        translateToolTitle(title, locale),
        locale === 'zh-CN' ? `${filtered.length} 项` : `${filtered.length} items`,
      )}
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
