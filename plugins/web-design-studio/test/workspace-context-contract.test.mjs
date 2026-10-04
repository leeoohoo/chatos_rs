import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const app = readFileSync('ui-src/studio/WebDesignStudioApp.tsx', 'utf8');
const renderer = readFileSync('ui-src/studio/WebDesignStudioWorkspace.tsx', 'utf8');
const context = readFileSync('ui-src/studio/WebDesignWorkspaceContext.ts', 'utf8');

test('workspace composition has an explicit typed boundary', () => {
  assert.doesNotMatch(renderer, /Record<string,\s*any>/);
  assert.doesNotMatch(renderer, /context:\s*Record</);
  assert.match(renderer, /context:\s*WebDesignWorkspaceContext/);
  assert.match(context, /interface WebDesignWorkspaceComposition/);
  assert.match(context, /export type WebDesignWorkspaceContext/);
});

test('the app composes workspace layers instead of forwarding a manual property list', () => {
  assert.match(app, /createWebDesignWorkspaceContext\(\{/);
  for (const layer of [
    'studioState',
    'webDesignCoreActions',
    'webDesignInsertActions',
    'webDesignCanvasActions',
    'webDesignViewportActions',
    'webDesignSelectionActions',
    'webDesignAssetActions',
    'webDesignDocumentActions',
    'webDesignRenderHelpers',
  ]) {
    assert.match(app, new RegExp(`\\.\\.\\.${layer}`));
  }
});
