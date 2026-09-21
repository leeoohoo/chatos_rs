import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const studio = [
  'ui-src/studio/WebDesignStudioApp.tsx',
  'ui-src/studio/useWebDesignStudioState.ts',
  'ui-src/studio/WebDesignCoreActions.ts',
  'ui-src/studio/WebDesignViewportActions.ts',
  'ui-src/studio/WebDesignRenderHelpers.tsx',
  'ui-src/studio/WebDesignStudioWorkspace.tsx'
].map((path) => readFileSync(path, 'utf8')).join('\n');
const sceneCanvas = readFileSync('ui-src/studio/SceneArtboardCanvas.tsx', 'utf8');
const styles = readFileSync('ui-src/styles.css', 'utf8');

test('the editor has no application-level fullscreen preview mode', () => {
  assert.doesNotMatch(studio, /全屏预览|toggleFullPreview|preview-active|preview-mode|preview-canvas-scroll/);
  assert.match(studio, /交互当前画板/);
  assert.match(studio, /interactionMode && renderPreviewSurfaceOverlay\(\)/);
});

test('scene transforms keep pointer ownership and synchronize live artboard height', () => {
  assert.match(sceneCanvas, /setPointerCapture\?\.\(event\.pointerId\)/);
  assert.match(sceneCanvas, /transformRef\.current/);
  assert.match(sceneCanvas, /onContentHeightChangeRef\.current\?\.\(contentHeight\)/);
  assert.match(studio, /scenePreviewHeights\[artboard\.pageId\]/);
  assert.match(styles, /\.scene-v2-artboard-canvas\.transforming iframe \{ pointer-events: none !important; \}/);
});

test('scene mutations are serialized while optimistic transforms remain immediately draggable', () => {
  assert.match(studio, /const sceneCommandQueue = useRef<Promise<void>>\(Promise\.resolve\(\)\)/);
  assert.match(studio, /sceneCommandQueue\.current\.then\(execute, execute\)/);
  assert.match(studio, /sceneCommandQueue\.current = queued\.then\(\(\) => undefined, \(\) => undefined\)/);
  assert.match(sceneCanvas, /const displayedScene = previewScene \?\? optimisticScene \?\? scene/);
  assert.match(sceneCanvas, /setOptimisticScene\(optimisticDocument\);\s*setPreviewScene\(undefined\);\s*setTransforming\(false\);\s*void onCommitRef\.current\(command\)/);
  assert.equal(sceneCanvas.match(/snapshot: displayedScene/g)?.length, 2);
  assert.equal(sceneCanvas.match(/if \(transformRef\.current\) return;/g)?.length, 2);
  assert.doesNotMatch(sceneCanvas, /if \(transforming \|\| previewScene\) return;/);
  assert.match(studio, /void refreshSceneHistory\(scene\.documentId\)\.catch\(\(\) => undefined\);\s*return result\.document/);
  assert.match(sceneCanvas, /event\.type === 'pointercancel'/);
});

test('the main canvas mounts only the active semantic artboard', () => {
  assert.match(studio, /data-canvas-mode="single-artboard"/);
  assert.match(studio, /activeWorkspaceArtboard \? renderWorkspaceArtboard\(activeWorkspaceArtboard\) : null/);
  assert.doesNotMatch(studio, /workspacePlacement\?\.artboards\.map\(renderWorkspaceArtboard\)/);
  assert.doesNotMatch(studio, /显示工作区全部画板/);
  assert.doesNotMatch(studio, /显示或隐藏画板之间的原型流程线/);
  assert.match(studio, /workspaceArtboardContentBounds\(current, \{ \.\.\.artboard, x: 0, y: 0 \}/);
  assert.doesNotMatch(studio, /className="workspace-artboard-header" onPointerDown/);
  assert.match(studio, /className="artboard-directory-bar" aria-label="画板目录"/);
  assert.match(studio, /className="artboard-directory-scroll" role="tablist" aria-label="项目画板"/);
  assert.match(studio, /role="tab"/);
  assert.match(studio, /className=\{`artboard-directory-item \$\{page\.id === currentPage\?\.id \? 'active' : ''\}`\}/);
  assert.match(studio, /onClick=\{\(\) => switchPage\(page\.id\)\}/);
  assert.doesNotMatch(studio, /aria-label="画板目录切换"/);
  assert.match(studio, /aria-label="当前画板布局宽度"/);
  assert.doesNotMatch(studio, /当前画板最小高度|最小 H/);
  assert.match(studio, /高度自动/);
  const canvasStage = studio.slice(studio.indexOf('<section ref={canvasStage}'), studio.indexOf('ref={canvasScroll}'));
  assert.match(canvasStage, /artboard-directory-bar/);
  assert.doesNotMatch(studio, /rotateViewport/);
  assert.doesNotMatch(studio, /aria-label="正在编辑的画板"/);
});
