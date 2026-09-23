import { createHash, randomUUID } from 'node:crypto';
import { PNG } from 'pngjs';
import { calibrateSceneLayout } from './layout-calibration.js';
import { solveSceneLayout, type SceneLayoutDiagnostic, type SolvedSceneLayout } from './layout-engine.js';
import { resolveResponsiveScene } from './responsive-scene.js';
import { renderSceneDocumentRoot } from './scene-html-renderer.js';
import {
  indexSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneDocument,
  type SceneNode,
  type SceneRect
} from './scene-schema.js';
import type { SceneDocumentStore } from './scene-store.js';
import type { GenerationArtifact, GenerationScope } from './generation-plan-schema.js';
import type { GenerationPageRun, GenerationStep } from './generation-plan-schema.js';
import {
  GenerationVisualArtifactStore,
  type GenerationVisualArtifactRecord,
  type VisualGroundingEntry
} from './generation-visual-artifact-store.js';
import type { SceneImageRenderer } from './headless-scene-renderer.js';

export interface GenerationVisualServiceOptions {
  projectId: string;
  scenes: SceneDocumentStore;
  artifacts: GenerationVisualArtifactStore;
  renderer: SceneImageRenderer;
  assertDocumentInScope(documentId: string): Promise<void>;
}

export interface ToolImagePayload {
  label: string;
  data: string;
  mimeType: 'image/png';
}

export type VisualToolResult = Record<string, unknown> & { __images: ToolImagePayload[] };

export interface CandidateVisualVerificationInput {
  scope: GenerationScope;
  page: GenerationPageRun;
  step: GenerationStep;
  baseDocument: SceneDocument;
  candidateDocument: SceneDocument;
  visualInputs: GenerationArtifact[];
}

export interface CandidateVisualVerificationResult {
  passed: boolean;
  qualitySummary: string;
  issueIds: string[];
  artifacts: GenerationArtifact[];
  error?: {
    code: 'layout_error' | 'render_error' | 'quality_reject';
    message: string;
    retryable: boolean;
    issueIds: string[];
  };
  __images: ToolImagePayload[];
}

function requireIdentifier(value: string, label: string): string {
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/.test(value)) throw new Error(`${label} is invalid.`);
  return value;
}

function scopeFor(projectId: string, documentId: string): GenerationScope {
  return { projectId: requireIdentifier(projectId, 'runtime projectId'), documentId: requireIdentifier(documentId, 'documentId') };
}

function rootForPage(document: SceneDocument, pageId: string): string {
  const page = document.pages.find((candidate) => candidate.id === pageId);
  if (!page) throw new Error(`Scene page not found: ${pageId}`);
  const root = page.children.find((node) => node.role === 'page-root') ?? page.children[0];
  if (!root) throw new Error(`Scene page ${pageId} has no root frame to capture.`);
  return root.id;
}

function intersects(left: SceneRect, right: SceneRect): boolean {
  return left.x < right.x + right.width && left.x + left.width > right.x
    && left.y < right.y + right.height && left.y + left.height > right.y;
}

function directChildren(node: SceneNode): SceneNode[] {
  if (isSceneContainer(node)) return node.children;
  if (isSceneSlotContainer(node)) return Object.values(node.slots).flat();
  return [];
}

function outsideDistance(inner: SceneRect, outer: SceneRect): { x: number; y: number } {
  return {
    x: Math.max(0, outer.x - inner.x, inner.x + inner.width - outer.x - outer.width),
    y: Math.max(0, outer.y - inner.y, inner.y + inner.height - outer.y - outer.height)
  };
}

function overlapSize(left: SceneRect, right: SceneRect): { x: number; y: number } {
  return {
    x: Math.max(0, Math.min(left.x + left.width, right.x + right.width) - Math.max(left.x, right.x)),
    y: Math.max(0, Math.min(left.y + left.height, right.y + right.height) - Math.max(left.y, right.y))
  };
}

/**
 * Browser calibration proves that the renderer followed the solved geometry. It does not prove
 * that the geometry itself is usable. This pass rejects flow content that escapes its container
 * and siblings that occupy the same visual space, which are especially easy to miss after a
 * responsive wrap changes a one-row desktop composition into several mobile rows.
 */
