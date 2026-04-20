import React, { useMemo, useState } from 'react';
import { BuiltinToolDetails, isBuiltinToolRenderable } from './BuiltinToolDetails';
import { LazyMarkdownRenderer } from './LazyMarkdownRenderer';
import { ToolArgumentsDetails } from './ToolArgumentsDetails';
import GenericStructuredResultDetails from './toolCards/shared/GenericStructuredResultDetails';
import type { ToolCall, Message } from '../types';
import type { ToolFamily } from '../lib/tools/catalog';
import { resolveToolFamily } from '../lib/tools/catalog';
import { getToolDisplayName } from '../lib/tools/displayName';
import './ToolCallRenderer.css';

const asRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

const asString = (value: unknown): string => (
  typeof value === 'string' ? value : ''
);

const asBoolean = (value: unknown): boolean | null => (
  typeof value === 'boolean' ? value : null
);

const asNumber = (value: unknown): number | null => {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === 'string' && value.trim().length > 0) {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return null;
};

const asArray = (value: unknown): unknown[] => (
  Array.isArray(value) ? value : []
);

const asStringList = (value: unknown): string[] => (
  asArray(value)
    .map((item) => asString(item).trim())
    .filter((item) => item.length > 0)
);

const CODE_READ_TOOL_NAMES = new Set([
  'read_file_raw',
  'read_file_range',
  'read_file',
]);

const OMITTED_STRUCTURED_RESULT_KEYS = new Set([
  '_summary_text',
  'summary_text',
  'summaryText',
  'research_findings',
  'researchFindings',
]);

const RESEARCH_STRUCTURED_RESULT_OMITTED_PATHS = new Set([
  'page.snapshot',
  'page.console_messages',
  'page.js_errors',
  'page.messages_brief',
  'page.errors_brief',
  'page.message_count_by_type',
  'search.data',
  'extract.results',
]);

const extractStructuredToolMessageResult = (message?: Message): unknown => {
  if (!message) return undefined;
  const metadata = message.metadata;
  if (metadata && typeof metadata === 'object' && !Array.isArray(metadata)) {
    if (Object.prototype.hasOwnProperty.call(metadata, 'structured_result')) {
      return (metadata as Record<string, unknown>).structured_result;
    }
    if (Object.prototype.hasOwnProperty.call(metadata, 'structuredResult')) {
      return (metadata as Record<string, unknown>).structuredResult;
    }
  }
  return message.content;
};

const isResearchToolName = (toolName: string): boolean => (
  toolNameMatches(toolName, 'browser_research')
  || toolNameMatches(toolName, 'web_research')
);

const shouldOmitStructuredResultPath = (
  toolName: string,
  path: string,
  value: unknown,
): boolean => {
  const key = path.split('.').pop() ?? path;
  if (OMITTED_STRUCTURED_RESULT_KEYS.has(key)) {
    return true;
  }

  if (!isResearchToolName(toolName)) {
    return false;
  }

  if (RESEARCH_STRUCTURED_RESULT_OMITTED_PATHS.has(path)) {
    return true;
  }

  if (
    (path === 'search.provider_attempts' || path === 'extract.provider_attempts')
    && asArray(value).length === 0
  ) {
    return true;
  }

  return false;
};

const sanitizeStructuredResultForDisplay = (
  value: unknown,
  toolName: string,
  path: string = '',
): unknown => {
  if (Array.isArray(value)) {
    return value.map((item) => sanitizeStructuredResultForDisplay(item, toolName, path));
  }
  if (!value || typeof value !== 'object') {
    return value;
  }

  const entries = Object.entries(value as Record<string, unknown>)
    .filter(([key, nestedValue]) => {
      const currentPath = path ? `${path}.${key}` : key;
      return !shouldOmitStructuredResultPath(toolName, currentPath, nestedValue);
    })
    .map(([key, nestedValue]) => {
      const currentPath = path ? `${path}.${key}` : key;
      return [key, sanitizeStructuredResultForDisplay(nestedValue, toolName, currentPath)];
    });

  return Object.fromEntries(entries);
};

const hasStructuredContent = (value: unknown): boolean => {
  if (Array.isArray(value)) {
    return value.length > 0;
  }
  if (!value || typeof value !== 'object') {
    return false;
  }
  return Object.keys(value as Record<string, unknown>).length > 0;
};

const extractCodeFenceContents = (value: string): string[] => {
  const matches = value.matchAll(/```(?:[\w-]+)?\s*([\s\S]*?)```/g);
  const candidates: string[] = [];

  for (const match of matches) {
    const candidate = (match[1] || '').trim();
    if (candidate.length > 0) {
      candidates.push(candidate);
    }
  }

  return candidates;
};

const extractBalancedJsonObject = (value: string): string | null => {
  for (let start = value.indexOf('{'); start >= 0; start = value.indexOf('{', start + 1)) {
    let depth = 0;
    let inString = false;
    let escaped = false;

    for (let index = start; index < value.length; index += 1) {
      const char = value[index];

      if (inString) {
        if (escaped) {
          escaped = false;
        } else if (char === '\\') {
          escaped = true;
        } else if (char === '"') {
          inString = false;
        }
        continue;
      }

      if (char === '"') {
        inString = true;
        continue;
      }

      if (char === '{') {
        depth += 1;
        continue;
      }

      if (char === '}') {
        depth -= 1;
        if (depth === 0) {
          return value.slice(start, index + 1);
        }
      }
    }
  }

  return null;
};

