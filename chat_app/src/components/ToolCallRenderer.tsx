import React, { useMemo, useState } from 'react';
import type { Message } from '../types';
import { resolveToolFamily } from '../lib/tools/catalog';
import { getToolDisplayName } from '../lib/tools/displayName';
import type { MessageToolCallLike } from './messageItem/messageReaders';
import { ToolCallChip } from './toolCallRenderer/ToolCallChip';
import { ToolCallExpandedDetails } from './toolCallRenderer/ToolCallExpandedDetails';
import {
  getToolFamilyDescription,
  getToolFamilyLabel,
} from './toolCallRenderer/core/labels';
import {
  extractStructuredToolMessageResult,
  normalizeToolResult,
} from './toolCallRenderer/core/parse';
import {
  hasStructuredContent,
  sanitizeStructuredResultForDisplay,
} from './toolCallRenderer/core/sanitize';
import {
  buildConsoleSummary,
  buildExtractSummary,
  buildInspectSummary,
  buildProcessSummary,
  buildResearchFindings,
  buildResearchSummary,
  getResultSummaryText,
} from './toolCallRenderer/core/summaries';
import { isResearchToolName } from './toolCallRenderer/core/toolName';
import {
  resolveToolResultRenderer,
  type ToolResultRenderContext,
} from './toolCallRenderer/registry';
import './ToolCallRenderer.css';

interface ToolCallRendererProps {
  toolCall: MessageToolCallLike;
  toolResultById?: Map<string, Message>;
  className?: string;
}

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
  const rawFinalResult = toolCall.finalResult;
  const normalizedResult = useMemo(
    () => normalizeToolResult([
      rawFinalResult,
      toolCall.result,
      toolResultMessageValue,
    ], displayToolName),
    [displayToolName, rawFinalResult, toolCall.result, toolResultMessageValue],
  );
  const result = normalizedResult.value;
  const streamLogText = useMemo(() => {
    if (typeof toolCall.streamLog === 'string') return toolCall.streamLog;
    return '';
  }, [toolCall.streamLog]);
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
  const isMarkedCompleted = toolCall.completed === true;
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

  const parsedResult = useMemo<Record<string, unknown> | null>(() => {
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

  const structuredResultNote = useMemo(() => {
    if (!showDetails || !hasStructuredResult || !isResearchToolName(toolCall.name)) {
      return '';
    }
    return 'Raw research payload is trimmed here for readability. Use the findings card, selected URLs, and results_brief entries for the most useful details.';
  }, [hasStructuredResult, showDetails, toolCall.name]);

  const extractSummary = useMemo(
    () => (showDetails ? buildExtractSummary(parsedResult) : null),
    [parsedResult, showDetails],
  );

  const researchFindings = useMemo(
    () => (showDetails ? buildResearchFindings(parsedResult) : null),
    [parsedResult, showDetails],
  );

  const researchSummary = useMemo(
    () => (showDetails ? buildResearchSummary(parsedResult) : null),
    [parsedResult, showDetails],
  );

  const inspectSummary = useMemo(
    () => (showDetails ? buildInspectSummary(parsedResult, toolCall.name) : null),
    [parsedResult, showDetails, toolCall.name],
  );

  const processSummary = useMemo(
    () => (showDetails ? buildProcessSummary(parsedResult, toolCall.name, displayToolName) : null),
    [displayToolName, parsedResult, showDetails, toolCall.name],
  );

  const consoleSummary = useMemo(
    () => (showDetails ? buildConsoleSummary(parsedResult) : null),
    [parsedResult, showDetails],
  );

  const resultSummaryText = useMemo(
    () => (showDetails ? getResultSummaryText(parsedResult) : ''),
    [parsedResult, showDetails],
  );

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
  const resultRenderContext = useMemo<ToolResultRenderContext>(() => ({
    toolName: toolCall.name,
    displayToolName,
    result,
    parsedResult,
    structuredDisplayResult,
    hasStructuredResult,
    structuredResultNote,
  }), [
    displayToolName,
    hasStructuredResult,
    parsedResult,
    result,
    structuredDisplayResult,
    structuredResultNote,
    toolCall.name,
  ]);
  const resultRenderer = useMemo(
    () => resolveToolResultRenderer(resultRenderContext),
    [resultRenderContext],
  );
  const executionTime = useMemo(() => {
    const date = new Date(toolCall.createdAt);
    return Number.isNaN(date.getTime()) ? '时间未知' : date.toLocaleString();
  }, [toolCall.createdAt]);
  const toolSourceLabel = resultRenderer.sourceLabel;
  const toggleTitle = showDetails ? '收起详情' : '查看详情';
  const toggleLabel = showDetails ? '收起' : '展开';
  const toggleDetails = () => setShowDetails((prev) => !prev);

  return (
    <div
      className={`tool-call-renderer tool-call-container tool-call-container--${toolFamily}${className ? ` ${className}` : ''}`}
    >
      <ToolCallChip
        toolFamily={toolFamily}
        toolName={displayToolName}
        toolFamilyLabel={toolFamilyLabel}
        toolSourceLabel={toolSourceLabel}
        statusText={statusText}
        statusClass={statusClass}
        toolDescription={toolDescription}
        canToggleDetails={canToggleDetails}
        showDetails={showDetails}
        toggleLabel={toggleLabel}
        toggleTitle={toggleTitle}
        onToggle={toggleDetails}
      />

      {canToggleDetails && showDetails && (
        <ToolCallExpandedDetails
          hasArguments={hasArguments}
          parsedArguments={parsedArguments}
          toolName={toolCall.name}
          hasResult={hasResult}
          resultSummaryText={resultSummaryText}
          researchFindings={researchFindings}
          researchSummary={researchSummary}
          inspectSummary={inspectSummary}
          extractSummary={extractSummary}
          consoleSummary={consoleSummary}
          processSummary={processSummary}
          resultRenderContext={resultRenderContext}
          resultRenderer={resultRenderer}
          hasStreamLog={hasStreamLog}
          streamLogText={streamLogText}
          hasError={hasError}
          errorText={toolCall.error ?? ''}
          executionTime={executionTime}
        />
      )}
    </div>
  );
};

export default ToolCallRenderer;
