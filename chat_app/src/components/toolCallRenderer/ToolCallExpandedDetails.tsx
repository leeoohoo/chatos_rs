import React from 'react';

import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
import { ToolArgumentsDetails } from '../ToolArgumentsDetails';
import { SectionIcon } from './ToolCallIcons';
import { ToolCallResultPanel } from './ToolCallResultPanel';
import type {
  ConsoleResultSummary,
  ExtractSummary,
  InspectResultSummary,
  ProcessResultSummary,
  ResearchFindingsSummary,
  ResearchResultSummary,
} from './core/summaries';
import type { ToolResultRenderContext, ToolResultRenderer } from './registry';

interface ToolCallExpandedDetailsProps {
  hasArguments: boolean;
  parsedArguments: unknown;
  toolName: string;
  hasResult: boolean;
  resultSummaryText: string;
  researchFindings: ResearchFindingsSummary | null;
  researchSummary: ResearchResultSummary | null;
  inspectSummary: InspectResultSummary | null;
  extractSummary: ExtractSummary | null;
  consoleSummary: ConsoleResultSummary | null;
  processSummary: ProcessResultSummary | null;
  resultRenderContext: ToolResultRenderContext;
  resultRenderer: ToolResultRenderer;
  hasStreamLog: boolean;
  streamLogText: string;
  hasError: boolean;
  errorText: string;
  executionTime: string;
}

export const ToolCallExpandedDetails: React.FC<ToolCallExpandedDetailsProps> = ({
  hasArguments,
  parsedArguments,
  toolName,
  hasResult,
  resultSummaryText,
  researchFindings,
  researchSummary,
  inspectSummary,
  extractSummary,
  consoleSummary,
  processSummary,
  resultRenderContext,
  resultRenderer,
  hasStreamLog,
  streamLogText,
  hasError,
  errorText,
  executionTime,
}) => (
  <div className="details-container">
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
          rawToolName={toolName}
        />
      </section>
    )}

    {hasResult && (
      <ToolCallResultPanel
        resultSummaryText={resultSummaryText}
        researchFindings={researchFindings}
        researchSummary={researchSummary}
        inspectSummary={inspectSummary}
        extractSummary={extractSummary}
        consoleSummary={consoleSummary}
        processSummary={processSummary}
        renderContext={resultRenderContext}
        resultRenderer={resultRenderer}
      />
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

    {hasError && (
      <section className="tool-panel-section">
        <div className="details-title">
          <span className="tool-section-icon">
            <SectionIcon kind="error" />
          </span>
          <span className="tool-section-label">错误</span>
          <span className="tool-section-subtitle">需要重点处理的失败信息</span>
        </div>
        <div className="tool-error-box">{errorText}</div>
      </section>
    )}

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
);