export function inspectSceneLayoutIntegrity(
  document: SceneDocument,
  solved: SolvedSceneLayout,
  tolerance = 2
): SceneLayoutDiagnostic[] {
  const responsiveDocument = resolveResponsiveScene(document, solved.viewportWidth).document;
  const index = indexSceneDocument(responsiveDocument);
  const issues: SceneLayoutDiagnostic[] = [];

  for (const { node: parent } of index.values()) {
    const parentBox = solved.boxes.get(parent.id);
    if (!parentBox) continue;
    const children = directChildren(parent).filter((child) => child.visible && solved.boxes.has(child.id));
    if (children.length === 0) continue;

    const contentBox: SceneRect = {
      x: parentBox.x + parent.layout.padding.left,
      y: parentBox.y + parent.layout.padding.top,
      width: Math.max(0, parentBox.width - parent.layout.padding.left - parent.layout.padding.right),
      height: Math.max(0, parentBox.height - parent.layout.padding.top - parent.layout.padding.bottom)
    };
    const containmentCandidates = parent.layout.clipContent
      ? children
      : parent.layout.mode === 'auto' || parent.layout.mode === 'grid'
        ? children.filter((child) => child.layout.position === 'flow')
        : [];
    const containmentTolerance = parent.layout.clipContent ? tolerance : Math.max(8, tolerance);
    for (const child of containmentCandidates) {
      const childBox = solved.boxes.get(child.id)!;
      const outside = outsideDistance(childBox, contentBox);
      if (outside.x <= containmentTolerance && outside.y <= containmentTolerance) continue;
      issues.push({
        code: 'child-outside-container',
        nodeId: child.id,
        relatedNodeId: parent.id,
        severity: 'error',
        message: `${child.id} extends outside ${parent.id} by ${outside.x.toFixed(1)}px horizontally and ${outside.y.toFixed(1)}px vertically.`
      });
    }

    if (parent.layout.mode !== 'auto' && parent.layout.mode !== 'grid') continue;
    const flowChildren = children.filter((child) => child.layout.position === 'flow');
    for (let leftIndex = 0; leftIndex < flowChildren.length; leftIndex += 1) {
      for (let rightIndex = leftIndex + 1; rightIndex < flowChildren.length; rightIndex += 1) {
        const left = flowChildren[leftIndex];
        const right = flowChildren[rightIndex];
        const overlap = overlapSize(solved.boxes.get(left.id)!, solved.boxes.get(right.id)!);
        if (overlap.x <= tolerance || overlap.y <= tolerance) continue;
        issues.push({
          code: 'flow-overlap',
          nodeId: left.id,
          relatedNodeId: right.id,
          severity: 'error',
          message: `${left.id} overlaps ${right.id} by ${overlap.x.toFixed(1)}×${overlap.y.toFixed(1)}px inside ${parent.id}.`
        });
      }
    }
  }
  return issues;
}

function clampCrop(crop: SceneRect, width: number, height: number): SceneRect {
  const x = Math.max(0, Math.min(width, crop.x));
  const y = Math.max(0, Math.min(height, crop.y));
  const right = Math.max(x, Math.min(width, crop.x + crop.width));
  const bottom = Math.max(y, Math.min(height, crop.y + crop.height));
  if (right - x < 1 || bottom - y < 1) throw new Error('The requested region is outside the rendered page.');
  return { x, y, width: right - x, height: bottom - y };
}

function sha256(data: Buffer): string {
  return createHash('sha256').update(data).digest('hex');
}

function artifact(kind: GenerationArtifact['kind'], revision: number, viewportWidth: number, createdAt: string, metadata: Record<string, string | number | boolean>, image?: Buffer): GenerationArtifact {
  const artifactId = `${kind}:${randomUUID()}`;
  return {
    artifactId,
    kind,
    revision,
    viewportWidth,
    uri: `web-design-artifact://${artifactId}`,
    ...(image ? { sha256: sha256(image) } : {}),
    metadata,
    createdAt
  };
}

