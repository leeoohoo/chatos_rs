import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const studio = [
  'ui-src/studio/WebDesignStudioApp.tsx',
  'ui-src/studio/WebDesignStudioWorkspace.tsx',
  'ui-src/studio/WebDesignCoreActions.ts',
  'ui-src/studio/WebDesignInsertActions.ts'
].map((path) => readFileSync(path, 'utf8')).join('\n');
const styles = readFileSync('ui-src/styles.css', 'utf8');

test('project design deletion uses an in-app confirmation that works inside sandboxed plugin views', () => {
  const deleteFlow = studio.slice(
    studio.indexOf('async function confirmDeleteProjectDocument'),
    studio.indexOf('function onPaletteDrag')
  );
  assert.doesNotMatch(deleteFlow, /window\.confirm/);
  assert.match(studio, /role="alertdialog"/);
  assert.match(studio, /aria-labelledby="delete-design-title"/);
  assert.match(studio, /setDeleteDesignTarget\(item\)/);
  assert.equal(studio.match(/setDeleteDesignTarget\(item\)/g)?.length, 2);
  assert.match(studio, /await repository\.remove\(target\.documentId\)/);
  assert.match(studio, /deletingDesign \? '正在删除…' : '永久删除'/);
  assert.match(styles, /\.primary-button\.destructive-button/);
});
