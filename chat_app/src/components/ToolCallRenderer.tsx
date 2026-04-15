import React, { useEffect, useMemo, useState } from 'react';
import { LazyMarkdownRenderer } from './LazyMarkdownRenderer';
import type { ToolCall, Message } from '../types';
import './ToolCallRenderer.css';

// 递归平铺对象属性
const flattenObject = (obj: any, prefix: string = ''): Record<string, any> => {
  const flattened: Record<string, any> = {};

  for (const key in obj) {
    if (obj.hasOwnProperty(key)) {
      const value = obj[key];
      const newKey = prefix ? `${prefix}.${key}` : key;

      if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
        // 如果是对象，递归平铺
        Object.assign(flattened, flattenObject(value, newKey));
      } else {
        // 如果是基本类型或数组，直接添加
        flattened[newKey] = value;
      }
    }
  }

  return flattened;
};

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

const triStateLabel = (value: boolean | null): string => (
  value === null ? 'unknown' : (value ? 'yes' : 'no')
);

// ===== 树形表格支持：根据 JSON 自动生成层级行 =====
const inferType = (val: any): string => {
  if (val === null) return 'null';
  if (Array.isArray(val)) {
    if (val.length === 0) return 'array []';
    const first = val[0];
    const elemType = inferType(first);
    return elemType === 'object' ? 'object []' : `${elemType}[]`;
  }
  switch (typeof val) {
    case 'string': return 'string';
    case 'number': return Number.isInteger(val) ? 'integer' : 'number';
    case 'boolean': return 'boolean';
    case 'object': return 'object';
    default: return typeof val;
  }
};

interface TreeNode {
  path: string;
  name: string;
  type: string;
  value: any;
  children: TreeNode[];
}

const buildTreeNodes = (obj: any, name = '', path = ''): TreeNode[] => {
  const nodes: TreeNode[] = [];
  if (obj === null || obj === undefined) return nodes;

  if (Array.isArray(obj)) {
    const node: TreeNode = {
      path: path || name || 'list',
      name: name || 'list',
      type: inferType(obj),
      value: obj,
      children: obj.map((item, index) => {
        const itemPath = `${path || name || 'list'}[${index}]`;
        const itemType = inferType(item);
        return {
          path: itemPath,
          name: `[${index}]`,
          type: itemType,
          value: item,
          children: (itemType === 'object' || Array.isArray(item)) ? buildTreeNodes(item, `[${index}]`, itemPath) : []
        };
      })
    };
    nodes.push(node);
    return nodes;
  }

  if (typeof obj === 'object') {
    Object.keys(obj).forEach((key) => {
      const value = obj[key];
      const currentPath = path ? `${path}.${key}` : key;
      const t = inferType(value);
      const children = (t === 'object' || Array.isArray(value)) ? buildTreeNodes(value, key, currentPath) : [];
      nodes.push({
        path: currentPath,
        name: key,
        type: t,
        value,
        children,
      });
    });
  }

  return nodes;
};

const TreeTable: React.FC<{ data: any }> = ({ data }) => {
  const roots = useMemo(
    () => (Array.isArray(data) ? buildTreeNodes(data, 'list', 'list') : buildTreeNodes(data, '', '')),
    [data]
  );
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  useEffect(() => {
    setExpanded(new Set(roots.filter(r => r.children.length > 0).map(r => r.path)));
  }, [roots]);

  const toggleExpand = (p: string) => {
    setExpanded(prev => {
      const next = new Set(prev);
      if (next.has(p)) next.delete(p); else next.add(p);
      return next;
    });
  };

  const formatValue = (val: any): string => {
    if (val === null) return 'null';
    if (Array.isArray(val)) return `数组(${val.length})`;
    if (typeof val === 'object') return '对象';
    if (typeof val === 'string') return val;
    try { return JSON.stringify(val); } catch { return String(val); }
  };

  const renderNodes = (nodes: TreeNode[], depth: number): React.ReactNode => {
    return nodes.map((node) => {
      const hasChildren = node.children && node.children.length > 0;
      const isExpanded = expanded.has(node.path);
      const icon = hasChildren ? (isExpanded ? '▾' : '▸') : '';
      const valueText = hasChildren ? formatValue(node.value) : formatValue(node.value);
      return (
        <React.Fragment key={node.path}>
          <tr>
            <td style={{ paddingLeft: depth * 16 }}>
              {hasChildren ? (
                <button
                  type="button"
                  onClick={() => toggleExpand(node.path)}
                  className="mr-2 text-gray-600 dark:text-gray-300 hover:text-black dark:hover:text-white"
                  aria-label={isExpanded ? '收起' : '展开'}
                >
                  {icon}
                </button>
              ) : (
                <span className="mr-4" />
              )}
              {node.name}
            </td>
            <td style={{ whiteSpace: 'normal', wordBreak: 'break-word' }}>{valueText}</td>
          </tr>
          {hasChildren && isExpanded && renderNodes(node.children, depth + 1)}
        </React.Fragment>
      );
    });
  };

  return (
    <div className="border-l-4 border-green-400 dark:border-green-500 rounded-lg overflow-hidden bg-green-50/50 dark:bg-green-900/20 mb-2">
      <div className="markdown-renderer">
        <table>
          <thead>
            <tr>
              <th>字段</th>
              <th>值</th>
            </tr>
          </thead>
          <tbody>
            {renderNodes(roots, 0)}
          </tbody>
        </table>
      </div>
    </div>
  );
};