const extractJsonishStringField = (
  value: string,
  keys: string[],
): string | undefined => {
  const keyPattern = keys
    .map((key) => key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    .join('|');
  const match = value.match(
    new RegExp(`"(?:${keyPattern})"\\s*:\\s*"((?:\\\\.|[^"\\\\])*)"`, 's'),
  );

  if (!match) {
    return undefined;
  }

  const normalized = match[1]
    .replace(/\r/g, '\\r')
    .replace(/\n/g, '\\n');

  try {
    const parsed = JSON.parse(`"${normalized}"`);
    return typeof parsed === 'string' ? parsed : undefined;
  } catch {
    return undefined;
  }
};

const extractJsonishNumberField = (
  value: string,
  keys: string[],
): number | undefined => {
  const keyPattern = keys
    .map((key) => key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    .join('|');
  const match = value.match(
    new RegExp(`"(?:${keyPattern})"\\s*:\\s*(-?\\d+(?:\\.\\d+)?)`, 'i'),
  );

  if (!match) {
    return undefined;
  }

  const parsed = Number(match[1]);
  return Number.isFinite(parsed) ? parsed : undefined;
};

const extractJsonishBooleanField = (
  value: string,
  keys: string[],
): boolean | undefined => {
  const keyPattern = keys
    .map((key) => key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    .join('|');
  const match = value.match(
    new RegExp(`"(?:${keyPattern})"\\s*:\\s*(true|false)`, 'i'),
  );

  if (!match) {
    return undefined;
  }

  return match[1].toLowerCase() === 'true';
};

const parseMaybeStructuredValue = (value: unknown): Record<string, unknown> | null => {
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }

  if (typeof value !== 'string') {
    return null;
  }

  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }

  const candidates = [
    trimmed,
    ...extractCodeFenceContents(trimmed),
  ];
  const extractedObject = extractBalancedJsonObject(trimmed);
  if (extractedObject && !candidates.includes(extractedObject)) {
    candidates.push(extractedObject);
  }

  const visited = new Set<string>();

  for (const candidate of candidates) {
    const normalized = candidate.trim();
    if (!normalized || visited.has(normalized)) {
      continue;
    }
    visited.add(normalized);

    try {
      const parsed = JSON.parse(normalized);
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        return parsed as Record<string, unknown>;
      }
      if (typeof parsed === 'string') {
        const nested = parseMaybeStructuredValue(parsed);
        if (nested) {
          return nested;
        }
      }
    } catch {
      continue;
    }
  }

  return null;
};

const extractCodeReadPayloadFromText = (value: string): Record<string, unknown> | null => {
  if (!/"(?:path|content|size_bytes|line_count|start_line|end_line|total_lines|ends_with_newline)"/.test(value)) {
    return null;
  }

  const content = extractJsonishStringField(value, ['content']);
  if (content === undefined) {
    return null;
  }

  const payload: Record<string, unknown> = {
    content,
  };

  const path = extractJsonishStringField(value, ['path']);
  const sha256 = extractJsonishStringField(value, ['sha256']);
  const sizeBytes = extractJsonishNumberField(value, ['size_bytes', 'sizeBytes']);
  const lineCount = extractJsonishNumberField(value, ['line_count', 'lineCount']);
  const startLine = extractJsonishNumberField(value, ['start_line', 'startLine']);
  const endLine = extractJsonishNumberField(value, ['end_line', 'endLine']);
  const totalLines = extractJsonishNumberField(value, ['total_lines', 'totalLines']);
  const endsWithNewline = extractJsonishBooleanField(value, ['ends_with_newline', 'endsWithNewline']);

  if (path !== undefined) payload.path = path;
  if (sha256 !== undefined) payload.sha256 = sha256;
  if (sizeBytes !== undefined) payload.size_bytes = sizeBytes;
  if (lineCount !== undefined) payload.line_count = lineCount;
  if (startLine !== undefined) payload.start_line = startLine;
  if (endLine !== undefined) payload.end_line = endLine;
  if (totalLines !== undefined) payload.total_lines = totalLines;
  if (endsWithNewline !== undefined) payload.ends_with_newline = endsWithNewline;

  return payload;
};

const parseToolStructuredValue = (
  value: unknown,
  displayName: string,
): Record<string, unknown> | null => {
  const parsed = parseMaybeStructuredValue(value);
  if (parsed) {
    return parsed;
  }

  if (typeof value !== 'string') {
    return null;
  }

  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }

  if (CODE_READ_TOOL_NAMES.has(displayName)) {
    return extractCodeReadPayloadFromText(trimmed);
  }

  return null;
};

const triStateLabel = (value: boolean | null): string => (
  value === null ? 'unknown' : (value ? 'yes' : 'no')
);

const toolNameMatches = (actual: string, expected: string): boolean => (
  actual === expected || actual.endsWith(`_${expected}`)
);

const isMeaningfulBrowserPageUrl = (url: string): boolean => {
  const normalized = url.trim().toLowerCase();
  if (!normalized) {
    return false;
  }

  return ![
    'about:blank',
    'about:srcdoc',
    'about:newtab',
    'data:,',
    'chrome://newtab/',
    'chrome://new-tab-page/',
    'edge://newtab/',
  ].includes(normalized);
};

interface ToolCallRendererProps {
  toolCall: ToolCall;
  toolResultById?: Map<string, Message>;
  className?: string;
}

interface ExtractSummary {
  pageCount: number | null;
  truncatedPageCount: number | null;
  totalOriginalChars: number | null;
  totalReturnedChars: number | null;
  totalOmittedChars: number | null;
}

interface ResearchResultSummary {
  searchBackend: string;
  extractBackend: string;
  searchResultCount: number | null;
  extractedPageCount: number | null;
  selectedUrlCount: number | null;
  totalOmittedChars: number | null;
  warning: string;
}

interface ResearchSourceHighlight {
  kind: string;
  title: string;
  url: string;
  status: string;
  note: string;
}

interface ResearchFindingsSummary {
  answerFrame: string;
  pageFindings: string[];
  webFindings: string[];
  sourceHighlights: ResearchSourceHighlight[];
  recommendedNextSteps: string[];
}