function groundingFromMeasurements(document: SceneDocument, measurements: Record<string, { rect: SceneRect }>, crop?: SceneRect): VisualGroundingEntry[] {
  const index = indexSceneDocument(document);
  const entries: VisualGroundingEntry[] = [];
  for (const [nodeId, measurement] of Object.entries(measurements)) {
    const scene = index.get(nodeId);
    if (!scene) continue;
    if (crop && !intersects(measurement.rect, crop)) continue;
    entries.push({
      nodeId,
      parentId: scene.parentId,
      pageId: scene.pageId,
      rect: crop
        ? { x: measurement.rect.x - crop.x, y: measurement.rect.y - crop.y, width: measurement.rect.width, height: measurement.rect.height }
        : structuredClone(measurement.rect),
      depth: scene.path.length
    });
  }
  return entries.sort((left, right) => left.depth - right.depth || left.nodeId.localeCompare(right.nodeId));
}

function publicRecord(record: GenerationVisualArtifactRecord): Record<string, unknown> {
  return {
    artifact: record.artifact,
    pageId: record.pageId,
    rootNodeId: record.rootNodeId,
    width: record.width,
    height: record.height,
    ...(record.snapshotArtifactId ? { snapshotArtifactId: record.snapshotArtifactId } : {}),
    ...(record.crop ? { crop: record.crop } : {}),
    groundingCount: record.grounding?.length ?? 0
  };
}

function pointCandidates(record: GenerationVisualArtifactRecord, x: number, y: number): VisualGroundingEntry[] {
  if (!Number.isFinite(x) || !Number.isFinite(y)) throw new Error('Inspection coordinates are invalid.');
  return (record.grounding ?? [])
    .filter((entry) => x >= entry.rect.x && x <= entry.rect.x + entry.rect.width && y >= entry.rect.y && y <= entry.rect.y + entry.rect.height)
    .sort((left, right) => {
      const leftArea = left.rect.width * left.rect.height;
      const rightArea = right.rect.width * right.rect.height;
      return leftArea - rightArea || right.depth - left.depth;
    });
}

function diffPng(beforeData: Buffer, afterData: Buffer): { png: Buffer; changedPixels: number; ratio: number; region?: SceneRect } {
  const before = PNG.sync.read(beforeData);
  const after = PNG.sync.read(afterData);
  if (before.width !== after.width) throw new Error('Snapshot comparison requires the same viewport width.');
  const outputHeight = Math.max(before.height, after.height);
  const output = new PNG({ width: before.width, height: outputHeight });
  let changedPixels = 0;
  let minX = before.width;
  let minY = outputHeight;
  let maxX = -1;
  let maxY = -1;
  const channel = (png: PNG, x: number, y: number, offset: number): number => y < png.height
    ? png.data[(y * png.width + x) * 4 + offset]
    : offset === 3 ? 0 : 255;
  for (let y = 0; y < outputHeight; y += 1) {
    for (let x = 0; x < before.width; x += 1) {
      const offset = (y * before.width + x) * 4;
      const delta = Math.max(
        Math.abs(channel(before, x, y, 0) - channel(after, x, y, 0)),
        Math.abs(channel(before, x, y, 1) - channel(after, x, y, 1)),
        Math.abs(channel(before, x, y, 2) - channel(after, x, y, 2)),
        Math.abs(channel(before, x, y, 3) - channel(after, x, y, 3))
      );
      if (delta > 12) {
        changedPixels += 1;
        minX = Math.min(minX, x); minY = Math.min(minY, y); maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
        output.data[offset] = 255; output.data[offset + 1] = 45; output.data[offset + 2] = 120; output.data[offset + 3] = 255;
      } else {
        output.data[offset] = Math.round(channel(after, x, y, 0) * 0.25 + 190);
        output.data[offset + 1] = Math.round(channel(after, x, y, 1) * 0.25 + 190);
        output.data[offset + 2] = Math.round(channel(after, x, y, 2) * 0.25 + 190);
        output.data[offset + 3] = 255;
      }
    }
  }
  return {
    png: PNG.sync.write(output),
    changedPixels,
    ratio: changedPixels / (before.width * outputHeight),
    ...(changedPixels > 0 ? { region: { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 } } : {})
  };
}

