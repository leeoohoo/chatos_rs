import { resolveToolFamily } from '../../../lib/tools/catalog';
import { asArray, asBoolean, asNumber, asRecord, asString, asStringList } from './value';
import { isMeaningfulBrowserPageUrl } from './labels';
import { toolNameMatches } from './toolName';

export interface ExtractSummary {
  pageCount: number | null;
  truncatedPageCount: number | null;
  totalOriginalChars: number | null;
  totalReturnedChars: number | null;
  totalOmittedChars: number | null;
}

export interface ResearchResultSummary {
  searchBackend: string;
  extractBackend: string;
  searchResultCount: number | null;
  extractedPageCount: number | null;
  selectedUrlCount: number | null;
  totalOmittedChars: number | null;
  warning: string;
}

export interface ResearchSourceHighlight {
  kind: string;
  title: string;
  url: string;
  status: string;
  note: string;
}

export interface ResearchFindingsSummary {
  answerFrame: string;
  pageFindings: string[];
  webFindings: string[];
  sourceHighlights: ResearchSourceHighlight[];
  recommendedNextSteps: string[];
}

export interface InspectResultSummary {
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

export interface ProcessResultSummary {
  terminalId: string;
  processId: string;
  status: string;
  busy: boolean | null;
  completed: boolean | null;
  timedOut: boolean | null;
  processCount: number | null;
}

export interface ConsoleResultSummary {
  totalMessages: number | null;
  totalErrors: number | null;
  clearApplied: boolean | null;
  logCount: number | null;
  warnCount: number | null;
  errorCount: number | null;
}

export const buildExtractSummary = (
  parsedResult: Record<string, unknown> | null,
): ExtractSummary | null => {
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
};

export const buildResearchFindings = (
  parsedResult: Record<string, unknown> | null,
): ResearchFindingsSummary | null => {
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
};

export const buildResearchSummary = (
  parsedResult: Record<string, unknown> | null,
): ResearchResultSummary | null => {
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
};

export const buildInspectSummary = (
  parsedResult: Record<string, unknown> | null,
  toolName: string,
): InspectResultSummary | null => {
  if (
    !toolNameMatches(toolName, 'browser_inspect')
    && !toolNameMatches(toolName, 'browser_research')
  ) {
    return null;
  }
  const rawRecord = asRecord(parsedResult);
  if (!rawRecord) return null;
  const record = toolNameMatches(toolName, 'browser_research')
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
    : (title || url || '');

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
    pageLabel,
    elementCount,
    snapshotStatus: snapshotStatus || 'unknown',
    consoleStatus: consoleStatus || 'unknown',
    visionStatus: visionStatus || 'unknown',
    totalMessages,
    totalErrors,
    pageStateAvailable,
    warning,
  };
};

export const buildProcessSummary = (
  parsedResult: Record<string, unknown> | null,
  toolName: string,
  displayToolName: string,
): ProcessResultSummary | null => {
  if (resolveToolFamily(toolName, displayToolName) !== 'process') {
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
};

export const buildConsoleSummary = (
  parsedResult: Record<string, unknown> | null,
): ConsoleResultSummary | null => {
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
};

export const getResultSummaryText = (
  parsedResult: Record<string, unknown> | null,
): string => {
  const record = asRecord(parsedResult);
  const summary = record ? asString(record._summary_text ?? record.summary_text ?? record.summaryText) : '';
  return summary.trim();
};
