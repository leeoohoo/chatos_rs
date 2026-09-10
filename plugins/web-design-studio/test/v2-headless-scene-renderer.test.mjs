import assert from 'node:assert/strict';
import test from 'node:test';
import { ChromiumSceneImageRenderer, resolveHeadlessBrowserExecutable } from '../dist/v2-headless-scene-renderer.test.mjs';
import { renderSceneDocumentRoot } from '../dist/v2-scene-html-renderer.test.mjs';
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
