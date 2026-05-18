import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
import { ToolResultSummaryStack } from './ToolResultSummaryStack';
import { SectionIcon } from './ToolCallIcons';
import type {
  ConsoleResultSummary,
  ExtractSummary,
  InspectResultSummary,
  ProcessResultSummary,
  ResearchFindingsSummary,
  ResearchResultSummary,
} from './core/summaries';
import type { ToolResultRenderContext, ToolResultRenderer } from './registry';

interface ToolCallResultPanelProps {
  resultSummaryText: string;
  researchFindings: ResearchFindingsSummary | null;
  researchSummary: ResearchResultSummary | null;
  inspectSummary: InspectResultSummary | null;
  extractSummary: ExtractSummary | null;
  consoleSummary: ConsoleResultSummary | null;
  processSummary: ProcessResultSummary | null;
  renderContext: ToolResultRenderContext;
  resultRenderer: ToolResultRenderer;
}

export const ToolCallResultPanel: React.FC<ToolCallResultPanelProps> = ({
  resultSummaryText,
  researchFindings,
  researchSummary,
  inspectSummary,
  extractSummary,
  consoleSummary,
  processSummary,
  renderContext,
  resultRenderer,
}) => {
  const { t } = useI18n();

  return (
    <section className="tool-panel-section">
    <div className="details-title">
      <span className="tool-section-icon">
        <SectionIcon kind="result" />
      </span>
      <span className="tool-section-label">{t('toolPanel.result')}</span>
      <span className="tool-section-subtitle">{t('toolPanel.resultHelp')}</span>
    </div>
    {resultSummaryText && (
      <div className="tool-rich-note tool-rich-note--summary">
        <LazyMarkdownRenderer content={resultSummaryText} />
      </div>
    )}
    <ToolResultSummaryStack
      researchFindings={researchFindings}
      researchSummary={researchSummary}
      inspectSummary={inspectSummary}
      extractSummary={extractSummary}
      consoleSummary={consoleSummary}
      processSummary={processSummary}
    />
    {resultRenderer.render(renderContext)}
  </section>
  );
};
