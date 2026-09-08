import { indexSceneDocument, type SceneDocument } from './scene-schema.js';
import type { SolvedSceneLayout } from './layout-engine.js';
import type { SceneLayoutCalibrationReport } from './layout-calibration.js';
import { SceneQueryIndex } from './scene-query.js';
import { assertDesignBrief, assertDesignSpecMatchesBrief, type DesignBrief, type DesignScope, type DesignSpec } from './design-protocol.js';
import type { SceneTransaction } from './scene-transaction.js';
import { collectSceneSubtreeNodeIds, validateScopedAiTransaction, type ScopedAiTransactionValidation } from './scoped-ai-transaction.js';

export type VisualQualityCriterion = 'semantic-completeness' | 'layout' | 'responsive' | 'rendering' | 'human-protection';
export type VisualQualitySeverity = 'blocker' | 'error' | 'warning';

export interface VisualQualityIssue {
  issueId: string;
  criterion: VisualQualityCriterion;
  severity: VisualQualitySeverity;
  message: string;
  nodeIds: string[];
  viewportWidth?: number;
  autoRepairable: boolean;
  suggestedAction: string;
}

export interface VisualQualityReport {
  reportId: string;
  scope: DesignScope;
  revision: number;
  score: number;
  passed: boolean;
  evaluatedViewportWidths: number[];
  issues: VisualQualityIssue[];
}

export interface VisualQualityArtifacts {
  layouts: SolvedSceneLayout[];
  calibrations?: SceneLayoutCalibrationReport[];
}

export interface VisualRepairRequest {
  requestId: string;
  reportId: string;
  scope: DesignScope;
  baseRevision: number;
  issueIds: string[];
  targetNodeIds: string[];
  protectedNodeIds: string[];
  blockedIssueIds: string[];
  constraints: {
    preserveHumanLocks: true;
    allowOnlyTargetNodesAndDescendants: true;
    requireFreshLayoutAndSnapshotValidation: true;
  };
}

function issueId(parts: Array<string | number>): string {
  return parts.join(':').replace(/[^A-Za-z0-9._:-]+/g, '-').slice(0, 160);
}

function pushIssue(issues: VisualQualityIssue[], issue: VisualQualityIssue): void {
  if (!issues.some((candidate) => candidate.issueId === issue.issueId)) issues.push(issue);
}

function penalty(severity: VisualQualitySeverity): number {
  if (severity === 'blocker') return 24;
  if (severity === 'error') return 10;
  return 3;
}

export function evaluateVisualQuality(
  document: SceneDocument,
  brief: DesignBrief,
  spec: DesignSpec,
  artifacts: VisualQualityArtifacts
): VisualQualityReport {
  assertDesignBrief(brief);
  assertDesignSpecMatchesBrief(spec, brief);
  if (document.documentId !== brief.scope.documentId) throw new Error('Visual quality scope does not match the Scene document.');
  if (!artifacts || !Array.isArray(artifacts.layouts)) throw new Error('Visual quality layouts are required.');
  const layoutsByWidth = new Map(artifacts.layouts.map((layout) => [layout.viewportWidth, layout]));
  if (layoutsByWidth.size !== artifacts.layouts.length) throw new Error('Visual quality layouts contain duplicate viewport widths.');
  const issues: VisualQualityIssue[] = [];
  const query = new SceneQueryIndex(document);

  for (const page of spec.pages) {
    for (const section of page.sections) {
      if (query.query({ pageIds: [document.pages.find((candidate) => candidate.name === page.pageKey)?.id ?? document.pages[0].id], roles: [section.nodeRole] }).length > 0
        || query.query({ roles: [section.nodeRole] }).length > 0) continue;
      pushIssue(issues, {
        issueId: issueId(['semantic', page.pageKey, section.sectionKey]),
        criterion: 'semantic-completeness',
        severity: section.visualPriority === 'primary' ? 'blocker' : 'error',
        message: `Required semantic section ${page.pageKey}/${section.sectionKey} with role ${section.nodeRole} is missing.`,
        nodeIds: [],
        autoRepairable: false,
        suggestedAction: 'Create the missing semantic section through its planned section transaction.'
      });
    }
  }

  for (const viewportWidth of brief.constraints.viewportWidths) {
    const layout = layoutsByWidth.get(viewportWidth);
    if (!layout) {
      pushIssue(issues, {
        issueId: issueId(['responsive', viewportWidth, 'missing-layout']),
        criterion: 'responsive',
        severity: 'blocker',
        message: `No solved layout was provided for viewport ${viewportWidth}.`,
        nodeIds: [], viewportWidth, autoRepairable: false,
        suggestedAction: 'Solve the current Scene at every required viewport before visual review.'
      });
      continue;
    }
    if (layout.documentId !== document.documentId || layout.revision !== document.revision) throw new Error(`Solved layout ${viewportWidth} is stale or belongs to another document.`);
    for (const diagnostic of layout.diagnostics) {
      const severity: VisualQualitySeverity = diagnostic.severity === 'error' ? 'blocker' : diagnostic.code === 'overflow-x' ? 'error' : 'warning';
      pushIssue(issues, {
        issueId: issueId(['layout', viewportWidth, diagnostic.nodeId, diagnostic.code]),
        criterion: diagnostic.code.startsWith('overflow') ? 'responsive' : 'layout',
        severity,
        message: diagnostic.message,
        nodeIds: [diagnostic.nodeId],
        viewportWidth,
        autoRepairable: true,
        suggestedAction: diagnostic.code === 'overflow-x'
          ? 'Adjust layout intent, wrapping, sizing, or responsive rules without changing unrelated nodes.'
          : 'Correct the targeted layout rule and re-solve every required viewport.'
      });
    }
  }

  for (const calibration of artifacts.calibrations ?? []) {
    if (!brief.constraints.viewportWidths.includes(calibration.viewportWidth)) throw new Error(`Calibration viewport ${calibration.viewportWidth} is outside the brief.`);
    for (const calibrationIssue of calibration.issues) {
      if (calibrationIssue.severity !== 'error') continue;
      pushIssue(issues, {
        issueId: issueId(['render', calibration.viewportWidth, calibrationIssue.nodeId, calibrationIssue.code]),
        criterion: 'rendering',
        severity: calibrationIssue.code === 'missing-rendered-node' ? 'blocker' : 'error',
        message: `Browser calibration reported ${calibrationIssue.code} for ${calibrationIssue.nodeId}.`,
        nodeIds: [calibrationIssue.nodeId],
        viewportWidth: calibration.viewportWidth,
        autoRepairable: calibrationIssue.code !== 'missing-rendered-node',
        suggestedAction: 'Reconcile the targeted node with browser rendering, then capture and calibrate again.'
      });
    }
  }

  const score = Math.max(0, 100 - issues.reduce((total, issue) => total + penalty(issue.severity), 0));
  return {
    reportId: `quality:${document.documentId}:${document.revision}`,
    scope: structuredClone(brief.scope),
    revision: document.revision,
    score,
    passed: issues.every((issue) => issue.severity === 'warning'),
    evaluatedViewportWidths: brief.constraints.viewportWidths.filter((width) => layoutsByWidth.has(width)),
    issues
  };
}