interface InspectResultSummary {
  inspectionMode: string;
  pageLabel: string;
  elementCount: number | null;
  snapshotStatus: string;
  consoleStatus: string;
  visionStatus: string;
  totalMessages: number | null;
  totalErrors: number | null;
  pageStateAvailable: boolean | null;
  warning: string;
}

interface ProcessResultSummary {
  terminalId: string;
  processId: string;
  status: string;
  busy: boolean | null;
  completed: boolean | null;
  timedOut: boolean | null;
  processCount: number | null;
}

interface ConsoleResultSummary {
  totalMessages: number | null;
  totalErrors: number | null;
  clearApplied: boolean | null;
  logCount: number | null;
  warnCount: number | null;
  errorCount: number | null;
}

const getToolFamilyLabel = (family: ToolFamily): string => {
  switch (family) {
    case 'browser':
      return '浏览器自动化';
    case 'web':
      return '网页研究';
    case 'code':
      return '文件与代码';
    case 'process':
      return '进程控制';
    case 'remote':
      return '远程连接';
    case 'notepad':
      return '笔记工具';
    case 'task':
      return '任务管理';
    case 'ui':
      return '交互确认';
    case 'agent':
      return '智能体构建';
    case 'memory':
      return '记忆读取';
    default:
      return '工具调用';
  }
};

const getToolFamilyDescription = (family: ToolFamily, displayName: string): string => {
  if (family === 'code') {
    if (displayName === 'list_dir') return '浏览目录结构、文件属性和变更范围';
    if (displayName === 'search_text' || displayName === 'search_files') return '聚合命中位置、内容片段和搜索结果';
    if (displayName === 'read_file' || displayName === 'read_file_raw' || displayName === 'read_file_range') {
      return '读取文件内容、范围信息和摘要';
    }
    if (displayName === 'delete_path') return '删除文件或目录并记录执行结果';
    return '修改工作区文件并回传变更摘要';
  }

  if (family === 'browser') {
    if (displayName === 'browser_research') return '结合当前页观察、搜索结果和提取来源';
    if (displayName === 'browser_inspect') return '观察当前页面状态、元素引用和告警';
    if (displayName === 'browser_console' || displayName === 'browser_console_eval') return '采集控制台信息或执行页面脚本';
    if (displayName === 'browser_vision') return '整理截图、视觉分析和模型元信息';
    if (displayName === 'browser_get_images') return '收集页面图片与资源尺寸信息';
    return '浏览器自动化执行与页面状态采集';
  }

  if (family === 'web') {
    if (displayName === 'web_research') return '搜索、筛选链接并整理研究结论';
    if (displayName === 'web_extract') return '提取网页正文、来源摘要和省略信息';
    return '网页搜索与内容提取结果';
  }

  if (family === 'process') {
    if (displayName === 'execute_command') return '展示命令执行状态与终端输出';
    if (displayName === 'get_recent_logs') return '展示最近终端日志与终端分组';
    if (displayName === 'process_log' || displayName === 'process_poll') return '展示进程日志窗口与运行状态';
    if (displayName === 'process_wait') return '展示等待结果、超时状态与输出';
    return '展示终端、进程状态和等待结果';
  }

  if (family === 'remote') {
    if (displayName === 'list_connections') return '展示可用 SSH 连接、目标主机和默认路径';
    if (displayName === 'test_connection') return '展示远程连通性结果与远端主机标识';
    if (displayName === 'run_command') return '展示远程 SSH 命令输出与执行状态';
    if (displayName === 'list_directory') return '展示远程目录条目与目录状态';
    if (displayName === 'read_file') return '展示远程文件内容与截断状态';
    return '展示远程连接与远程主机操作结果';
  }

  if (family === 'notepad') {
    if (displayName === 'init') return '展示笔记空间初始化状态与当前笔记数量';
    if (displayName === 'read_note') return '展示笔记元信息和正文内容';
    if (displayName === 'search_notes') return '展示命中的笔记列表与检索结果';
    if (displayName === 'list_tags') return '展示标签及其使用次数';
    return '展示文件夹、笔记、标签与检索结果';
  }

  if (family === 'task') {
    if (displayName === 'add_task') return '展示待确认的任务创建结果与任务清单';
    if (displayName === 'list_tasks') return '展示当前会话任务列表和范围';
    if (displayName === 'update_task' || displayName === 'complete_task') return '展示任务状态更新后的结果';
    if (displayName === 'delete_task') return '展示任务删除结果';
    return '展示任务确认、任务列表与状态变更';
  }

  if (family === 'ui') {
    if (displayName === 'prompt_choices') return '展示用户选择结果与状态';
    if (displayName === 'prompt_mixed_form') return '展示混合表单填写结果与选择内容';
    if (displayName === 'prompt_key_values') return '展示键值表单填写结果';
    return '展示用户确认结果、表单填写与选择结果';
  }

  if (family === 'agent') {
    if (displayName === 'recommend_agent_profile') return '展示推荐的智能体定位、描述和角色设定';
    if (displayName === 'list_available_skills') return '展示可用技能清单与来源';
    if (displayName === 'create_memory_agent' || displayName === 'update_memory_agent') {
      return '展示 Memory Agent 配置结果、技能和插件来源';
    }
    if (displayName === 'preview_agent_context') return '展示最终注入的角色上下文预览';
    return '展示智能体建议、技能列表与 Memory Agent 配置结果';
  }

  if (family === 'memory') {
    if (displayName === 'get_command_detail') return '展示命令说明、参数提示与完整内容';
    if (displayName === 'get_plugin_detail') return '展示插件信息、命令清单与关联技能';
    if (displayName === 'get_skill_detail') return '展示技能来源、说明和完整内容';
    return '展示命令、插件和技能详情内容';
  }

  return '展示工具输入、输出和运行状态';
};

