import { V2_BASELINE_VIEWPORTS, V2_WEBSITE_BENCHMARKS, type BaselineViewport } from './phase0-baseline.js';

export type BaselineDesignTarget = {
  benchmarkId: string;
  url: string;
  captureMode?: 'navigate' | 'html';
  viewportQuery?: string;
  projectId?: string;
  documentId?: string;
};

export type BaselineRunDefinition = {
  runId: string;
  sourceVersion: string;
  createdAt: string;
  designs: BaselineDesignTarget[];
};

export type BaselineCaptureTarget = BaselineDesignTarget & {
  viewport: BaselineViewport;
  relativeScreenshotPath: string;
};

function assertSafeSegment(value: string, label: string): void {
  if (!/^[a-z0-9][a-z0-9._-]{0,119}$/i.test(value)) throw new Error(`${label} must be a safe file name segment.`);
}

function assertCaptureUrl(value: string): void {
  const parsed = new URL(value);
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') throw new Error('Baseline capture URLs must use HTTP or HTTPS.');
}

export function validateBaselineRunDefinition(definition: BaselineRunDefinition, requireComplete = true): string[] {
  const errors: string[] = [];
  try { assertSafeSegment(definition.runId, 'runId'); } catch (error) { errors.push((error as Error).message); }
  if (!definition.sourceVersion.trim()) errors.push('sourceVersion is required.');
  if (!Number.isFinite(Date.parse(definition.createdAt))) errors.push('createdAt must be an ISO date.');
  const knownIds = new Set(V2_WEBSITE_BENCHMARKS.map((benchmark) => benchmark.id));
  const seenIds = new Set<string>();
  for (const design of definition.designs) {
    if (!knownIds.has(design.benchmarkId)) errors.push(`Unknown benchmark id: ${design.benchmarkId}`);
    if (seenIds.has(design.benchmarkId)) errors.push(`Duplicate design target: ${design.benchmarkId}`);
    seenIds.add(design.benchmarkId);
    try { assertCaptureUrl(design.url); } catch (error) { errors.push(`${design.benchmarkId}: ${(error as Error).message}`); }
    if (design.captureMode !== undefined && design.captureMode !== 'navigate' && design.captureMode !== 'html') {
      errors.push(`${design.benchmarkId}: captureMode must be navigate or html.`);
    }
    if (design.viewportQuery !== undefined && !/^[A-Za-z][A-Za-z0-9_-]{0,39}$/.test(design.viewportQuery)) {
      errors.push(`${design.benchmarkId}: viewportQuery must be a safe query parameter name.`);
    }
  }
  if (requireComplete) {
    for (const benchmark of V2_WEBSITE_BENCHMARKS) {
      if (!seenIds.has(benchmark.id)) errors.push(`Missing design target: ${benchmark.id}`);
    }
  }
  return errors;
}

export function createBaselineCapturePlan(definition: BaselineRunDefinition, requireComplete = true): BaselineCaptureTarget[] {
  const errors = validateBaselineRunDefinition(definition, requireComplete);
  if (errors.length > 0) throw new Error(`Invalid baseline run:\n${errors.join('\n')}`);
  return definition.designs.flatMap((design) => V2_BASELINE_VIEWPORTS.map((viewport) => ({
    ...design,
    viewport,
    relativeScreenshotPath: `${definition.runId}/${design.benchmarkId}/${viewport.id}-${viewport.width}x${viewport.height}.png`
  })));
}
