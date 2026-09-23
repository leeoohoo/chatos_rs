import assert from 'node:assert/strict';
import test from 'node:test';
import { PNG } from 'pngjs';
import { ChromiumSceneImageRenderer, resolveHeadlessBrowserExecutable } from '../dist/v2-headless-scene-renderer.test.mjs';
import { renderSceneDocumentRoot } from '../dist/v2-scene-html-renderer.test.mjs';
import { createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

let browser;
try { browser = resolveHeadlessBrowserExecutable(); } catch { browser = undefined; }

test('Chromium renders a real Scene PNG with stable node measurements', { skip: !browser, timeout: 45_000 }, async () => {
  const rendered = renderSceneDocumentRoot(nestedWebsite(), 'section-responsive', 800);
  const captured = await new ChromiumSceneImageRenderer(browser).capture({
    html: rendered.documentHtml,
    width: rendered.width,
    height: rendered.height
  });
  assert.ok(captured.png.byteLength > 1000);
  assert.deepEqual({ width: captured.width, height: captured.height }, { width: 800, height: 1100 });
  assert.ok(captured.measurements['section-responsive']);
  assert.ok(captured.measurements['text-hero-heading']);
  assert.equal(Math.round(captured.measurements['text-hero-heading'].rect.x), 80);
});

test('Chromium does not wait for library instances outside the captured root', { skip: !browser, timeout: 45_000 }, async () => {
  const document = nestedWebsite();
  addLibraryInstance(document, {
    id: 'desktop-only-button', library: 'shadcn', component: 'Button', slug: 'button', content: 'Desktop only',
    x: 80, y: 300, width: 220, height: 64
  });
  const mobileRoot = {
    ...createSceneNodeBase('frame', 'Mobile root', { x: 0, y: 0, width: 390, height: 768 }, 'system'),
    id: 'root-mobile-empty',
    children: []
  };
  document.pages.push({ id: 'page-mobile', name: 'Mobile', children: [mobileRoot] });

  const rendered = renderSceneDocumentRoot(document, mobileRoot.id, 390);
  assert.match(rendered.documentHtml, /data-scene-ready="true"/);
  assert.doesNotMatch(rendered.documentHtml, /desktop-only-button/);
  const captured = await new ChromiumSceneImageRenderer(browser).capture({
    html: rendered.documentHtml,
    width: rendered.width,
    height: rendered.height
  });
  assert.ok(captured.png.byteLength > 1000);
  assert.ok(captured.measurements[mobileRoot.id]);
});

function addLibraryInstance(document, { id, library, component, slug, content, properties = {}, x, y, width, height }) {
  const node = {
    ...createSceneNodeBase('library-instance', component, { x, y, width, height }, 'ai'),
    id,
    type: 'library-instance',
    library,
    component,
    content,
    properties: { componentSlug: slug, ...properties },
    slots: {}
  };
  node.layout.position = 'absolute';
  document.pages[0].children[0].children.push(node);
}

function nonWhitePixels(pngBuffer, rect) {
  const png = PNG.sync.read(pngBuffer);
  let count = 0;
  for (let y = Math.max(0, Math.floor(rect.y)); y < Math.min(png.height, Math.ceil(rect.y + rect.height)); y += 1) {
    for (let x = Math.max(0, Math.floor(rect.x)); x < Math.min(png.width, Math.ceil(rect.x + rect.width)); x += 1) {
      const offset = (y * png.width + x) * 4;
      if (png.data[offset + 3] > 0 && (png.data[offset] < 245 || png.data[offset + 1] < 245 || png.data[offset + 2] < 245)) count += 1;
    }
  }
  return count;
}

test('Chromium waits for real shadcn, Magic UI, and Chakra component pixels', { skip: !browser, timeout: 60_000 }, async () => {
  const document = nestedWebsite();
  addLibraryInstance(document, {
    id: 'runtime-shadcn-button', library: 'shadcn', component: 'Button', slug: 'button', content: '开始设计',
    properties: { registryDemo: 'button-demo', variant: 'outline', size: 'default' }, x: 80, y: 300, width: 220, height: 64
  });
  addLibraryInstance(document, {
    id: 'runtime-magic-text', library: 'magicui', component: 'TextAnimate', slug: 'text-animate', content: '真实组件画面',
    properties: { animation: 'blurInUp', by: 'word' }, x: 80, y: 400, width: 460, height: 100
  });
  addLibraryInstance(document, {
    id: 'runtime-chakra-heading', library: 'chakra', component: 'Heading', slug: 'heading', content: '漂亮的网站从真实画面开始',
    properties: { size: '2xl', level: 2 }, x: 80, y: 540, width: 460, height: 90
  });

  const rendered = renderSceneDocumentRoot(document, 'section-responsive', 800);
  const captured = await new ChromiumSceneImageRenderer(browser).capture({
    html: rendered.documentHtml,
    width: rendered.width,
    height: rendered.height
  });

  for (const id of ['runtime-shadcn-button', 'runtime-magic-text', 'runtime-chakra-heading']) {
    const rect = captured.measurements[id].rect;
    assert.ok(nonWhitePixels(captured.png, rect) > 40, `${id} should contribute visible pixels to the Candidate PNG`);
  }
  assert.match(captured.measurements['runtime-shadcn-button'].renderedText, /开始设计/);
  assert.doesNotMatch(captured.measurements['runtime-shadcn-button'].renderedText, /^Button$/);
});

test('Chromium renders unitless line heights without overflow', { skip: !browser, timeout: 45_000 }, async () => {
  const document = nestedWebsite();
  const heading = document.pages[0].children[0].children[0].children[0].children[0];
  heading.appearance.typography.lineHeight = 1.125;
  const rendered = renderSceneDocumentRoot(document, 'section-responsive', 800);
  const captured = await new ChromiumSceneImageRenderer(browser).capture({
    html: rendered.documentHtml,
    width: rendered.width,
    height: rendered.height
  });
  const measurement = captured.measurements['text-hero-heading'];
  assert.ok(measurement.scrollHeight <= measurement.rect.height + 1, JSON.stringify(measurement));
});