const ToolFamilyIcon: React.FC<{ family: ToolFamily }> = ({ family }) => {
  if (family === 'browser') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <rect x="3" y="4" width="18" height="14" rx="2" />
        <path d="M8 20h8" />
        <path d="M12 18v2" />
      </svg>
    );
  }
  if (family === 'web') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <circle cx="12" cy="12" r="9" />
        <path d="M3 12h18" />
        <path d="M12 3a14.5 14.5 0 0 1 0 18" />
        <path d="M12 3a14.5 14.5 0 0 0 0 18" />
      </svg>
    );
  }
  if (family === 'code') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="m8 9-3 3 3 3" />
        <path d="m16 9 3 3-3 3" />
        <path d="m14 4-4 16" />
      </svg>
    );
  }
  if (family === 'process') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M4 4h16v10H4z" />
        <path d="M8 20h8" />
        <path d="M12 14v6" />
      </svg>
    );
  }
  if (family === 'remote') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M5 12h14" />
        <path d="m13 6 6 6-6 6" />
        <path d="M11 6H7a3 3 0 0 0-3 3v6a3 3 0 0 0 3 3h4" />
      </svg>
    );
  }
  if (family === 'notepad') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M8 2v4" />
        <path d="M16 2v4" />
        <rect x="4" y="4" width="16" height="18" rx="2" />
        <path d="M8 10h8" />
        <path d="M8 14h8" />
      </svg>
    );
  }
  if (family === 'task') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M9 11l3 3L22 4" />
        <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
      </svg>
    );
  }
  if (family === 'ui') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
      </svg>
    );
  }
  if (family === 'agent') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <rect x="7" y="11" width="10" height="8" rx="2" />
        <path d="M12 2v4" />
        <path d="M9 7h6" />
        <circle cx="10" cy="15" r="1" />
        <circle cx="14" cy="15" r="1" />
      </svg>
    );
  }
  if (family === 'memory') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M6 4h8a4 4 0 0 1 4 4v12H6a2 2 0 0 0-2 2V6a2 2 0 0 1 2-2z" />
        <path d="M18 20a2 2 0 0 0-2-2H4" />
      </svg>
    );
  }
  return (
    <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M13 2 3 14h7l-1 8 12-14h-7l1-6z" />
    </svg>
  );
};

const SectionIcon: React.FC<{ kind: 'input' | 'result' | 'stream' | 'error' | 'meta' }> = ({ kind }) => {
  if (kind === 'input') {
    return (
      <svg className="tool-section-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M14 5h6v6" />
        <path d="M10 19H4v-6" />
        <path d="M20 5 9 16" />
      </svg>
    );
  }
  if (kind === 'result') {
    return (
      <svg className="tool-section-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M20 6 9 17l-5-5" />
      </svg>
    );
  }
  if (kind === 'stream') {
    return (
      <svg className="tool-section-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M5 12h14" />
        <path d="m13 6 6 6-6 6" />
      </svg>
    );
  }
  if (kind === 'error') {
    return (
      <svg className="tool-section-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <circle cx="12" cy="12" r="9" />
        <path d="m15 9-6 6" />
        <path d="m9 9 6 6" />
      </svg>
    );
  }
  return (
    <svg className="tool-section-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 8v4l3 3" />
      <circle cx="12" cy="12" r="9" />
    </svg>
  );
};