export class GenerationVisualService {
  constructor(private readonly options: GenerationVisualServiceOptions) {
    requireIdentifier(options.projectId, 'runtime projectId');
  }

  private async scene(documentId: string): Promise<{ scope: GenerationScope; document: SceneDocument }> {
    await this.options.assertDocumentInScope(documentId);
    const scope = scopeFor(this.options.projectId, documentId);
    return { scope, document: await this.options.scenes.read(documentId) };
  }

  private async captureScene(
    scope: GenerationScope,
    document: SceneDocument,
    pageId: string,
    viewportWidth: number,
    requestedCrop?: SceneRect
  ): Promise<VisualToolResult> {
    if (!Number.isSafeInteger(viewportWidth) || viewportWidth < 240 || viewportWidth > 10000) throw new Error('viewportWidth is invalid.');
    const rootNodeId = rootForPage(document, pageId);
    const rendered = renderSceneDocumentRoot(document, rootNodeId, viewportWidth);
    const crop = requestedCrop ? clampCrop(requestedCrop, rendered.width, rendered.height) : undefined;
    const captured = await this.options.renderer.capture({
      html: rendered.documentHtml,
      width: rendered.width,
      height: rendered.height,
      ...(crop ? { clip: crop } : {})
    });
    const solved = solveSceneLayout(document, { rootNodeId, viewportWidth });
    const calibration = calibrateSceneLayout(solved, captured.measurements);
    const integrityIssues = inspectSceneLayoutIntegrity(document, solved);
    const diagnostics = [...rendered.diagnostics, ...integrityIssues];
    const grounding = groundingFromMeasurements(document, captured.measurements, crop);
    const createdAt = new Date().toISOString();
    const snapshot = artifact(crop ? 'region-crop' : 'page-snapshot', document.revision, viewportWidth, createdAt, {
      pageId, rootNodeId, width: captured.width, height: captured.height, calibrationPassed: calibration.passed
    }, captured.png);
    const groundingArtifact = artifact('visual-grounding', document.revision, viewportWidth, createdAt, {
      pageId, rootNodeId, snapshotArtifactId: snapshot.artifactId, nodeCount: grounding.length
    });
    const layoutArtifact = artifact('layout', document.revision, viewportWidth, createdAt, {
      pageId, rootNodeId, diagnosticCount: diagnostics.length, integrityIssueCount: integrityIssues.length
    });
    const calibrationArtifact = artifact('calibration', document.revision, viewportWidth, createdAt, {
      pageId, rootNodeId, passed: calibration.passed, issueCount: calibration.issues.length
    });
    const common = { schemaVersion: 1 as const, scope, pageId, rootNodeId, width: captured.width, height: captured.height, createdAt };
    const snapshotRecord = await this.options.artifacts.create({
      ...common, artifact: snapshot, ...(crop ? { crop } : {}), grounding, calibration, diagnostics
    }, captured.png);
    await this.options.artifacts.create({ ...common, artifact: groundingArtifact, snapshotArtifactId: snapshot.artifactId, ...(crop ? { crop } : {}), grounding });
    await this.options.artifacts.create({ ...common, artifact: layoutArtifact, snapshotArtifactId: snapshot.artifactId, diagnostics });
    await this.options.artifacts.create({ ...common, artifact: calibrationArtifact, snapshotArtifactId: snapshot.artifactId, calibration });
    return {
      capture: publicRecord(snapshotRecord),
      artifacts: [snapshot, groundingArtifact, layoutArtifact, calibrationArtifact],
      calibration,
      diagnostics,
      __images: [{ label: crop ? 'region' : 'page', data: captured.png.toString('base64'), mimeType: 'image/png' }]
    };
  }

  async capturePage(documentId: string, pageId: string, viewportWidth: number): Promise<VisualToolResult> {
    const { scope, document } = await this.scene(documentId);
    return this.captureScene(scope, document, pageId, viewportWidth);
  }

