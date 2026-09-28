import assert from 'node:assert/strict';
import test from 'node:test';
import { applyWebDesignPatch, assertWebDesignDocument } from '../dist/schema.test.mjs';
import {
  autoLayoutContainer,
  breakpointFor,
  cloneComponentSubtrees,
  componentsForPage,
  flattenComponentTree,
  moveComponentsWithDescendants,
  resolveComponent,
  snapComponentFrame
} from '../dist/editor-model.test.mjs';
import { createLandingPage } from '../dist/templates.test.mjs';
import { exportDocumentHtmlFiles, exportPageHtml } from '../dist/html-exporter.test.mjs';
import { exportReactComponent } from '../dist/react-exporter.test.mjs';
import { exportVueComponent } from '../dist/vue-exporter.test.mjs';

test('device-specific patches preserve desktop layout and update mobile overrides', () => {
  const document = createLandingPage();
  const heading = document.components.find((component) => component.id === 'hero-heading');
  const desktopBefore = resolveComponent(heading, 'desktop');
  const next = applyWebDesignPatch(document, [
    { op: 'set_breakpoint', device: 'mobile', width: 430, height: 1500 },
    { op: 'move_component', componentId: 'hero-heading', device: 'mobile', x: 44, y: 70 },
    { op: 'resize_component', componentId: 'hero-heading', device: 'mobile', width: 342, height: 190 },
    { op: 'update_component', componentId: 'hero-heading', device: 'mobile', changes: { style: { fontSize: 36 }, locked: true } }
  ]);
  assert.deepEqual(resolveComponent(next.components.find((component) => component.id === 'hero-heading'), 'desktop'), desktopBefore);
  assert.deepEqual(resolveComponent(next.components.find((component) => component.id === 'hero-heading'), 'mobile'), {
    x: 44,
    y: 70,
    width: 342,
    height: 190,
    hidden: false,
    style: { color: '#1D1D1F', fontSize: 36, fontWeight: 800, textAlign: 'left' }
  });
  assert.equal(next.components.find((component) => component.id === 'hero-heading').locked, true);
  assert.equal(breakpointFor(next, 'mobile').width, 430);
});

test('focused patches preserve unrelated components and attach requests', () => {
  const document = createLandingPage();
  const untouched = structuredClone(document.components.find((component) => component.id === 'hero-copy'));
  const next = applyWebDesignPatch(document, [
    { op: 'update_component', componentId: 'hero-heading', changes: { content: '新的标题', style: { color: '#111111' } } },
    {
      op: 'add_request',
      request: {
        id: 'request-test',
        componentId: 'hero-heading',
        instruction: '让标题更有力量',
        status: 'pending',
        createdAt: new Date().toISOString()
      }
    }
  ]);
  assert.equal(next.components.find((component) => component.id === 'hero-heading').content, '新的标题');
  assert.deepEqual(next.components.find((component) => component.id === 'hero-copy'), untouched);
  assert.equal(next.requests.length, 1);
});

test('parent relationships validate, flatten into a tree, and reject cycles', () => {
  const document = createLandingPage();
  document.components.find((component) => component.id === 'hero-heading').parentId = 'hero-section';
  document.components.find((component) => component.id === 'hero-copy').parentId = 'hero-section';
  assertWebDesignDocument(document);
  const tree = flattenComponentTree(document);
  assert.equal(tree.find((item) => item.component.id === 'hero-heading').depth, 1);

  const cyclic = structuredClone(document);
  cyclic.components.find((component) => component.id === 'hero-section').parentId = 'hero-heading';
  assert.throws(() => assertWebDesignDocument(cyclic), /cycle/);
});

test('moving selected roots moves descendants once even when parent and child are both selected', () => {
  const document = createLandingPage();
  const heading = document.components.find((component) => component.id === 'hero-heading');
  heading.parentId = 'hero-section';
  const before = resolveComponent(heading, 'desktop');
  const moved = moveComponentsWithDescendants(document, ['hero-section', 'hero-heading'], 'desktop', 25, 30);
  const after = resolveComponent(moved.components.find((component) => component.id === 'hero-heading'), 'desktop');
  assert.equal(after.x, before.x + 25);
  assert.equal(after.y, before.y + 30);
});