interface ToolCallRendererProps {
  toolCall: ToolCall;
  toolResultById?: Map<string, Message>;
  className?: string;
}

interface WebResultSummary {
  backend: string;
  fallbackUsed: boolean | null;
  providerAttempts: number;
}

interface ExtractSummary {
  pageCount: number | null;
  truncatedPageCount: number | null;
  totalOriginalChars: number | null;
  totalReturnedChars: number | null;
  totalOmittedChars: number | null;
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

export const ToolCallRenderer: React.FC<ToolCallRendererProps> = ({
  toolCall,
  toolResultById,
  className,
}) => {
  const [showDetails, setShowDetails] = useState(false);

  const toolResultMessage = useMemo(() => {
    return toolResultById?.get(String(toolCall.id));
  }, [toolCall.id, toolResultById]);

  // 优先使用流内结果，其次是最终结果，再次是 tool message 回填
  const result = toolCall.result ?? (toolCall as any)?.finalResult ?? toolResultMessage?.content;
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
    const raw = (toolCall as any)?.finalResult;
    if (typeof raw === 'string') return raw;
    if (raw === null || raw === undefined) return '';
    try {
      return JSON.stringify(raw);
    } catch {
      return '';
    }
  }, [toolCall]);
  
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

  const parsedArguments = useMemo(() => {
    if (!showDetails || !hasArguments) return {};

    // 如果已经是对象，直接返回
    if (typeof toolCall.arguments === 'object') {
      return toolCall.arguments;
    }

    // 如果是字符串，尝试解析为 JSON
    if (typeof toolCall.arguments === 'string') {
      try {
        return JSON.parse(toolCall.arguments);
      } catch {
        return {};
      }
    }

    return {};
  }, [hasArguments, showDetails, toolCall.arguments]);

  const argumentsMessage = useMemo(() => {
    if (!showDetails || !hasArguments) return '';

    // 平铺所有参数
    const flattenedArgs = flattenObject(parsedArguments);
    const argKeys = Object.keys(flattenedArgs);

    // 统一使用表格形式显示所有参数（包括单个参数）
    let tableContent = '| 参数 | 值 |\n|------|------|\n';
    argKeys.forEach(key => {
      const value = flattenedArgs[key];
      let formattedValue: string;

      if (typeof value === 'string') {
        formattedValue = value.replace(/\n/g, '<br>').replace(/\|/g, '\\|');
      } else if (Array.isArray(value)) {
        formattedValue = `[${value.join(', ')}]`.replace(/\|/g, '\\|');
      } else {
        formattedValue = JSON.stringify(value).replace(/\|/g, '\\|');
      }

      tableContent += `| ${key} | ${formattedValue} |\n`;
    });

    return tableContent;
  }, [hasArguments, parsedArguments, showDetails]);

  const parsedResult = useMemo((): any | null => {
    if (!showDetails || !hasResult) return null;
    if (result && typeof result === 'object') {
      return result;
    }
    if (typeof result === 'string') {
      try {
        const parsed = JSON.parse(result);
        if (parsed && typeof parsed === 'object') {
          return parsed;
        }
      } catch (e) {
        // 非JSON字符串，按原文本渲染
        return null;
      }
    }
    return null;
  }, [hasResult, result, showDetails]);

  const hasStructuredResult = !!(parsedResult && typeof parsedResult === 'object');

  const webSummary = useMemo<WebResultSummary | null>(() => {
    if (!showDetails) return null;
    const record = asRecord(parsedResult);
    if (!record) return null;
    const backend = asString(record.backend || record.provider).trim();
    const fallbackUsed = asBoolean(record.fallback_used ?? record.fallbackUsed);
    const providerAttempts = asArray(record.provider_attempts ?? record.providerAttempts).length;
    if (!backend && fallbackUsed === null && providerAttempts === 0) {
      return null;
    }
    return {
      backend: backend || 'unknown',
      fallbackUsed,
      providerAttempts,
    };
  }, [parsedResult, showDetails]);

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

  const processSummary = useMemo<ProcessResultSummary | null>(() => {
    if (!showDetails) return null;
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
  }, [parsedResult, showDetails]);

  // 移除未使用的表格格式化方法

  const statusText = hasError
    ? '错误'
    : (hasFinalResult ? '完成' : (hasStreamLog ? '运行中' : '等待中'));
  const statusClass = hasError
    ? 'error'
    : (hasFinalResult ? 'success' : 'pending');
  const canToggleDetails = hasArguments || hasResult || hasError || hasStreamLog;

  return (
    <div className={`tool-call-renderer tool-call-container${className ? ` ${className}` : ''}`}>
      <div className={`tool-chip ${showDetails ? 'expanded' : ''}`}>
        <div className="tool-chip-left">
          <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M13 2 3 14h7l-1 8 12-14h-7l1-6z" />
          </svg>
          <span className="tool-name" title={toolCall.name}>@{toolCall.name}</span>
          <span className={`tool-status ${statusClass}`}>{statusText}</span>
        </div>
        {canToggleDetails && (
          <button
            type="button"
            onClick={() => setShowDetails(!showDetails)}
            className={`tool-toggle ${showDetails ? 'expanded' : ''}`}
            aria-label={showDetails ? '收起详情' : '查看详情'}
            aria-expanded={showDetails}
            title={showDetails ? '收起详情' : '查看详情'}
          >
            <svg className="tool-toggle-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <polyline points="6 9 12 15 18 9" />
            </svg>
          </button>
        )}
      </div>

      {/* 详细信息 - 展开时显示 */}
      {canToggleDetails && showDetails && (
         <div className="details-container">
         
          {/* 参数详情 - 移动到详情里面，移除标题 */}
          {hasArguments && (
            <div>
              {/* 使用格式化的参数内容，而不是原始JSON */}
              {argumentsMessage && (
                <div className="border-l-4 border-blue-400 dark:border-blue-500 rounded-lg overflow-hidden bg-blue-50/50 dark:bg-blue-900/20 mb-4">
                  <LazyMarkdownRenderer 
                    content={argumentsMessage} 
                  />
                </div>
              )}
            </div>
          )}

          {/* 结果 */}
          {hasResult && (
            <div>
              <div className="details-title">结果:</div>
              {(webSummary || extractSummary || processSummary) && (
                <div className="tool-summary-stack">
                  {webSummary && (
                    <div className="tool-summary-card">
                      <div className="tool-summary-title">Web backend</div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">provider</span>
                        <span className="tool-summary-value">{webSummary.backend}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">fallback</span>
                        <span className="tool-summary-value">
                          {webSummary.fallbackUsed === null ? 'unknown' : (webSummary.fallbackUsed ? 'yes' : 'no')}
                        </span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">attempts</span>
                        <span className="tool-summary-value">{webSummary.providerAttempts}</span>
                      </div>
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
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">omitted chars</span>
                        <span className="tool-summary-value">{extractSummary.totalOmittedChars ?? 'n/a'}</span>
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
                        <span className="tool-summary-key">terminal</span>
                        <span className="tool-summary-value">{processSummary.terminalId || 'n/a'}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">process id</span>
                        <span className="tool-summary-value">{processSummary.processId || 'n/a'}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">busy</span>
                        <span className="tool-summary-value">{triStateLabel(processSummary.busy)}</span>
                      </div>
                      <div className="tool-summary-row">
                        <span className="tool-summary-key">completed</span>
                        <span className="tool-summary-value">{triStateLabel(processSummary.completed)}</span>
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
              {hasStructuredResult ? (
                <TreeTable data={parsedResult} />
              ) : (
                <LazyMarkdownRenderer content={typeof result === 'string' ? result : JSON.stringify(result)} />
              )}
            </div>
          )}

          {hasStreamLog && !hasResult && (
            <div>
              <div className="details-title">流式输出:</div>
              <LazyMarkdownRenderer content={streamLogText} isStreaming />
            </div>
          )}

          {/* 错误 */}
          {hasError && (
            <div>
              <div className="details-title">错误:</div>
              <div className="tool-error-box">{toolCall.error}</div>
            </div>
          )}

          {/* 时间戳 */}
          <div className="text-xs text-gray-500 dark:text-gray-400 border-t border-gray-200 dark:border-gray-700 pt-2">
            执行时间: {(() => {
              const date = new Date(toolCall.createdAt);
              return isNaN(date.getTime()) ? '时间未知' : date.toLocaleString();
            })()}
          </div>
        </div>
      )}
    </div>
  );
};

export default ToolCallRenderer;