  async captureRegion(input: { documentId: string; pageId: string; viewportWidth: number; nodeId?: string; rect?: SceneRect; padding?: number }): Promise<VisualToolResult> {
    const { scope, document } = await this.scene(input.documentId);
    const rootNodeId = rootForPage(document, input.pageId);
    const solved = solveSceneLayout(document, { rootNodeId, viewportWidth: input.viewportWidth });
    let region = input.rect ? structuredClone(input.rect) : input.nodeId ? solved.boxes.get(input.nodeId) : undefined;
    if (!region) throw new Error(input.nodeId ? `Scene node is not visible at this viewport: ${input.nodeId}` : 'capture_region requires nodeId or rect.');
    const padding = input.padding ?? 16;
    if (!Number.isFinite(padding) || padding < 0 || padding > 1000) throw new Error('Region padding is invalid.');
    region = { x: region.x - padding, y: region.y - padding, width: region.width + padding * 2, height: region.height + padding * 2 };
    return this.captureScene(scope, document, input.pageId, input.viewportWidth, region);
  }

  async verifyCandidate(input: CandidateVisualVerificationInput): Promise<CandidateVisualVerificationResult> {
    await this.options.assertDocumentInScope(input.scope.documentId);
    const expectedScope = scopeFor(this.options.projectId, input.scope.documentId);
    if (input.scope.projectId !== expectedScope.projectId || input.candidateDocument.documentId !== input.scope.documentId) {
      throw new Error('Candidate visual verification does not match the active scope.');
    }
    if (input.candidateDocument.revision !== input.baseDocument.revision + 1) {
      throw new Error('Candidate visual verification requires the next Scene revision.');
    }
    const viewportWidths = input.step.target.viewportWidths.length > 0
      ? [...new Set(input.step.target.viewportWidths)]
      : [...new Set(input.visualInputs.flatMap((item) => item.viewportWidth === undefined ? [] : [item.viewportWidth]))];
    if (viewportWidths.length === 0) throw new Error('Candidate visual verification needs at least one viewport.');

    const artifacts: GenerationArtifact[] = [];
    const images: ToolImagePayload[] = [];
    const issueIds: string[] = [];
    const captures: Array<{ rootNodeId: string; width: number; height: number }> = [];
    let layoutFailed = false;
    let visibleChangeFailed = false;

    for (const viewportWidth of viewportWidths) {
      const beforeArtifact = input.visualInputs.find((item) => item.kind === 'page-snapshot' && item.viewportWidth === viewportWidth);
      if (!beforeArtifact) throw new Error(`Candidate verification is missing the before snapshot for viewport ${viewportWidth}.`);
      const beforeRecord = await this.options.artifacts.read(input.scope, beforeArtifact.artifactId);
      if (beforeRecord.pageId !== input.page.pageId || beforeRecord.artifact.revision !== input.baseDocument.revision) {
        throw new Error(`The before snapshot for viewport ${viewportWidth} is stale or belongs to another page.`);
      }

      const captured = await this.captureScene(input.scope, input.candidateDocument, input.page.pageId, viewportWidth);
      const capture = captured.capture as {
        artifact: GenerationArtifact;
        rootNodeId: string;
        width: number;
        height: number;
      };
      const calibration = captured.calibration as { passed: boolean; issues: Array<{ code: string; nodeId: string; severity: string }> };
      const diagnostics = captured.diagnostics as Array<{ code: string; nodeId: string; relatedNodeId?: string; severity: string }>;
      artifacts.push(...captured.artifacts as GenerationArtifact[]);
      captures.push({ rootNodeId: capture.rootNodeId, width: capture.width, height: capture.height });
      const candidateImage = captured.__images[0];
      if (candidateImage) images.push({ ...candidateImage, label: `candidate-${viewportWidth}` });

      for (const diagnostic of diagnostics.filter((item) => item.severity === 'error')) {
        layoutFailed = true;
        if (diagnostic.code === 'child-outside-container') {
          issueIds.push(`containment:${viewportWidth}:${diagnostic.relatedNodeId}:${diagnostic.nodeId}`);
        } else if (diagnostic.code === 'flow-overlap') {
          issueIds.push(`overlap:${viewportWidth}:${diagnostic.nodeId}:${diagnostic.relatedNodeId}`);
        } else {
          issueIds.push(`layout:${viewportWidth}:${diagnostic.nodeId}:${diagnostic.code}`);
        }
      }
      for (const issue of calibration.issues.filter((item) => item.severity === 'error')) {
        layoutFailed = true;
        issueIds.push(`calibration:${viewportWidth}:${issue.nodeId}:${issue.code}`);
      }

      const compared = await this.compareSnapshots(input.scope.documentId, beforeArtifact.artifactId, capture.artifact.artifactId);
      const comparison = compared.comparison as { artifact: GenerationArtifact };
      artifacts.push(comparison.artifact);
      const diffImage = compared.__images.find((item) => item.label === 'diff');
      if (diffImage) images.push({ ...diffImage, label: `diff-${viewportWidth}` });
      if (input.step.kind !== 'interaction' && Number(compared.changedPixels) === 0) {
        visibleChangeFailed = true;
        issueIds.push(`visual:${viewportWidth}:no-visible-change`);
      }
    }

    const uniqueIssueIds = [...new Set(issueIds)];
    const passed = !layoutFailed && !visibleChangeFailed;
    const createdAt = new Date().toISOString();
    const qualityArtifact = artifact('quality-report', input.candidateDocument.revision, viewportWidths[0], createdAt, {
      pageId: input.page.pageId,
      stepId: input.step.stepId,
      passed,
      viewportCount: viewportWidths.length,
      issueCount: uniqueIssueIds.length
    });
    const firstCapture = captures[0];
    await this.options.artifacts.create({
      schemaVersion: 1,
      artifact: qualityArtifact,
      scope: input.scope,
      pageId: input.page.pageId,
      rootNodeId: firstCapture.rootNodeId,
      width: firstCapture.width,
      height: firstCapture.height,
      createdAt
    });
    artifacts.push(qualityArtifact);

    const qualitySummary = passed
      ? `Candidate rendered with passing layout and browser calibration at ${viewportWidths.join(', ')}px; real before/after diffs are attached for visual review.`
      : layoutFailed
        ? `Candidate rendering or layout calibration failed at one or more required viewports (${viewportWidths.join(', ')}px).`
        : `Candidate produced no visible change at one or more required viewports (${viewportWidths.join(', ')}px). Retry with actual visible geometry or content. Prefer insert-simple-node; renaming nodes or changing only empty-container padding/gap cannot satisfy a visual Step, and raw paints must include visible:true.`;
    return {
      passed,
      qualitySummary,
      issueIds: uniqueIssueIds,
      artifacts,
      ...(passed ? {} : {
        error: {
          code: layoutFailed ? 'layout_error' as const : 'quality_reject' as const,
          message: qualitySummary,
          retryable: true,
          issueIds: uniqueIssueIds
        }
      }),
      __images: images
    };
  }

