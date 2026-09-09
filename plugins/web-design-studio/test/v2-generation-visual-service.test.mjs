import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { PNG } from 'pngjs';
import { GenerationVisualArtifactStore } from '../dist/v2-generation-visual-artifact-store.test.mjs';
import { GenerationVisualService } from '../dist/v2-generation-visual-service.test.mjs';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

function measurementsFromHtml(html) {
  const result = {};
  const pattern = /data-scene-node-id="([^"]+)" data-expected-x="([^"]+)" data-expected-y="([^"]+)" data-expected-width="([^"]+)" data-expected-height="([^"]+)"/g;
  for (const match of html.matchAll(pattern)) {
    const rect = { x: Number(match[2]), y: Number(match[3]), width: Number(match[4]), height: Number(match[5]) };
    result[match[1]] = { rect, scrollWidth: rect.width, scrollHeight: rect.height };
  }
  return result;
}

function solidPng(width, height, color) {
  const png = new PNG({ width, height });
  for (let offset = 0; offset < png.data.length; offset += 4) {
    png.data[offset] = color[0];
    png.data[offset + 1] = color[1];
    png.data[offset + 2] = color[2];
    png.data[offset + 3] = 255;
  }
  return PNG.sync.write(png);
}

class FakeSceneRenderer {
  captures = 0;

  async capture(input) {
    this.captures += 1;
    const width = Math.ceil(input.clip?.width ?? input.width);
    const height = Math.ceil(input.clip?.height ?? input.height);
    const color = this.captures === 1 ? [30, 60, 90] : [210, 120, 40];
    return { png: solidPng(width, height, color), width, height, measurements: measurementsFromHtml(input.html) };
  }
}

async function fixture(root) {
  const scenes = new SceneDocumentStore(root);
  const document = await scenes.create(nestedWebsite());
  const renderer = new FakeSceneRenderer();
  return {
    document,
    renderer,
    service: new GenerationVisualService({
      projectId: 'project-visual-service',
      scenes,
      artifacts: new GenerationVisualArtifactStore(root),
      renderer,
      assertDocumentInScope: async (documentId) => {
        if (documentId !== document.documentId) throw new Error('outside scope');
      }
    })
  };
}

test('visual service captures persistent page and region images with stable node grounding', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-visual-service-'));
  try {
    const { document, service } = await fixture(root);
    const page = await service.capturePage(document.documentId, 'page-home', 800);
    assert.equal(page.capture.artifact.kind, 'page-snapshot');
    assert.equal(page.capture.artifact.revision, 1);
    assert.equal(page.capture.width, 800);
    assert.equal(page.capture.height, 1100);
    assert.equal(page.calibration.passed, true);
    assert.deepEqual(page.artifacts.map((item) => item.kind), ['page-snapshot', 'visual-grounding', 'layout', 'calibration']);
    assert.equal(page.__images.length, 1);
    assert.ok(Buffer.from(page.__images[0].data, 'base64').byteLength > 100);

    const groundingArtifact = page.artifacts.find((item) => item.kind === 'visual-grounding');
    const grounding = await service.getVisualGrounding(document.documentId, groundingArtifact.artifactId);
    assert.ok(grounding.nodes.some((node) => node.nodeId === 'text-hero-heading'));
    assert.equal(grounding.__images.length, 1);

    const point = await service.inspectAtPoint(document.documentId, page.capture.artifact.artifactId, 100, 100);
    assert.equal(point.candidates[0].nodeId, 'text-hero-heading');
    assert.ok(point.candidates[0].ancestors.some((ancestor) => ancestor.nodeId === 'section-responsive'));

    const region = await service.captureRegion({
      documentId: document.documentId, pageId: 'page-home', viewportWidth: 800,
      nodeId: 'text-hero-heading', padding: 10
    });
    assert.equal(region.capture.artifact.kind, 'region-crop');
    assert.equal(region.capture.width, 660);
    assert.ok(region.capture.height > 150 && region.capture.height < 160);
    const regionGrounding = await service.getVisualGrounding(document.documentId, region.capture.artifact.artifactId);
    const heading = regionGrounding.nodes.find((node) => node.nodeId === 'text-hero-heading');
    assert.deepEqual({ x: heading.rect.x, y: heading.rect.y }, { x: 10, y: 10 });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('visual service returns before, after, and Diff PNGs with affected stable node IDs', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-visual-diff-'));
  try {
    const { document, service } = await fixture(root);
    const before = await service.capturePage(document.documentId, 'page-home', 800);
    const after = await service.capturePage(document.documentId, 'page-home', 800);
    const comparison = await service.compareSnapshots(
      document.documentId, before.capture.artifact.artifactId, after.capture.artifact.artifactId
    );
    assert.equal(comparison.comparison.artifact.kind, 'visual-diff');
    assert.equal(comparison.changedRatio, 1);
    assert.equal(comparison.regions.length, 1);
    assert.ok(comparison.affectedNodeIds.includes('text-hero-heading'));
    assert.deepEqual(comparison.__images.map((item) => item.label), ['before', 'after', 'diff']);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
