import React, { useEffect, useMemo, useState } from 'react';
import { MarkdownRenderer } from './MarkdownRenderer';
import SuggestSubAgentModal from './SuggestSubAgentModal';
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

const isSuggestSubAgentTool = (toolName: string): boolean => {
  const normalized = String(toolName || '').trim().toLowerCase();
  if (!normalized) return false;
  return normalized.endsWith('_suggest_sub_agent') || normalized.includes('__suggest_sub_agent');
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
  allMessages?: Message[];
  toolResultById?: Map<string, Message>;
  className?: string;
}

export const ToolCallRenderer: React.FC<ToolCallRendererProps> = ({
  toolCall,
  allMessages = [],
  toolResultById,
  className,
}) => {
  const [showDetails, setShowDetails] = useState(false);
  const [showSuggestModal, setShowSuggestModal] = useState(false);
  const toolName = String(toolCall?.name || '');
  const isSuggestSubAgent = useMemo(() => isSuggestSubAgentTool(toolName), [toolName]);

  const toolResultMessage = useMemo(() => {
    const direct = toolResultById?.get(String(toolCall.id));
    if (direct) return direct;
    return allMessages.find(msg => {
      if (msg.role !== 'tool') return false;
      // 同时检查顶层和metadata中的tool_call_id（兼容不同格式）
      const topLevelId = (msg as any).tool_call_id || (msg as any).toolCallId;
      const metadataId = msg.metadata?.tool_call_id || msg.metadata?.toolCallId;
      return topLevelId === toolCall.id || metadataId === toolCall.id;
    });
  }, [allMessages, toolCall.id, toolResultById]);

  // 优先使用toolCall.result，如果没有则使用tool消息的内容
  const result = toolCall.result || toolResultMessage?.content;
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
      } catch (e) {
        console.warn('Failed to parse tool arguments:', toolCall.arguments, e);
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

  // 移除未使用的表格格式化方法

  const statusText = hasError
    ? '错误'
    : (isSuggestSubAgent
      ? (hasFinalResult ? '完成' : '运行中')
      : (hasResult ? '完成' : '等待中'));
  const statusClass = hasError
    ? 'error'
    : (isSuggestSubAgent
      ? (hasFinalResult ? 'success' : 'pending')
      : (hasResult ? 'success' : 'pending'));
  const canToggleDetails = !isSuggestSubAgent && (hasArguments || hasResult || hasError);

  return (
    <div className={`tool-call-renderer tool-call-container${className ? ` ${className}` : ''}`}>
      <div
        className={`tool-chip ${showDetails ? 'expanded' : ''} ${isSuggestSubAgent ? 'cursor-pointer hover:border-blue-400/70 dark:hover:border-blue-500/70' : ''}`}
        onClick={isSuggestSubAgent ? () => setShowSuggestModal(true) : undefined}
        role={isSuggestSubAgent ? 'button' : undefined}
        tabIndex={isSuggestSubAgent ? 0 : undefined}
        onKeyDown={isSuggestSubAgent ? (event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            setShowSuggestModal(true);
          }
        } : undefined}
      >
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
        {isSuggestSubAgent && (
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              setShowSuggestModal(true);
            }}
            className="tool-toggle"
            aria-label="打开推荐详情弹窗"
            title="打开推荐详情弹窗"
          >
            <svg className="tool-toggle-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M14 3h7v7" />
              <path d="M10 14 21 3" />
              <path d="M21 14v7h-7" />
              <path d="M3 10V3h7" />
              <path d="M3 21h7v-7" />
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
                  <MarkdownRenderer 
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
              {hasStructuredResult ? (
                <TreeTable data={parsedResult} />
              ) : (
                <MarkdownRenderer content={typeof result === 'string' ? result : JSON.stringify(result)} />
              )}
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

      {isSuggestSubAgent && showSuggestModal && (
        <SuggestSubAgentModal
          toolCall={{
            ...toolCall,
            result,
            finalResult: (toolCall as any)?.finalResult,
            persistedResult: toolResultMessage?.content,
          }}
          onClose={() => setShowSuggestModal(false)}
        />
      )}
    </div>
  );
};

export default ToolCallRenderer;