  async loadArtifactImages(documentId: string, artifacts: GenerationArtifact[]): Promise<ToolImagePayload[]> {
    const { scope } = await this.scene(documentId);
    const images: ToolImagePayload[] = [];
    const visualArtifacts = artifacts.filter((artifact) => ['page-snapshot', 'region-crop', 'visual-diff'].includes(artifact.kind));
    const latestRevision = visualArtifacts.reduce((latest, artifact) => Math.max(latest, artifact.revision), -1);
    for (const artifact of visualArtifacts) {
      if (artifact.revision !== latestRevision) continue;
      try {
        const image = await this.options.artifacts.readImage(scope, artifact.artifactId);
        images.push({
          label: `${artifact.kind}-${artifact.viewportWidth ?? 'unknown'}-r${artifact.revision}`,
          data: image.data.toString('base64'),
          mimeType: image.mimeType
        });
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
      }
    }
    return images;
  }

  async getVisualGrounding(documentId: string, artifactId: string): Promise<VisualToolResult> {
    const { scope } = await this.scene(documentId);
    let record = await this.options.artifacts.read(scope, artifactId);
    if (!record.grounding && record.snapshotArtifactId) record = await this.options.artifacts.read(scope, record.snapshotArtifactId);
    if (!record.grounding) throw new Error(`Visual artifact ${artifactId} has no grounding map.`);
    const image = record.imageFileName ? await this.options.artifacts.readImage(scope, record.artifact.artifactId) : record.snapshotArtifactId
      ? await this.options.artifacts.readImage(scope, record.snapshotArtifactId) : undefined;
    return {
      artifact: publicRecord(record),
      nodes: record.grounding,
      __images: image ? [{ label: 'grounded-snapshot', data: image.data.toString('base64'), mimeType: image.mimeType }] : []
    };
  }

