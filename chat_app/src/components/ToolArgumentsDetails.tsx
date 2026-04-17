import React from 'react';
import { resolveToolRoutingKey } from '../lib/tools/catalog';
import { getToolDisplayName } from '../lib/tools/displayName';

const asRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

const asString = (value: unknown): string => (
  typeof value === 'string' ? value : ''
);

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
  || /(content|text|prompt|script|code|patch|diff|html|markdown|body|message|instruction|analysis|query)/i.test(key)
);

const truncateText = (value: string, maxLength: number = 240): string => {
  const trimmed = value.trim();
  if (trimmed.length <= maxLength) {
    return trimmed;
  }
  return `${trimmed.slice(0, maxLength - 1)}...`;
};

const renderCardHeader = (title: string, meta?: string) => (
  <div className="tool-card-header">
    <div className="tool-detail-title">{title}</div>
    {meta && <span className="tool-card-badge">{meta}</span>}
  </div>
);

const renderRowsCard = (
  title: string,
  rows: Array<{ key: string; value: string }>,
  fullWidth: boolean = false,
) => {
  const filtered = rows.filter((row) => row.value.trim().length > 0);
  if (filtered.length === 0) return null;

  return (
    <div className={`tool-detail-card${fullWidth ? ' tool-detail-card--full' : ''}`}>
      {renderCardHeader(title, `${filtered.length} 项`)}
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

const renderTextBlock = (title: string, content: string, fullWidth: boolean = true) => {
  const trimmed = content.trim();
  if (!trimmed) return null;

  return (
    <div className={`tool-detail-card${fullWidth ? ' tool-detail-card--full' : ''}`}>
      {renderCardHeader(title)}
      <pre className="tool-detail-code">{trimmed}</pre>
    </div>
  );
};

const renderStringListCard = (
  title: string,
  values: string[],
  linkify: boolean = false,
  fullWidth: boolean = false,
) => {
  const filtered = values.map((item) => item.trim()).filter(Boolean);
  if (filtered.length === 0) return null;

  return (
    <div className={`tool-detail-card${fullWidth ? ' tool-detail-card--full' : ''}`}>
      {renderCardHeader(title, `${filtered.length} 项`)}
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
    const serialized = JSON.stringify(compactRecord, null, 2);
    return serialized === '{}' ? '' : truncateText(serialized, 320);
  } catch {
    return '';
  }
};

const renderObjectListCard = (title: string, values: unknown[]) => {
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

const renderObjectCard = (title: string, value: Record<string, unknown>) => {
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

  return renderTextBlock(title, JSON.stringify(value, null, 2));
};

interface ToolArgumentsDetailsProps {
  argumentsValue: unknown;
  rawToolName?: string;
}

const GENERIC_HIDDEN_ARGUMENT_KEYS = new Set([
  'annotation_count',
  'annotationCount',
  'clear_applied',
  'clearApplied',
  'debug',
  'fallback',
  'fallback_used',
  'fallbackUsed',
  'include_metadata',
  'includeMetadata',
  'include_raw',
  'includeRaw',
  'max_results',
  'maxResults',
  'model',
  'process_id',
  'processId',
  'prompt_source',
  'promptSource',
  'provider',
  'terminal_id',
  'terminalId',
  'timeout',
  'timeout_ms',
  'timeoutMs',
  'transport',
  'with_line_numbers',
  'withLineNumbers',
]);

const TOOL_ARGUMENT_ALLOWLIST: Record<string, Set<string>> = {
  'code:read_file': new Set(['path', 'start_line', 'startLine', 'end_line', 'endLine']),
  'code:read_file_raw': new Set(['path', 'start_line', 'startLine', 'end_line', 'endLine']),
  'code:read_file_range': new Set(['path', 'start_line', 'startLine', 'end_line', 'endLine']),
  'code:search_text': new Set(['path', 'pattern', 'query', 'text', 'directory', 'dir']),
  'code:search_files': new Set(['path', 'pattern', 'query', 'glob', 'directory', 'dir']),
  'code:list_dir': new Set(['path', 'directory', 'dir']),
  'code:write_file': new Set(['path']),
  'code:edit_file': new Set(['path']),
  'code:append_file': new Set(['path']),
  'code:delete_path': new Set(['path']),
  'code:apply_patch': new Set([]),
  'code:patch': new Set([]),
  'browser:browser_open': new Set(['url']),
  'browser:browser_snapshot': new Set(['full']),
  'browser:browser_click': new Set(['ref']),
  'browser:browser_type': new Set(['ref', 'text']),
  'browser:browser_scroll': new Set(['direction']),
  'browser:browser_back': new Set([]),
  'browser:browser_press': new Set(['key']),
  'browser:browser_console': new Set(['clear', 'expression']),
  'browser:browser_get_images': new Set([]),
  'browser:browser_inspect': new Set(['question', 'full', 'annotate']),
  'browser:browser_research': new Set(['question', 'web_query', 'include_web', 'web_limit', 'extract_top', 'full', 'annotate']),
  'browser:browser_vision': new Set(['question', 'annotate']),
  'browser:browser_navigate': new Set(['url']),
  'web:web_search': new Set(['query']),
  'web:web_extract': new Set(['url', 'urls', 'selected_urls', 'selectedUrls']),
  'web:web_research': new Set(['query', 'url', 'urls']),
  'process:execute_command': new Set(['path', 'common', 'command', 'background']),
  'process:get_recent_logs': new Set(['per_terminal_limit', 'terminal_limit']),
  'process:process_list': new Set(['include_exited', 'limit']),
  'process:process_poll': new Set(['terminal_id', 'offset', 'limit']),
  'process:process_log': new Set(['terminal_id', 'offset', 'limit']),
  'process:process_wait': new Set(['terminal_id', 'timeout_ms', 'timeout']),
  'process:process_write': new Set(['terminal_id', 'data', 'submit']),
  'process:process_kill': new Set(['terminal_id']),
  'process:process': new Set(['action', 'terminal_id', 'include_exited', 'offset', 'limit', 'timeout_ms', 'timeout', 'data']),
  'remote:list_connections': new Set([]),
  'remote:test_connection': new Set([]),
  'remote:run_command': new Set(['command', 'timeout_seconds', 'allow_dangerous']),
  'remote:list_directory': new Set(['path', 'limit']),
  'remote:read_file': new Set(['path', 'max_bytes']),
  'notepad:init': new Set([]),
  'notepad:list_folders': new Set([]),
  'notepad:create_folder': new Set(['folder']),
  'notepad:rename_folder': new Set(['from', 'to']),
  'notepad:delete_folder': new Set(['folder', 'recursive']),
  'notepad:list_notes': new Set(['folder', 'recursive', 'tags', 'match', 'query', 'limit']),
  'notepad:create_note': new Set(['folder', 'title', 'content', 'tags']),
  'notepad:read_note': new Set(['id']),
  'notepad:update_note': new Set(['id', 'title', 'content', 'folder', 'tags']),
  'notepad:delete_note': new Set(['id']),
  'notepad:list_tags': new Set([]),
  'notepad:search_notes': new Set(['query', 'folder', 'recursive', 'tags', 'match', 'includeContent', 'limit']),
  'task:add_task': new Set(['tasks', 'title', 'details', 'priority', 'status', 'tags', 'due_at']),
  'task:list_tasks': new Set(['include_done', 'current_turn_only', 'limit']),
  'task:update_task': new Set(['task_id', 'changes']),
  'task:complete_task': new Set(['task_id']),
  'task:delete_task': new Set(['task_id']),
  'ui:prompt_key_values': new Set(['title', 'message', 'fields', 'allow_cancel']),
  'ui:prompt_choices': new Set(['title', 'message', 'multiple', 'options', 'default', 'min_selections', 'max_selections', 'allow_cancel']),
  'ui:prompt_mixed_form': new Set(['title', 'message', 'fields', 'choice', 'allow_cancel']),
  'agent:recommend_agent_profile': new Set(['requirement']),
  'agent:list_available_skills': new Set([]),
  'agent:create_memory_agent': new Set(['name', 'role_definition', 'description', 'category', 'enabled', 'plugin_sources', 'skill_ids', 'default_skill_ids', 'skills']),
  'agent:update_memory_agent': new Set(['agent_id', 'name', 'role_definition', 'description', 'category', 'enabled', 'plugin_sources', 'skill_ids', 'default_skill_ids', 'skills']),
  'agent:preview_agent_context': new Set(['role_definition', 'skills', 'plugin_sources', 'skill_ids']),
  'memory:get_command_detail': new Set(['command_ref']),
  'memory:get_plugin_detail': new Set(['plugin_ref']),
  'memory:get_skill_detail': new Set(['skill_ref']),
};

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

const shouldHideArgumentKey = (rawToolName: string | undefined, key: string): boolean => {
  if (GENERIC_HIDDEN_ARGUMENT_KEYS.has(key)) {
    return true;
  }

  if (!rawToolName) {
    return false;
  }

  const displayName = getToolDisplayName(rawToolName);
  const routingKey = resolveToolRoutingKey(rawToolName, displayName);
  const allowlist = TOOL_ARGUMENT_ALLOWLIST[routingKey];
  if (allowlist) {
    return !allowlist.has(key);
  }

  return false;
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
