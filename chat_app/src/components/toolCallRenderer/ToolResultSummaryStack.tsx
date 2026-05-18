import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { triStateLabel } from './core/labels';
import type {
  ConsoleResultSummary,
  ExtractSummary,
  InspectResultSummary,
  ProcessResultSummary,
  ResearchFindingsSummary,
  ResearchResultSummary,
} from './core/summaries';

interface ToolResultSummaryStackProps {
  researchFindings: ResearchFindingsSummary | null;
  researchSummary: ResearchResultSummary | null;
  inspectSummary: InspectResultSummary | null;
  extractSummary: ExtractSummary | null;
  consoleSummary: ConsoleResultSummary | null;
  processSummary: ProcessResultSummary | null;
}

export const ToolResultSummaryStack: React.FC<ToolResultSummaryStackProps> = ({
  researchFindings,
  researchSummary,
  inspectSummary,
  extractSummary,
  consoleSummary,
  processSummary,
}) => {
  const { t } = useI18n();
  if (!researchFindings && !researchSummary && !inspectSummary && !extractSummary && !consoleSummary && !processSummary) {
    return null;
  }

  return (
    <div className="tool-summary-stack">
      {researchFindings && (
        <div className="tool-summary-card tool-findings-card">
          <div className="tool-summary-title">{t('toolSummary.researchFindings')}</div>
          {researchFindings.answerFrame && (
            <div className="tool-findings-answer">{researchFindings.answerFrame}</div>
          )}

          {researchFindings.pageFindings.length > 0 && (
            <div className="tool-findings-section">
              <div className="tool-findings-section-title">{t('toolSummary.pageFindings')}</div>
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
              <div className="tool-findings-section-title">{t('toolSummary.webFindings')}</div>
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
              <div className="tool-findings-section-title">{t('toolSummary.sourceHighlights')}</div>
              <div className="tool-findings-list">
                {researchFindings.sourceHighlights.map((source, index) => {
                  const label = source.title || source.url || t('toolSummary.untitledSource');
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
              <div className="tool-findings-section-title">{t('toolSummary.recommendedNextSteps')}</div>
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
          <div className="tool-summary-title">{t('toolSummary.researchOverview')}</div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.searchResults')}</span>
            <span className="tool-summary-value">{researchSummary.searchResultCount ?? t('toolSummary.na')}</span>
          </div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.extractedPages')}</span>
            <span className="tool-summary-value">{researchSummary.extractedPageCount ?? t('toolSummary.na')}</span>
          </div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.selectedUrls')}</span>
            <span className="tool-summary-value">{researchSummary.selectedUrlCount ?? t('toolSummary.na')}</span>
          </div>
          {researchSummary.warning && (
            <div className="tool-summary-row">
              <span className="tool-summary-key">{t('toolSummary.warning')}</span>
              <span className="tool-summary-value">{researchSummary.warning}</span>
            </div>
          )}
        </div>
      )}

      {inspectSummary && (
        <div className="tool-summary-card">
          <div className="tool-summary-title">{t('toolSummary.currentPage')}</div>
          {/*
            Prefer a locale-aware fallback label when inspect summary intentionally
            suppresses placeholder URLs such as about:blank.
          */}
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.page')}</span>
            <span className="tool-summary-value">
              {inspectSummary.pageLabel || t('toolSummary.noOpenPage')}
            </span>
          </div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.consoleMessages')}</span>
            <span className="tool-summary-value">{inspectSummary.totalMessages ?? t('toolSummary.na')}</span>
          </div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.jsErrors')}</span>
            <span className="tool-summary-value">{inspectSummary.totalErrors ?? t('toolSummary.na')}</span>
          </div>
          {inspectSummary.warning && (
            <div className="tool-summary-row">
              <span className="tool-summary-key">{t('toolSummary.warning')}</span>
              <span className="tool-summary-value">{inspectSummary.warning}</span>
            </div>
          )}
        </div>
      )}

      {extractSummary && (
        <div className="tool-summary-card">
          <div className="tool-summary-title">{t('toolSummary.extractSummary')}</div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.pages')}</span>
            <span className="tool-summary-value">{extractSummary.pageCount ?? t('toolSummary.na')}</span>
          </div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.truncatedPages')}</span>
            <span className="tool-summary-value">{extractSummary.truncatedPageCount ?? t('toolSummary.na')}</span>
          </div>
        </div>
      )}

      {consoleSummary && (
        <div className="tool-summary-card">
          <div className="tool-summary-title">{t('toolSummary.consoleSummary')}</div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.messages')}</span>
            <span className="tool-summary-value">{consoleSummary.totalMessages ?? t('toolSummary.na')}</span>
          </div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.errors')}</span>
            <span className="tool-summary-value">{consoleSummary.totalErrors ?? t('toolSummary.na')}</span>
          </div>
        </div>
      )}

      {processSummary && (
        <div className="tool-summary-card">
          <div className="tool-summary-title">{t('toolSummary.processSummary')}</div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.status')}</span>
            <span className="tool-summary-value">{processSummary.status}</span>
          </div>
          <div className="tool-summary-row">
            <span className="tool-summary-key">{t('toolSummary.timedOut')}</span>
            <span className="tool-summary-value">
              {t(`toolSummary.${triStateLabel(processSummary.timedOut)}`)}
            </span>
          </div>
          {processSummary.processCount !== null && (
            <div className="tool-summary-row">
              <span className="tool-summary-key">{t('toolSummary.processes')}</span>
              <span className="tool-summary-value">{processSummary.processCount}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