export const ToolCallRenderer: React.FC<ToolCallRendererProps> = ({
  toolCall,
  toolResultById,
  className,
}) => {
  const [showDetails, setShowDetails] = useState(false);
  const displayToolName = useMemo(() => getToolDisplayName(toolCall.name), [toolCall.name]);

  const toolResultMessage = useMemo(() => {
    return toolResultById?.get(String(toolCall.id));
  }, [toolCall.id, toolResultById]);

  // 优先使用流内结果，其次是最终结果，再次是 tool message 回填
  const toolResultMessageValue = useMemo(
    () => extractStructuredToolMessageResult(toolResultMessage),
    [toolResultMessage],
  );
  const rawFinalResult = (toolCall as any)?.finalResult;
  const normalizedResult = useMemo(() => {
    const candidates: unknown[] = [
      rawFinalResult,
      toolCall.result,
      toolResultMessageValue,
    ];

    for (const candidate of candidates) {
      if (candidate && typeof candidate === 'object') {
        return {
          value: candidate,
          parsed: asRecord(candidate),
        };
      }
    }

    for (const candidate of candidates) {
      const parsed = parseToolStructuredValue(candidate, displayToolName);
      if (parsed) {
        return {
          value: parsed,
          parsed,
        };
      }
    }

    for (const candidate of candidates) {
      if (typeof candidate === 'string' && candidate.trim().length > 0) {
        return {
          value: candidate,
          parsed: null,
        };
      }
      if (candidate !== undefined && candidate !== null) {
        return {
          value: candidate,
          parsed: null,
        };
      }
    }

    return {
      value: undefined,
      parsed: null,
    };
  }, [displayToolName, rawFinalResult, toolCall.result, toolResultMessageValue]);
  const result = normalizedResult.value;
  const streamLogText = useMemo(() => {
    if (typeof (toolCall as any)?.streamLog === 'string') return (toolCall as any).streamLog;
    return '';
  }, [toolCall]);
  const resultText = useMemo(() => {
    if (typeof result === 'string') return result;
    if (result === null || result === undefined) return '';
    try {
      return JSON.stringify(result);
    } catch {
      return '';
    }
  }, [result]);
  const finalResultText = useMemo(() => {
    const raw = rawFinalResult;
    if (typeof raw === 'string') return raw;
    if (raw === null || raw === undefined) return '';
    try {
      return JSON.stringify(raw);
    } catch {
      return '';
    }
  }, [rawFinalResult]);
  
  const hasError = !!toolCall.error;
  const hasResult = !!result;
  const hasStreamLog = streamLogText.trim().length > 0;
  const hasPersistedResult = !!toolResultMessage?.content;
  const isMarkedCompleted = (toolCall as any)?.completed === true;
  const looksCompletedFromSnapshot =
    hasStreamLog && resultText.trim().length > 0 && resultText !== streamLogText;
  const hasFinalResult =
    isMarkedCompleted
    || finalResultText.trim().length > 0
    || hasPersistedResult
    || looksCompletedFromSnapshot
    || (!hasStreamLog && hasResult);
  
  const hasArguments = useMemo(() => {
    if (!toolCall.arguments) return false;
    if (typeof toolCall.arguments === 'string') {
      const trimmed = toolCall.arguments.trim();
      return trimmed.length > 0 && trimmed !== '{}' && trimmed !== '[]';
    }
    if (typeof toolCall.arguments === 'object') {
      return Object.keys(toolCall.arguments).length > 0;
    }
    return false;
  }, [toolCall.arguments]);

  const parsedArguments = useMemo<unknown>(() => {
    if (!showDetails || !hasArguments) return null;

    if (typeof toolCall.arguments === 'object') {
      return toolCall.arguments;
    }

    if (typeof toolCall.arguments === 'string') {
      const trimmed = toolCall.arguments.trim();
      if (!trimmed) {
        return null;
      }

      try {
        return JSON.parse(trimmed);
      } catch {
        return trimmed;
      }
    }

    return toolCall.arguments;
  }, [hasArguments, showDetails, toolCall.arguments]);

  const parsedResult = useMemo((): any | null => {
    if (!hasResult) return null;
    return normalizedResult.parsed;
  }, [hasResult, normalizedResult.parsed]);

  const structuredDisplayResult = useMemo(
    () => sanitizeStructuredResultForDisplay(parsedResult, toolCall.name),
    [parsedResult, toolCall.name],
  );

  const hasStructuredResult = useMemo(
    () => hasStructuredContent(structuredDisplayResult),
    [structuredDisplayResult],
  );
  const shouldUseBuiltinDetails = useMemo(
    () => isBuiltinToolRenderable(toolCall.name, parsedResult),
    [parsedResult, toolCall.name],
  );

  const structuredResultNote = useMemo(() => {
    if (!showDetails || !hasStructuredResult || !isResearchToolName(toolCall.name)) {
      return '';
    }
    return 'Raw research payload is trimmed here for readability. Use the findings card, selected URLs, and results_brief entries for the most useful details.';
  }, [hasStructuredResult, showDetails, toolCall.name]);

  const extractSummary = useMemo<ExtractSummary | null>(() => {
    if (!showDetails) return null;
    const record = asRecord(parsedResult);
    if (!record) return null;
    const extract = asRecord(record.extract_summary ?? record.extractSummary);
    if (!extract) return null;
    return {
      pageCount: asNumber(extract.page_count ?? extract.pageCount),
      truncatedPageCount: asNumber(extract.truncated_page_count ?? extract.truncatedPageCount),
      totalOriginalChars: asNumber(extract.total_original_chars ?? extract.totalOriginalChars),
      totalReturnedChars: asNumber(extract.total_returned_chars ?? extract.totalReturnedChars),
      totalOmittedChars: asNumber(extract.total_omitted_chars ?? extract.totalOmittedChars),
    };
  }, [parsedResult, showDetails]);

  const researchFindings = useMemo<ResearchFindingsSummary | null>(() => {
    if (!showDetails) return null;
    const record = asRecord(parsedResult);
    if (!record) return null;

    const findings = asRecord(record.research_findings ?? record.researchFindings);
    if (!findings) return null;

    const sourceHighlights = asArray(
      findings.source_highlights ?? findings.sourceHighlights,
    )
      .map((item) => {
        const source = asRecord(item);
        if (!source) return null;
        const title = asString(source.title).trim();
        const url = asString(source.url).trim();
        const status = asString(source.status).trim();
        const note = asString(source.note).trim();
        const kind = asString(source.kind).trim();

        if (!title && !url && !status && !note && !kind) {
          return null;
        }

        return {
          kind: kind || 'unknown',
          title,
          url,
          status: status || 'unknown',
          note,
        };
      })
      .filter((item): item is ResearchSourceHighlight => item !== null);

    const answerFrame = asString(
      findings.answer_frame ?? findings.answerFrame,
    ).trim();
    const pageFindings = asStringList(
      findings.page_findings ?? findings.pageFindings,
    );
    const webFindings = asStringList(
      findings.web_findings ?? findings.webFindings,
    );
    const recommendedNextSteps = asStringList(
      findings.recommended_next_steps ?? findings.recommendedNextSteps,
    );

    if (
      !answerFrame
      && pageFindings.length === 0
      && webFindings.length === 0
      && sourceHighlights.length === 0
      && recommendedNextSteps.length === 0
    ) {
      return null;
    }

    return {
      answerFrame,
      pageFindings,
      webFindings,
      sourceHighlights,
      recommendedNextSteps,
    };
  }, [parsedResult, showDetails]);

  const researchSummary = useMemo<ResearchResultSummary | null>(() => {
    if (!showDetails) return null;
    const record = asRecord(parsedResult);
    if (!record) return null;

    const research = asRecord(record.research_summary ?? record.researchSummary);
    const nestedSearch = asRecord(record.search);
    const nestedExtract = asRecord(record.extract);
    const nestedExtractSummary = asRecord(
      nestedExtract?.extract_summary ?? nestedExtract?.extractSummary,
    );

    const searchBackend = asString(
      research?.search_backend ?? research?.searchBackend ?? nestedSearch?.backend,
    ).trim();
    const extractBackend = asString(
      research?.extract_backend ?? research?.extractBackend ?? nestedExtract?.backend,
    ).trim();
    const searchResultCount = asNumber(
      research?.search_result_count ?? research?.searchResultCount ?? nestedSearch?.result_count ?? nestedSearch?.resultCount,
    );
    const extractedPageCount = asNumber(
      research?.extracted_page_count ?? research?.extractedPageCount ?? nestedExtractSummary?.page_count ?? nestedExtractSummary?.pageCount,
    );
    const selectedUrlCount = asNumber(
      research?.selected_url_count ?? research?.selectedUrlCount,
    );
    const totalOmittedChars = asNumber(
      research?.total_omitted_chars ?? research?.totalOmittedChars ?? nestedExtractSummary?.total_omitted_chars ?? nestedExtractSummary?.totalOmittedChars,
    );
    const warning = asString(research?.warning).trim();

    if (
      !searchBackend
      && !extractBackend
      && searchResultCount === null
      && extractedPageCount === null
      && selectedUrlCount === null
      && totalOmittedChars === null
      && !warning
    ) {
      return null;
    }

    return {
      searchBackend: searchBackend || 'unknown',
      extractBackend: extractBackend || 'unknown',
      searchResultCount,
      extractedPageCount,
      selectedUrlCount,
      totalOmittedChars,
      warning,
    };
  }, [parsedResult, showDetails]);

  const inspectSummary = useMemo<InspectResultSummary | null>(() => {
    if (
      !showDetails
      || (!toolNameMatches(toolCall.name, 'browser_inspect')
        && !toolNameMatches(toolCall.name, 'browser_research'))
    ) {
      return null;
    }
    const rawRecord = asRecord(parsedResult);
    if (!rawRecord) return null;
    const record = toolNameMatches(toolCall.name, 'browser_research')
      ? (asRecord(rawRecord.page) || rawRecord)
      : rawRecord;

    const steps = asRecord(record.inspection_steps ?? record.inspectionSteps);
    const inspectionMode = asString(
      record.inspection_mode ?? record.inspectionMode,
    ).trim();
    const title = asString(record.title).trim();
    const rawUrl = asString(record.url).trim();
    const url = isMeaningfulBrowserPageUrl(rawUrl) ? rawUrl : '';
    const elementCount = asNumber(record.element_count ?? record.elementCount);
    const snapshotStatus = asString(steps?.snapshot).trim();
    const consoleStatus = asString(steps?.console).trim();
    const visionStatus = asString(steps?.vision).trim();
    const totalMessages = asNumber(record.total_messages ?? record.totalMessages);
    const totalErrors = asNumber(record.total_errors ?? record.totalErrors);
    const pageStateAvailable = asBoolean(
      record.page_state_available ?? record.pageStateAvailable,
    );
    const warning = asString(
      record.inspection_warning ?? record.inspectionWarning,
    ).trim();

    const pageLabel = title && url
      ? `${title} [${url}]`
      : (title || url || (pageStateAvailable === false ? '未打开页面' : ''));

    if (
      !inspectionMode
      && !pageLabel
      && elementCount === null
      && !snapshotStatus
      && !consoleStatus
      && !visionStatus
      && totalMessages === null
      && totalErrors === null
      && pageStateAvailable === null
      && !warning
    ) {
      return null;
    }

    return {
      inspectionMode: inspectionMode || 'unknown',
      pageLabel: pageLabel || 'n/a',
      elementCount,
      snapshotStatus: snapshotStatus || 'unknown',
      consoleStatus: consoleStatus || 'unknown',
      visionStatus: visionStatus || 'unknown',
      totalMessages,
      totalErrors,
      pageStateAvailable,
      warning,
    };
  }, [parsedResult, showDetails, toolCall.name]);

  const processSummary = useMemo<ProcessResultSummary | null>(() => {
    if (!showDetails || resolveToolFamily(toolCall.name, displayToolName) !== 'process') {
      return null;
    }
    const record = asRecord(parsedResult);
    if (!record) return null;

    const terminalId = (
      asString(record.terminal_id)
      || asString(record.process_id)
    ).trim();
    const processId = (
      asString(record.process_id)
      || asString(record.terminal_id)
    ).trim();
    const status = (
      asString(record.wait_status)
      || asString(record.operation_status)
      || asString(record.process_status)
      || asString(record.status)
    ).trim();
    const busy = asBoolean(record.busy);
    const completed = asBoolean(record.completed);
    const timedOut = asBoolean(record.timed_out ?? record.timedOut);
    let processCount = asNumber(record.process_count ?? record.processCount);
    if (processCount === null) {
      processCount = asArray(record.processes).length || null;
    }

    if (!terminalId && !processId && !status && busy === null && completed === null && timedOut === null && processCount === null) {
      return null;
    }
    return {
      terminalId,
      processId,
      status: status || 'unknown',
      busy,
      completed,
      timedOut,
      processCount,
    };
  }, [displayToolName, parsedResult, showDetails, toolCall.name]);

  const consoleSummary = useMemo<ConsoleResultSummary | null>(() => {
    if (!showDetails) return null;
    const record = asRecord(parsedResult);
    if (!record) return null;

    const counts = asRecord(record.message_count_by_type ?? record.messageCountByType);
    const totalMessages = asNumber(record.total_messages ?? record.totalMessages);
    const totalErrors = asNumber(record.total_errors ?? record.totalErrors);
    const clearApplied = asBoolean(record.clear_applied ?? record.clearApplied);
    const logCount = counts ? asNumber(counts.log) : null;
    const warnCount = counts ? asNumber(counts.warn ?? counts.warning) : null;
    const errorCount = counts ? asNumber(counts.error) : null;

    if (
      totalMessages === null
      && totalErrors === null
      && clearApplied === null
      && logCount === null
      && warnCount === null
      && errorCount === null
    ) {
      return null;
    }

    return {
      totalMessages,
      totalErrors,
      clearApplied,
      logCount,
      warnCount,
      errorCount,
    };
  }, [parsedResult, showDetails]);

  const resultSummaryText = useMemo(() => {
    if (!showDetails) return '';
    const record = asRecord(parsedResult);
    const summary = record ? asString(record._summary_text ?? record.summary_text ?? record.summaryText) : '';
    return summary.trim();
  }, [parsedResult, showDetails]);

  const statusText = hasError
    ? '错误'
    : (hasFinalResult ? '完成' : (hasStreamLog ? '运行中' : '等待中'));
  const statusClass = hasError
    ? 'error'
    : (hasFinalResult ? 'success' : 'pending');
  const canToggleDetails = hasArguments || hasResult || hasError || hasStreamLog;
  const toolFamily = useMemo(
    () => resolveToolFamily(toolCall.name, displayToolName),
    [displayToolName, toolCall.name],
  );
  const toolFamilyLabel = useMemo(
    () => getToolFamilyLabel(toolFamily),
    [toolFamily],
  );
  const toolDescription = useMemo(
    () => getToolFamilyDescription(toolFamily, displayToolName),
    [displayToolName, toolFamily],
  );
  const executionTime = useMemo(() => {
    const date = new Date(toolCall.createdAt);
    return Number.isNaN(date.getTime()) ? '时间未知' : date.toLocaleString();
  }, [toolCall.createdAt]);
  const toolSourceLabel = shouldUseBuiltinDetails ? '内置面板' : '通用面板';
  const toggleTitle = showDetails ? '收起详情' : '查看详情';
  const toggleLabel = showDetails ? '收起' : '展开';
  const toggleDetails = () => setShowDetails((prev) => !prev);

  const chipContent = (
    <>
      <div className="tool-chip-left">
        <div className="tool-icon-shell">
          <ToolFamilyIcon family={toolFamily} />
        </div>
        <div className="tool-chip-main">
          <div className="tool-chip-topline">
            <span className="tool-family-badge">{toolFamilyLabel}</span>
            <span className="tool-source-badge">{toolSourceLabel}</span>
            <span className={`tool-status ${statusClass}`}>{statusText}</span>
          </div>
          <div className="tool-name-row">
            <div className="tool-name" title={toolCall.name}>@{displayToolName}</div>
            {canToggleDetails && (
              <span className={`tool-inline-toggle ${showDetails ? 'expanded' : ''}`}>
                <span>{toggleLabel}</span>
                <svg className="tool-toggle-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <polyline points="6 9 12 15 18 9" />
                </svg>
              </span>
            )}
          </div>
          {(showDetails || !canToggleDetails) && (
            <div className="tool-chip-subtitle">{toolDescription}</div>
          )}
        </div>
      </div>
    </>
  );

  return (
    <div
      className={`tool-call-renderer tool-call-container tool-call-container--${toolFamily}${className ? ` ${className}` : ''}`}
    >
      {canToggleDetails ? (
        <button
          type="button"
          onClick={toggleDetails}
          className={`tool-chip tool-chip--clickable ${showDetails ? 'expanded' : ''}`}
          aria-label={toggleTitle}
          aria-expanded={showDetails}
          title={toggleTitle}
        >
          {chipContent}
        </button>
      ) : (
        <div className={`tool-chip ${showDetails ? 'expanded' : ''}`}>
          {chipContent}
        </div>
      )}

      {/* 详细信息 - 展开时显示 */}
      {canToggleDetails && showDetails && (
         <div className="details-container">

          {/* 参数详情 */}
          {hasArguments && parsedArguments !== null && (
            <section className="tool-panel-section">
              <div className="details-title">
                <span className="tool-section-icon">
                  <SectionIcon kind="input" />
                </span>
                <span className="tool-section-label">输入</span>
                <span className="tool-section-subtitle">本次调用传入的参数</span>
              </div>
              <ToolArgumentsDetails
                argumentsValue={parsedArguments}
                rawToolName={toolCall.name}
              />
            </section>
          )}

          {/* 结果 */}
          {hasResult && (
            <section className="tool-panel-section">
              <div className="details-title">
                <span className="tool-section-icon">
                  <SectionIcon kind="result" />
                </span>
                <span className="tool-section-label">结果</span>
                <span className="tool-section-subtitle">工具返回的结构化内容与摘要</span>
              </div>
              {resultSummaryText && (
                <div className="tool-rich-note tool-rich-note--summary">
                  <LazyMarkdownRenderer content={resultSummaryText} />
                </div>
              )}
              {(researchFindings || extractSummary || researchSummary || inspectSummary || consoleSummary || processSummary) && (
                <div className="tool-summary-stack">
                  {researchFindings && (
                    <div className="tool-summary-card tool-findings-card">
                      <div className="tool-summary-title">Research findings</div>
                      {researchFindings.answerFrame && (
                        <div className="tool-findings-answer">{researchFindings.answerFrame}</div>
                      )}

                      {researchFindings.pageFindings.length > 0 && (
                        <div className="tool-findings-section">
                          <div className="tool-findings-section-title">Page findings</div>
                          <div className="tool-findings-list">
                            {researchFindings.pageFindings.map((finding, index) => (
                              <div key={`page-finding-${index}`} className="tool-findings-item">
                                {finding}
                              </div>
                            ))}
                          </div>
                        </div>
                      )}

                      {researchFindings.webFindings.length > 0 && (
                        <div className="tool-findings-section">
                          <div className="tool-findings-section-title">Web findings</div>
                          <div className="tool-findings-list">
                            {researchFindings.webFindings.map((finding, index) => (
                              <div key={`web-finding-${index}`} className="tool-findings-item">
                                {finding}
                              </div>
                            ))}
                          </div>
                        </div>
                      )}

                      {researchFindings.sourceHighlights.length > 0 && (
                        <div className="tool-findings-section">
                          <div className="tool-findings-section-title">Source highlights</div>
                          <div className="tool-findings-list">
                            {researchFindings.sourceHighlights.map((source, index) => {
                              const label = source.title || source.url || 'untitled source';
                              return (
                                <div key={`source-highlight-${index}`} className="tool-findings-source">
                                  <div className="tool-findings-source-row">
                                    <span className="tool-findings-source-kind">{source.kind}</span>
                                    <span className="tool-findings-source-status">{source.status}</span>
                                  </div>
                                  <div className="tool-findings-source-title">
                                    {source.url ? (
                                      <a href={source.url} target="_blank" rel="noreferrer">
                                        {label}
                                      </a>
                                    ) : (
                                      label
                                    )}
                                  </div>
                                  {source.note && (
                                    <div className="tool-findings-source-note">{source.note}</div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        </div>
                      )}

                      {researchFindings.recommendedNextSteps.length > 0 && (
                        <div className="tool-findings-section">
                          <div className="tool-findings-section-title">Recommended next steps</div>
                          <div className="tool-findings-list">
                            {researchFindings.recommendedNextSteps.map((step, index) => (
                              <div key={`next-step-${index}`} className="tool-findings-item">
                                {step}
                              </div>
                            ))}
                          </div>
                        </div>
                      )}
                    </div>
                  )}

                  {researchSummary && (
                    <div className="tool-summary-card">
                      <div className="tool-summary-title">Research overview</div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">search results</span>
                        <span className="tool-summary-value">{researchSummary.searchResultCount ?? 'n/a'}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">extracted pages</span>
                        <span className="tool-summary-value">{researchSummary.extractedPageCount ?? 'n/a'}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">selected urls</span>
                        <span className="tool-summary-value">{researchSummary.selectedUrlCount ?? 'n/a'}</span>
                      </div>
                      {researchSummary.warning && (
                        <div className="tool-summary-row">
                          <span className="tool-summary-key">warning</span>
                          <span className="tool-summary-value">{researchSummary.warning}</span>
                        </div>
                      )}
                    </div>
                  )}

                  {inspectSummary && (
                    <div className="tool-summary-card">
                      <div className="tool-summary-title">Current page</div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">page</span>
                        <span className="tool-summary-value">{inspectSummary.pageLabel}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">console messages</span>
                        <span className="tool-summary-value">{inspectSummary.totalMessages ?? 'n/a'}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">js errors</span>
                        <span className="tool-summary-value">{inspectSummary.totalErrors ?? 'n/a'}</span>
                      </div>
                      {inspectSummary.warning && (
                        <div className="tool-summary-row">
                          <span className="tool-summary-key">warning</span>
                          <span className="tool-summary-value">{inspectSummary.warning}</span>
                        </div>
                      )}
                    </div>
                  )}

                  {extractSummary && (
                    <div className="tool-summary-card">
                      <div className="tool-summary-title">Extract summary</div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">pages</span>
                        <span className="tool-summary-value">{extractSummary.pageCount ?? 'n/a'}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">truncated pages</span>
                        <span className="tool-summary-value">{extractSummary.truncatedPageCount ?? 'n/a'}</span>
                      </div>
                    </div>
                  )}

                  {consoleSummary && (
                    <div className="tool-summary-card">
                      <div className="tool-summary-title">Console summary</div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">messages</span>
                        <span className="tool-summary-value">{consoleSummary.totalMessages ?? 'n/a'}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">errors</span>
                        <span className="tool-summary-value">{consoleSummary.totalErrors ?? 'n/a'}</span>
                      </div>
                    </div>
                  )}

                  {processSummary && (
                    <div className="tool-summary-card">
                      <div className="tool-summary-title">Process summary</div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">status</span>
                        <span className="tool-summary-value">{processSummary.status}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">timed out</span>
                        <span className="tool-summary-value">{triStateLabel(processSummary.timedOut)}</span>
                      </div>
                      {processSummary.processCount !== null && (
                        <div className="tool-summary-row">
                          <span className="tool-summary-key">processes</span>
                          <span className="tool-summary-value">{processSummary.processCount}</span>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              )}
              {shouldUseBuiltinDetails ? (
                <BuiltinToolDetails rawToolName={toolCall.name} result={parsedResult} />
              ) : hasStructuredResult ? (
                <>
                  {structuredResultNote && (
                    <div className="tool-structured-note">{structuredResultNote}</div>
                  )}
                  <GenericStructuredResultDetails value={structuredDisplayResult} />
                </>
              ) : (
                <LazyMarkdownRenderer content={typeof result === 'string' ? result : JSON.stringify(result)} />
              )}
            </section>
          )}

          {hasStreamLog && !hasResult && (
            <section className="tool-panel-section">
              <div className="details-title">
                <span className="tool-section-icon">
                  <SectionIcon kind="stream" />
                </span>
                <span className="tool-section-label">流式输出</span>
                <span className="tool-section-subtitle">运行过程中的实时内容</span>
              </div>
              <LazyMarkdownRenderer content={streamLogText} isStreaming />
            </section>
          )}

          {/* 错误 */}
          {hasError && (
            <section className="tool-panel-section">
              <div className="details-title">
                <span className="tool-section-icon">
                  <SectionIcon kind="error" />
                </span>
                <span className="tool-section-label">错误</span>
                <span className="tool-section-subtitle">需要重点处理的失败信息</span>
              </div>
              <div className="tool-error-box">{toolCall.error}</div>
            </section>
          )}

          {/* 时间戳 */}
          <div className="tool-footer">
            <div className="tool-footer-item">
              <span className="tool-section-icon">
                <SectionIcon kind="meta" />
              </span>
              <span className="tool-footer-label">执行时间</span>
              <span className="tool-footer-value">{executionTime}</span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default ToolCallRenderer;
