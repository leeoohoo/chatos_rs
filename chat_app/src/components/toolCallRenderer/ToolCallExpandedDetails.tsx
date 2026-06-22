import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
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
}) => {
  const { t } = useI18n();

  return (
    <div className="details-container">
    {hasArguments && parsedArguments !== null && (
      <section className="tool-panel-section">
        <div className="details-title">
          <span className="tool-section-icon">
            <SectionIcon kind="input" />
          </span>
          <span className="tool-section-label">{t('toolPanel.input')}</span>
          <span className="tool-section-subtitle">{t('toolPanel.inputHelp')}</span>
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
          <span className="tool-section-label">{t('toolPanel.streaming')}</span>
          <span className="tool-section-subtitle">{t('toolPanel.streamingHelp')}</span>
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
          <span className="tool-section-label">{t('toolPanel.error')}</span>
          <span className="tool-section-subtitle">{t('toolPanel.errorHelp')}</span>
        </div>
        <div className="tool-error-box">{errorText}</div>
      </section>
    )}

    <div className="tool-footer">
      <div className="tool-footer-item">
        <span className="tool-section-icon">
          <SectionIcon kind="meta" />
        </span>
        <span className="tool-footer-label">{t('toolPanel.executionTime')}</span>
        <span className="tool-footer-value">{executionTime}</span>
      </div>
    </div>
  </div>
  );
};
