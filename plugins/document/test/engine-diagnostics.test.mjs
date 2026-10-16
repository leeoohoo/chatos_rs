import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { build } from 'esbuild';

const bundle = await build({ entryPoints: [path.resolve(import.meta.dirname, '../src/engines/diagnostics.ts')], bundle: true, platform: 'node', format: 'esm', write: false });
const { officeCliFailure } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);

test('renderer diagnostics expose actionable classification without private engine output', () => {
  const error = officeCliFailure({ code: 1, stdout: JSON.stringify({ success: false, error: { code: 'no_screenshot_backend', error: '/Users/private/document.docx: Failed to create a unique user data directory for headless.' } }), stderr: 'secret' });
  assert.equal(error.code, 'ENGINE_UNAVAILABLE');
  assert.deepEqual(error.details, { engineCode: 'no_screenshot_backend', operation: 'render', exitCode: 1 });
  assert.doesNotMatch(JSON.stringify(error), /private|secret/);
});

test('timeout and unstructured failures preserve their categories without leaking output', () => {
  assert.equal(officeCliFailure({ killed: true }).code, 'ENGINE_TIMEOUT');
  assert.equal(officeCliFailure({ signal: 'SIGTERM' }).code, 'ENGINE_TIMEOUT');
  const error = officeCliFailure({ code: 7, stdout: 'password/private path', stderr: 'secret' });
  assert.equal(error.code, 'ENGINE_ERROR');
  assert.deepEqual(error.details, { exitCode: 7 });
  assert.doesNotMatch(JSON.stringify(error), /password|private|secret/);
  assert.equal(officeCliFailure(null).code, 'ENGINE_ERROR');
});