export function createVisualRepairRequest(report: VisualQualityReport, document: SceneDocument): VisualRepairRequest {
  if (report.scope.documentId !== document.documentId || report.revision !== document.revision) throw new Error('Visual quality report is stale or belongs to another Scene document.');
  const index = indexSceneDocument(document);
  const targetNodeIds = new Set<string>();
  const protectedNodeIds = new Set<string>();
  const blockedIssueIds: string[] = [];
  const issueIds: string[] = [];
  for (const issue of report.issues) {
    if (!issue.autoRepairable || issue.nodeIds.length === 0) {
      blockedIssueIds.push(issue.issueId);
      continue;
    }
    let blocked = false;
    for (const nodeId of issue.nodeIds) {
      const node = index.get(nodeId)?.node;
      if (!node) {
        blocked = true;
        continue;
      }
      if (node.locked || !node.aiPolicy.editable || node.aiPolicy.lockedFields.length > 0) {
        protectedNodeIds.add(nodeId);
        blocked = true;
      }
    }
    if (blocked) {
      blockedIssueIds.push(issue.issueId);
      continue;
    }
    issueIds.push(issue.issueId);
    for (const nodeId of issue.nodeIds) targetNodeIds.add(nodeId);
  }
  return {
    requestId: `repair:${report.reportId}`,
    reportId: report.reportId,
    scope: structuredClone(report.scope),
    baseRevision: report.revision,
    issueIds,
    targetNodeIds: [...targetNodeIds],
    protectedNodeIds: [...protectedNodeIds],
    blockedIssueIds,
    constraints: {
      preserveHumanLocks: true,
      allowOnlyTargetNodesAndDescendants: true,
      requireFreshLayoutAndSnapshotValidation: true
    }
  };
}

export function validateVisualRepairTransaction(
  request: VisualRepairRequest,
  report: VisualQualityReport,
  document: SceneDocument,
  transaction: SceneTransaction
): ScopedAiTransactionValidation {
  if (request.reportId !== report.reportId || request.baseRevision !== report.revision) throw new Error('Visual repair request does not match its quality report.');
  if (request.scope.projectId !== report.scope.projectId || request.scope.documentId !== report.scope.documentId) throw new Error('Visual repair request scope does not match its quality report.');
  if (document.documentId !== request.scope.documentId || document.revision !== request.baseRevision) throw new Error('Visual repair request is stale or belongs to another Scene document.');
  if (request.targetNodeIds.length === 0) throw new Error('Visual repair request has no repairable target nodes.');
  const allowedExistingNodeIds = collectSceneSubtreeNodeIds(document, request.targetNodeIds);
  return validateScopedAiTransaction(document, transaction, {
    baseRevision: request.baseRevision,
    allowedExistingNodeIds,
    targetRootNodeIds: request.targetNodeIds,
    dependencyNodeIds: [],
    preserveTargetRoots: true,
    forbiddenPatchRoots: ['annotations', 'aiPolicy', 'locked']
  });
}