test('container auto layout supports flex row, flex column, and grid', () => {
  const base = createLandingPage();
  for (const component of base.components) component.parentId = undefined;
  for (const id of ['hero-heading', 'hero-copy', 'hero-primary-action']) {
    base.components.find((component) => component.id === id).parentId = 'hero-section';
  }
  const container = base.components.find((component) => component.id === 'hero-section');
  container.layout = { mode: 'flex-row', gap: 10, padding: 20, align: 'start' };
  const row = autoLayoutContainer(base, container.id, 'desktop');
  assert.equal(resolveComponent(row.components.find((component) => component.id === 'hero-heading'), 'desktop').x, 80);
  assert.equal(resolveComponent(row.components.find((component) => component.id === 'hero-copy'), 'desktop').x, 650);

  container.layout = { mode: 'flex-column', gap: 12, padding: 24, align: 'center' };
  const column = autoLayoutContainer(base, container.id, 'desktop');
  assert.equal(resolveComponent(column.components.find((component) => component.id === 'hero-heading'), 'desktop').y, 84);
  assert.equal(resolveComponent(column.components.find((component) => component.id === 'hero-copy'), 'desktop').y, 246);

  container.layout = { mode: 'grid', gap: 10, padding: 20, columns: 2, align: 'stretch' };
  const grid = autoLayoutContainer(base, container.id, 'desktop');
  const first = resolveComponent(grid.components.find((component) => component.id === 'hero-heading'), 'desktop');
  const second = resolveComponent(grid.components.find((component) => component.id === 'hero-copy'), 'desktop');
  const third = resolveComponent(grid.components.find((component) => component.id === 'hero-primary-action'), 'desktop');
  assert.equal(first.width, 515);
  assert.equal(second.x, 605);
  assert.equal(third.y, 240);
});

test('flex auto layout can distribute and wrap arbitrary child compositions', () => {
  const document = createLandingPage();
  document.components = document.components.filter((component) => ['hero-section', 'hero-heading', 'hero-copy', 'hero-primary-action'].includes(component.id));
  const container = document.components.find((component) => component.id === 'hero-section');
  container.width = 700;
  container.height = 500;
  const children = document.components.filter((component) => component.id !== container.id);
  for (const child of children) {
    child.parentId = container.id;
    child.width = 280;
    child.height = 80;
  }
  container.layout = { mode: 'flex-row', gap: 20, padding: 20, align: 'center', justify: 'space-between', wrap: true };
  const laidOut = autoLayoutContainer(document, container.id, 'desktop');
  const frames = children.map((child) => resolveComponent(laidOut.components.find((component) => component.id === child.id), 'desktop'));
  assert.deepEqual(frames.map((frame) => [frame.x, frame.y]), [[80, 80], [460, 80], [80, 180]]);
  assertWebDesignDocument(laidOut);
});

test('auto layout preserves nested child offsets when moving a child container', () => {
  const document = createLandingPage();
  for (const component of document.components) component.parentId = undefined;
  const container = document.components.find((component) => component.id === 'hero-section');
  const heading = document.components.find((component) => component.id === 'hero-heading');
  const button = document.components.find((component) => component.id === 'hero-primary-action');
  heading.parentId = container.id;
  button.parentId = heading.id;
  container.layout = { mode: 'flex-column', gap: 10, padding: 20, align: 'start' };
  const headingBefore = resolveComponent(heading, 'desktop');
  const buttonBefore = resolveComponent(button, 'desktop');
  const laidOut = autoLayoutContainer(document, container.id, 'desktop');
  const headingAfter = resolveComponent(laidOut.components.find((component) => component.id === heading.id), 'desktop');
  const buttonAfter = resolveComponent(laidOut.components.find((component) => component.id === button.id), 'desktop');
  assert.equal(buttonAfter.x - buttonBefore.x, headingAfter.x - headingBefore.x);
  assert.equal(buttonAfter.y - buttonBefore.y, headingAfter.y - headingBefore.y);
});

test('snapping aligns frames to canvas and sibling geometry', () => {
  const document = createLandingPage();
  const heading = document.components.find((component) => component.id === 'hero-heading');
  const canvasSnap = snapComponentFrame(document, heading.id, 'desktop', { ...resolveComponent(heading, 'desktop'), x: 5, y: 4 });
  assert.equal(canvasSnap.frame.x, 0);
  assert.equal(canvasSnap.frame.y, 0);
  assert.equal(canvasSnap.guides.x, 0);

  const copy = document.components.find((component) => component.id === 'hero-copy');
  const copyFrame = resolveComponent(copy, 'desktop');
  const siblingSnap = snapComponentFrame(document, heading.id, 'desktop', {
    ...resolveComponent(heading, 'desktop'),
    x: copyFrame.x + copyFrame.width / 2 - heading.width / 2 + 6,
    y: 900
  }, document.components.filter((component) => component.id !== copy.id).map((component) => component.id));
  assert.equal(siblingSnap.frame.x + heading.width / 2, copyFrame.x + copyFrame.width / 2);
});