  async compareSnapshots(documentId: string, beforeArtifactId: string, afterArtifactId: string): Promise<VisualToolResult> {
    const { scope } = await this.scene(documentId);
    const before = await this.options.artifacts.readImage(scope, beforeArtifactId);
    const after = await this.options.artifacts.readImage(scope, afterArtifactId);
    if (before.record.pageId !== after.record.pageId || before.record.artifact.viewportWidth !== after.record.artifact.viewportWidth) {
      throw new Error('Snapshot comparison requires the same page and viewport width.');
    }
    const diff = diffPng(before.data, after.data);
    const createdAt = new Date().toISOString();
    const diffArtifact = artifact('visual-diff', after.record.artifact.revision, after.record.artifact.viewportWidth!, createdAt, {
      pageId: after.record.pageId,
      beforeArtifactId,
      afterArtifactId,
      changedPixels: diff.changedPixels,
      changedRatio: diff.ratio
    }, diff.png);
    const affectedNodeIds = diff.region
      ? (after.record.grounding ?? []).filter((entry) => intersects(entry.rect, diff.region!)).map((entry) => entry.nodeId)
      : [];
    const record = await this.options.artifacts.create({
      schemaVersion: 1,
      artifact: { ...diffArtifact, nodeIds: affectedNodeIds },
      scope,
      pageId: after.record.pageId,
      rootNodeId: after.record.rootNodeId,
      width: before.record.width,
      height: Math.max(before.record.height, after.record.height),
      snapshotArtifactId: afterArtifactId,
      ...(diff.region ? { crop: diff.region } : {}),
      grounding: after.record.grounding,
      createdAt
    }, diff.png);
    return {
      comparison: publicRecord(record),
      before: publicRecord(before.record),
      after: publicRecord(after.record),
      changedPixels: diff.changedPixels,
      changedRatio: diff.ratio,
      regions: diff.region ? [diff.region] : [],
      affectedNodeIds,
      __images: [
        { label: 'before', data: before.data.toString('base64'), mimeType: 'image/png' },
        { label: 'after', data: after.data.toString('base64'), mimeType: 'image/png' },
        { label: 'diff', data: diff.png.toString('base64'), mimeType: 'image/png' }
      ]
    };
  }

  async inspectAtPoint(documentId: string, artifactId: string, x: number, y: number, limit = 12): Promise<VisualToolResult> {
    const { scope } = await this.scene(documentId);
    let record = await this.options.artifacts.read(scope, artifactId);
    if (!record.grounding && record.snapshotArtifactId) record = await this.options.artifacts.read(scope, record.snapshotArtifactId);
    if (!record.grounding) throw new Error(`Visual artifact ${artifactId} has no grounding map.`);
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > 50) throw new Error('Inspection limit is invalid.');
    const candidates = pointCandidates(record, x, y).slice(0, limit);
    const byId = new Map(record.grounding.map((entry) => [entry.nodeId, entry]));
    const withAncestors = candidates.map((candidate) => {
      const ancestors: VisualGroundingEntry[] = [];
      let parentId = candidate.parentId;
      while (byId.has(parentId)) {
        const parent = byId.get(parentId)!;
        ancestors.unshift(parent);
        parentId = parent.parentId;
      }
      return { ...candidate, ancestors: ancestors.map((ancestor) => ({ nodeId: ancestor.nodeId, rect: ancestor.rect })) };
    });
    const image = record.imageFileName ? await this.options.artifacts.readImage(scope, record.artifact.artifactId) : record.snapshotArtifactId
      ? await this.options.artifacts.readImage(scope, record.snapshotArtifactId) : undefined;
    return {
      artifact: publicRecord(record),
      point: { x, y },
      candidates: withAncestors,
      __images: image ? [{ label: 'inspected-snapshot', data: image.data.toString('base64'), mimeType: image.mimeType }] : []
    };
  }
}
