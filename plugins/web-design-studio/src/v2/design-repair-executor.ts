import { diffSceneDocuments, type SceneDocumentDiff } from './scene-diff.js';
import type { SceneDocument } from './scene-schema.js';
import type { SceneTransaction, SceneTransactionSummary } from './scene-transaction.js';
import type { DesignBrief, DesignScope, DesignSpec } from './design-protocol.js';
import type { SceneLayoutCalibrationReport } from './layout-calibration.js';
import type { SolvedSceneLayout } from './layout-engine.js';
import {
  evaluateVisualQuality,
  validateVisualRepairTransaction,
  type VisualQualityReport,
  type VisualRepairRequest
} from './visual-quality.js';
import type { ScopedAiTransactionValidation } from './scoped-ai-transaction.js';

export interface DesignRepairRepository {
  read(documentId: string): Promise<SceneDocument>;
  apply(documentId: string, transaction: SceneTransaction): Promise<{ document: SceneDocument; summary: SceneTransactionSummary }>;
}

export interface DesignRepairSnapshot {
  viewportWidth: number;
  snapshotId: string;
  sha256?: string;
}

export interface DesignRepairVerification {
  layouts: SolvedSceneLayout[];
  snapshots: DesignRepairSnapshot[];
  calibrations: SceneLayoutCalibrationReport[];
}

export interface DesignRepairHandlers {
  createTransaction(
    request: VisualRepairRequest,
    report: VisualQualityReport,
    document: SceneDocument,
    scope: DesignScope
  ): Promise<SceneTransaction> | SceneTransaction;
  verify(
    document: SceneDocument,
    viewportWidths: number[],
    scope: DesignScope
  ): Promise<DesignRepairVerification> | DesignRepairVerification;
}

export interface DesignRepairRun {
  requestId: string;
  scope: DesignScope;
  status: 'completed' | 'failed-quality';
  baseRevision: number;
  finalRevision: number;
  transactionSummary: SceneTransactionSummary;
  scopeValidation: ScopedAiTransactionValidation;
  diff: SceneDocumentDiff;
  snapshots: DesignRepairSnapshot[];
  qualityReport: VisualQualityReport;
}

function assertViewportCoverage(expected: number[], actual: Array<{ viewportWidth: number }>, label: string): void {
  if (!Array.isArray(actual)) throw new Error(`${label} must return a viewport artifact list.`);
  const widths = actual.map((artifact) => artifact.viewportWidth);
  if (widths.length !== expected.length || new Set(widths).size !== widths.length || expected.some((width) => !widths.includes(width))) {
    throw new Error(`${label} must return exactly one fresh artifact for every required viewport.`);
  }
}

export async function executeDesignRepair(
  request: VisualRepairRequest,
  report: VisualQualityReport,
  brief: DesignBrief,
  spec: DesignSpec,
  repository: DesignRepairRepository,
  handlers: DesignRepairHandlers
): Promise<DesignRepairRun> {
  if (request.scope.projectId !== brief.scope.projectId || request.scope.documentId !== brief.scope.documentId
    || request.scope.projectId !== spec.scope.projectId || request.scope.documentId !== spec.scope.documentId) {
    throw new Error('Design repair scope must exactly preserve projectId and documentId through brief, spec, report, and request.');
  }
  const before = await repository.read(request.scope.documentId);
  const transaction = await handlers.createTransaction(request, report, structuredClone(before), structuredClone(request.scope));
  const scopeValidation = validateVisualRepairTransaction(request, report, before, transaction);
  const applied = await repository.apply(request.scope.documentId, transaction);
  const verification = await handlers.verify(
    structuredClone(applied.document),
    [...brief.constraints.viewportWidths],
    structuredClone(request.scope)
  );
  assertViewportCoverage(brief.constraints.viewportWidths, verification.layouts, 'repair layouts');
  assertViewportCoverage(brief.constraints.viewportWidths, verification.snapshots, 'repair snapshots');
  assertViewportCoverage(brief.constraints.viewportWidths, verification.calibrations, 'repair calibrations');
  if (verification.snapshots.some((snapshot) => !snapshot.snapshotId)) throw new Error('Every repair snapshot needs a stable snapshotId.');
  if (verification.calibrations.some((calibration) => !calibration.passed)) throw new Error('Repair browser calibration must pass at every required viewport.');
  const qualityReport = evaluateVisualQuality(applied.document, brief, spec, {
    layouts: verification.layouts,
    calibrations: verification.calibrations
  });
  return {
    requestId: request.requestId,
    scope: structuredClone(request.scope),
    status: qualityReport.passed ? 'completed' : 'failed-quality',
    baseRevision: request.baseRevision,
    finalRevision: applied.document.revision,
    transactionSummary: applied.summary,
    scopeValidation,
    diff: diffSceneDocuments(before, applied.document),
    snapshots: structuredClone(verification.snapshots),
    qualityReport
  };
}