test('pages, image assets, and page removal are revision-safe patch operations', () => {
  const document = createLandingPage();
  const aboutCard = structuredClone(document.components.find((component) => component.id === 'feature-ai'));
  aboutCard.id = 'about-card';
  aboutCard.pageId = 'about';
  aboutCard.parentId = undefined;
  const withPage = applyWebDesignPatch(document, [
    { op: 'upsert_page', page: { id: 'about', name: '关于', slug: '/about' } },
    { op: 'upsert_asset', asset: { id: 'asset-logo', name: 'logo.png', mimeType: 'image/png', dataUrl: 'data:image/png;base64,AA==', createdAt: new Date().toISOString() } },
    { op: 'upsert_component', component: aboutCard }
  ]);
  assert.equal(componentsForPage(withPage, 'about').length, 1);
  assert.equal(withPage.assets.length, 1);
  const removed = applyWebDesignPatch(withPage, [{ op: 'remove_page', pageId: 'about' }, { op: 'remove_asset', assetId: 'asset-logo' }]);
  assert.equal(removed.pages.length, 1);
  assert.equal(removed.components.some((component) => component.id === 'about-card'), false);
  assert.equal(removed.assets.length, 0);
});

test('copying component subtrees remaps IDs and preserves hierarchy across pages', () => {
  const document = createLandingPage();
  document.pages.push({ id: 'about', name: '关于', slug: '/about' });
  const cloned = cloneComponentSubtrees(document, ['hero-section', 'hero-heading'], 'about', 0, document);
  const clonedRoot = cloned.components.find((component) => component.id === cloned.rootIds[0]);
  const clonedHeading = cloned.components.find((component) => component.content.includes('把想法变成'));
  const clonedImage = cloned.components.find((component) => component.type === 'image');
  assert.equal(cloned.components.length, 5);
  assert.equal(clonedRoot.pageId, 'about');
  assert.equal(clonedHeading.parentId, clonedRoot.id);
  assert.notEqual(clonedHeading.id, 'hero-heading');
  assert.ok(clonedImage.zIndex < clonedHeading.zIndex);
});

test('HTML export emits standalone files and isolates page components', () => {
  const document = createLandingPage();
  document.pages.push({ id: 'about', name: '关于', slug: '/about' });
  const card = structuredClone(document.components.find((component) => component.id === 'feature-ai'));
  card.id = 'about-only';
  card.pageId = 'about';
  card.parentId = undefined;
  card.content = '只在关于页面';
  document.components.push(card);
  const homeHtml = exportPageHtml(document, 'home', 'mobile');
  assert.match(homeHtml, /width: 390px/);
  assert.doesNotMatch(homeHtml, /只在关于页面/);
  const files = exportDocumentHtmlFiles(document);
  assert.deepEqual(files.map((file) => file.filename), ['index.html', 'about.html']);
  assert.match(files[1].html, /只在关于页面/);
});

test('design tokens flow into HTML and React exports', () => {
  const document = applyWebDesignPatch(createLandingPage(), [{
    op: 'set_tokens',
    tokens: {
      colors: { primary: '#123456', accent: '#22C55E', surface: '#FAFAFA', text: '#111111', muted: '#777777' },
      radii: { small: 6, medium: 14, large: 30 },
      typography: { fontFamily: 'Manrope, sans-serif', baseFontSize: 18 }
    }
  }]);
  const html = exportPageHtml(document, 'home');
  assert.match(html, /--color-primary: #123456/);
  assert.match(html, /font-family: Manrope, sans-serif/);
  const react = exportReactComponent(document);
  assert.equal(react.filename, 'WebDesignApp.jsx');
  assert.match(react.content, /window\.history\.pushState/);
  assert.match(react.content, /#123456/);
});

test('component interactions validate and flow into routed React and Vue exports', () => {
  const document = createLandingPage();
  document.pages.push({ id: 'about', name: '关于', slug: '/about' });
  document.components.find((component) => component.id === 'hero-primary-action').interaction = { type: 'page', target: 'about' };
  document.components.find((component) => component.id === 'feature-ai').interaction = { type: 'url', target: 'https://example.com/docs' };
  assertWebDesignDocument(document);
  const react = exportReactComponent(document);
  assert.match(react.content, /component\.interaction\.type === 'page'/);
  assert.match(react.content, /window\.open/);
  const vue = exportVueComponent(document, 'mobile');
  assert.equal(vue.filename, 'WebDesignApp.vue');
  assert.match(vue.content, /<script setup>/);
  assert.match(vue.content, /window\.history\.pushState/);
  assert.match(vue.content, /https:\/\/example\.com\/docs/);

  const invalidPage = structuredClone(document);
  invalidPage.components.find((component) => component.id === 'hero-primary-action').interaction.target = 'missing-page';
  assert.throws(() => assertWebDesignDocument(invalidPage), /missing page/);
  const invalidUrl = structuredClone(document);
  invalidUrl.components.find((component) => component.id === 'feature-ai').interaction.target = 'javascript:alert(1)';
  assert.throws(() => assertWebDesignDocument(invalidUrl), /http or https/);
});
