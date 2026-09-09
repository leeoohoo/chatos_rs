import { createHash, randomUUID } from 'node:crypto';
import { PNG } from 'pngjs';
import { calibrateSceneLayout } from './layout-calibration.js';
import { solveSceneLayout } from './layout-engine.js';
import { renderSceneDocumentRoot } from './scene-html-renderer.js';
import { indexSceneDocument, type SceneDocument, type SceneRect } from './scene-schema.js';
import type { SceneDocumentStore } from './scene-store.js';
import type { GenerationArtifact, GenerationScope } from './generation-plan-schema.js';
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
  if (before.width !== after.width || before.height !== after.height) throw new Error('Snapshot comparison requires equal image dimensions and viewport width.');
  const output = new PNG({ width: before.width, height: before.height });
  let changedPixels = 0;
  let minX = before.width;
  let minY = before.height;
  let maxX = -1;
  let maxY = -1;
  for (let y = 0; y < before.height; y += 1) {
    for (let x = 0; x < before.width; x += 1) {
      const offset = (y * before.width + x) * 4;
      const delta = Math.max(
        Math.abs(before.data[offset] - after.data[offset]),
        Math.abs(before.data[offset + 1] - after.data[offset + 1]),
        Math.abs(before.data[offset + 2] - after.data[offset + 2]),
        Math.abs(before.data[offset + 3] - after.data[offset + 3])
      );
      if (delta > 12) {
        changedPixels += 1;
        minX = Math.min(minX, x); minY = Math.min(minY, y); maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
        output.data[offset] = 255; output.data[offset + 1] = 45; output.data[offset + 2] = 120; output.data[offset + 3] = 255;
      } else {
        output.data[offset] = Math.round(after.data[offset] * 0.25 + 190);
        output.data[offset + 1] = Math.round(after.data[offset + 1] * 0.25 + 190);
        output.data[offset + 2] = Math.round(after.data[offset + 2] * 0.25 + 190);
        output.data[offset + 3] = 255;
      }
    }
  }
  return {
    png: PNG.sync.write(output),
    changedPixels,
    ratio: changedPixels / (before.width * before.height),
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
    const grounding = groundingFromMeasurements(document, captured.measurements, crop);
    const createdAt = new Date().toISOString();
    const snapshot = artifact(crop ? 'region-crop' : 'page-snapshot', document.revision, viewportWidth, createdAt, {
      pageId, rootNodeId, width: captured.width, height: captured.height, calibrationPassed: calibration.passed
    }, captured.png);
    const groundingArtifact = artifact('visual-grounding', document.revision, viewportWidth, createdAt, {
      pageId, rootNodeId, snapshotArtifactId: snapshot.artifactId, nodeCount: grounding.length
    });
    const layoutArtifact = artifact('layout', document.revision, viewportWidth, createdAt, {
      pageId, rootNodeId, diagnosticCount: rendered.diagnostics.length
    });
    const calibrationArtifact = artifact('calibration', document.revision, viewportWidth, createdAt, {
      pageId, rootNodeId, passed: calibration.passed, issueCount: calibration.issues.length
    });
    const common = { schemaVersion: 1 as const, scope, pageId, rootNodeId, width: captured.width, height: captured.height, createdAt };
    const snapshotRecord = await this.options.artifacts.create({
      ...common, artifact: snapshot, ...(crop ? { crop } : {}), grounding, calibration, diagnostics: rendered.diagnostics
    }, captured.png);
    await this.options.artifacts.create({ ...common, artifact: groundingArtifact, snapshotArtifactId: snapshot.artifactId, ...(crop ? { crop } : {}), grounding });
    await this.options.artifacts.create({ ...common, artifact: layoutArtifact, snapshotArtifactId: snapshot.artifactId, diagnostics: rendered.diagnostics });
    await this.options.artifacts.create({ ...common, artifact: calibrationArtifact, snapshotArtifactId: snapshot.artifactId, calibration });
    return {
      capture: publicRecord(snapshotRecord),
      artifacts: [snapshot, groundingArtifact, layoutArtifact, calibrationArtifact],
      calibration,
      diagnostics: rendered.diagnostics,
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
      width: after.record.width,
      height: after.record.height,
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
