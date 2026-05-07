import React from 'react';

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
  if (!researchFindings && !researchSummary && !inspectSummary && !extractSummary && !consoleSummary && !processSummary) {
    return null;
  }

  return (
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
  );
};
