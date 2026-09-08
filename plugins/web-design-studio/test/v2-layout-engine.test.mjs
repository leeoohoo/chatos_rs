import assert from 'node:assert/strict';
import test from 'node:test';
import { createBlankSceneDocument, createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';
import { solveSceneLayout } from '../dist/v2-layout-engine.test.mjs';

function text(id, content, sizingX = 'hug', sizingY = 'hug') {
  return {
    ...createSceneNodeBase('text', id, { x: 0, y: 0, width: 120, height: 24 }),
    id,
    content,
    layout: {
      ...createSceneNodeBase('text', 'layout', { x: 0, y: 0, width: 1, height: 1 }).layout,
      sizingX,
      sizingY
    },
    appearance: {
      ...createSceneNodeBase('text', 'appearance', { x: 0, y: 0, width: 1, height: 1 }).appearance,
      typography: { fontFamily: 'Inter', fontSize: 16, fontWeight: 400, lineHeight: 1.5, letterSpacing: 0, textAlign: 'left' }
    }
  };
}

function frame(id, direction, children, options = {}) {
  const base = createSceneNodeBase('frame', id, { x: 0, y: 0, width: options.width ?? 600, height: options.height ?? 400 });
  return {
    ...base,
    id,
    layout: {
      ...base.layout,
      mode: 'auto',
      direction,
      wrap: options.wrap ?? false,
      padding: options.padding ?? { top: 20, right: 20, bottom: 20, left: 20 },
      gap: options.gap ?? { row: 10, column: 10 },
      alignItems: options.alignItems ?? 'start',
      justifyContent: options.justifyContent ?? 'start',
      sizingX: options.sizingX ?? 'fixed',
      sizingY: options.sizingY ?? 'fixed',
      minWidth: options.minWidth,
      maxWidth: options.maxWidth,
      minHeight: options.minHeight,
      maxHeight: options.maxHeight,
      position: 'flow',
      clipContent: false
    },
    children
  };
}

function documentWithRoot(root) {
  const document = createBlankSceneDocument('Layout');
  document.documentId = 'scene-layout';
  document.pages[0].id = 'page-layout';
  document.pages[0].children = [root];
  return document;
}

function gridFrame(id, children, columns, options = {}) {
  const root = frame(id, 'vertical', children, options);
  root.layout.mode = 'grid';
  delete root.layout.direction;
  delete root.layout.wrap;
  root.layout.grid = {
    columns,
    rows: options.rows ?? [],
    autoFlow: options.autoFlow ?? 'row'
  };
  return root;
}

function gridItem(id, width = 120, height = 40) {
  const item = text(id, id, 'fill', 'fixed');
  item.frame.width = width;
  item.frame.height = height;
  return item;
}

test('vertical auto layout positions flow children and hugs content height', () => {
  const root = frame('frame-root', 'vertical', [
    text('text-heading', 'A compact heading'),
    text('text-body', 'Supporting copy that can grow with its content.', 'fill', 'hug')
  ], { width: 800, sizingX: 'fill', sizingY: 'hug', padding: { top: 40, right: 40, bottom: 40, left: 40 }, gap: { row: 24, column: 24 } });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 480 });
  const heading = solved.boxes.get('text-heading');
  const body = solved.boxes.get('text-body');
  const box = solved.boxes.get(root.id);
  assert.equal(heading.x, 40);
  assert.equal(body.x, 40);
  assert.equal(body.width, 400);
  assert.equal(body.y, heading.y + heading.height + 24);
  assert.equal(box.height, 40 + heading.height + 24 + body.height + 40);
});

test('horizontal fill divides remaining space without changing fixed children', () => {
  const fixed = text('text-fixed', 'Fixed', 'fixed', 'fixed');
  fixed.frame.width = 100;
  fixed.frame.height = 40;
  const root = frame('frame-root', 'horizontal', [fixed, text('text-fill-a', 'A', 'fill', 'fixed'), text('text-fill-b', 'B', 'fill', 'fixed')], {
    width: 600,
    height: 80,
    sizingX: 'fill',
    padding: { top: 20, right: 20, bottom: 20, left: 20 },
    gap: { row: 10, column: 20 }
  });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 600, viewportHeight: 80 });
  assert.equal(solved.boxes.get('text-fixed').width, 100);
  assert.equal(solved.boxes.get('text-fill-a').width, 210);
  assert.equal(solved.boxes.get('text-fill-b').width, 210);
  assert.equal(solved.boxes.get('text-fill-b').x, 370);
});

test('content changes remeasure text and grow every hugging ancestor', () => {
  const body = text('text-body', 'Short copy', 'fill', 'hug');
  const card = frame('frame-card', 'vertical', [body], { sizingX: 'fill', sizingY: 'hug' });
  const root = frame('frame-root', 'vertical', [card], { sizingX: 'fill', sizingY: 'hug' });
  const document = documentWithRoot(root);
  const short = solveSceneLayout(document, { rootNodeId: root.id, viewportWidth: 260 });
  body.content = 'This is much longer website copy that must wrap across several lines and force both the card and root frame to grow naturally.';
  const long = solveSceneLayout(document, { rootNodeId: root.id, viewportWidth: 260 });
  assert.ok(long.boxes.get('text-body').height > short.boxes.get('text-body').height);
  assert.ok(long.boxes.get('frame-card').height > short.boxes.get('frame-card').height);
  assert.ok(long.boxes.get('frame-root').height > short.boxes.get('frame-root').height);
});

test('horizontal wrap creates stable rows at continuously changing viewport widths', () => {
  const items = Array.from({ length: 4 }, (_, index) => {
    const item = text(`text-item-${index}`, `Item ${index}`, 'fixed', 'fixed');
    item.frame.width = 120;
    item.frame.height = 40;
    return item;
  });
  const root = frame('frame-root', 'horizontal', items, {
    sizingX: 'fill', sizingY: 'hug', wrap: true,
    padding: { top: 10, right: 10, bottom: 10, left: 10 },
    gap: { row: 12, column: 10 }
  });
  const narrow = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 280 });
  assert.equal(narrow.boxes.get('text-item-0').y, 10);
  assert.equal(narrow.boxes.get('text-item-2').y, 62);
  assert.equal(narrow.boxes.get(root.id).height, 112);
  const wide = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 540 });
  assert.equal(wide.boxes.get('text-item-3').y, 10);
  assert.equal(wide.boxes.get(root.id).height, 60);
});

test('absolute children stay out of flow and min/max bounds constrain solved sizes', () => {
  const flow = text('text-flow', 'Flow content');
  const overlay = text('text-overlay', 'Overlay', 'fixed', 'fixed');
  overlay.layout.position = 'absolute';
  overlay.frame = { x: 300, y: 200, width: 140, height: 40 };
  const root = frame('frame-root', 'vertical', [flow, overlay], {
    sizingX: 'fill', sizingY: 'hug', minHeight: 100, maxHeight: 140
  });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 500 });
  assert.equal(solved.boxes.get(root.id).height, 100);
  assert.equal(solved.boxes.get('text-overlay').x, 300);
  assert.equal(solved.boxes.get('text-overlay').y, 200);
  assert.equal(solved.boxes.get('text-flow').y, 20);
});

test('fill layouts remain inside ten continuous benchmark viewport widths', () => {
  const root = frame('frame-root', 'vertical', [text('text-content', 'Responsive content', 'fill', 'hug')], {
    sizingX: 'fill', sizingY: 'hug', maxWidth: 1440,
    padding: { top: 32, right: 32, bottom: 32, left: 32 }
  });
  const document = documentWithRoot(root);
  for (const viewportWidth of [320, 390, 768, 1024, 1280, 1440, 1920, 2560, 3840, 7680]) {
    const solved = solveSceneLayout(document, { rootNodeId: root.id, viewportWidth });
    const rootBox = solved.boxes.get(root.id);
    const content = solved.boxes.get('text-content');
    assert.equal(rootBox.width, Math.min(viewportWidth, 1440));
    assert.ok(content.x + content.width <= rootBox.x + rootBox.width);
    assert.equal(rootBox.overflowX, false);
  }
});

test('auto-fit grid changes column count continuously without duplicating nodes', () => {
  const items = Array.from({ length: 4 }, (_, index) => gridItem(`text-card-${index}`));
  const root = gridFrame('frame-grid', items, ['repeat(auto-fit, minmax(200px, 1fr))'], {
    sizingX: 'fill', sizingY: 'hug',
    padding: { top: 10, right: 10, bottom: 10, left: 10 },
    gap: { row: 10, column: 20 }
  });
  const document = documentWithRoot(root);
  const wide = solveSceneLayout(document, { rootNodeId: root.id, viewportWidth: 900 });
  assert.equal(wide.boxes.get('text-card-3').y, 10);
  assert.equal(wide.boxes.get(root.id).height, 60);
  const narrow = solveSceneLayout(document, { rootNodeId: root.id, viewportWidth: 480 });
  assert.equal(narrow.boxes.get('text-card-2').y, 60);
  assert.equal(narrow.boxes.get(root.id).height, 110);
  assert.equal(narrow.boxes.size, wide.boxes.size);
});

test('grid placement supports explicit columns, rows, and spans', () => {
  const featured = gridItem('text-featured');
  featured.layout.gridPlacement = { columnStart: 1, rowStart: 1, columnSpan: 2, rowSpan: 1 };
  const side = gridItem('text-side');
  side.layout.gridPlacement = { columnStart: 3, rowStart: 1 };
  const root = gridFrame('frame-grid', [featured, side], ['1fr', '1fr', '1fr'], {
    width: 600, height: 100, sizingX: 'fill', sizingY: 'fixed',
    rows: ['1fr'],
    padding: { top: 20, right: 20, bottom: 20, left: 20 },
    gap: { row: 10, column: 20 }
  });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 600, viewportHeight: 100 });
  assert.ok(Math.abs(solved.boxes.get('text-featured').width - 366.6666666667) < 0.01);
  assert.ok(Math.abs(solved.boxes.get('text-side').x - 406.6666666667) < 0.01);
  assert.equal(solved.boxes.get('text-featured').height, 40);
});

test('grid reports explicit placement collisions and column overflow', () => {
  const first = gridItem('text-first');
  first.layout.gridPlacement = { columnStart: 1, rowStart: 1 };
  const collision = gridItem('text-collision');
  collision.layout.gridPlacement = { columnStart: 1, rowStart: 1 };
  const overflow = gridItem('text-overflow');
  overflow.layout.gridPlacement = { columnStart: 2, rowStart: 2, columnSpan: 2 };
  const root = gridFrame('frame-grid', [first, collision, overflow], ['1fr', '1fr'], { sizingX: 'fill', sizingY: 'hug' });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 500 });
  assert.ok(solved.diagnostics.some((diagnostic) => diagnostic.nodeId === 'text-collision' && diagnostic.code === 'grid-placement-collision'));
  assert.ok(solved.diagnostics.some((diagnostic) => diagnostic.nodeId === 'text-overflow' && diagnostic.code === 'grid-placement-out-of-bounds'));
});

test('free-layout constraints preserve left, center, right, stretch, and scale intent', () => {
  const modes = [
    ['left', 100, 200, 100, 200],
    ['center', 400, 200, 150, 200],
    ['right', 700, 200, 200, 200],
    ['stretch', 100, 800, 100, 300],
    ['scale', 100, 200, 50, 100]
  ];
  const children = modes.map(([mode, designX, designWidth], index) => {
    const item = gridItem(`text-${mode}`, designWidth, 40);
    item.frame.x = designX;
    item.frame.y = index * 60;
    item.layout.sizingX = 'fixed';
    item.layout.constraints = { horizontal: mode, vertical: 'top' };
    return item;
  });
  const rootBase = createSceneNodeBase('frame', 'Free root', { x: 0, y: 0, width: 1000, height: 400 });
  const root = { ...rootBase, id: 'frame-free-root', children };
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 500, viewportHeight: 400 });
  for (const [mode, , , expectedX, expectedWidth] of modes) {
    assert.equal(solved.boxes.get(`text-${mode}`).x, expectedX);
    assert.equal(solved.boxes.get(`text-${mode}`).width, expectedWidth);
  }
});

test('vertical constraints and absolute children use the same responsive solver', () => {
  const overlay = gridItem('text-overlay', 100, 100);
  overlay.frame.x = 700;
  overlay.frame.y = 650;
  overlay.layout.sizingX = 'fixed';
  overlay.layout.position = 'absolute';
  overlay.layout.constraints = { horizontal: 'right', vertical: 'bottom' };
  const root = frame('frame-auto-root', 'vertical', [text('text-flow', 'Flow'), overlay], {
    width: 1000, height: 800, sizingX: 'fill', sizingY: 'fixed'
  });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 500, viewportHeight: 400 });
  assert.equal(solved.boxes.get('text-overlay').x, 200);
  assert.equal(solved.boxes.get('text-overlay').y, 250);
  assert.equal(solved.boxes.get('text-overlay').width, 100);
  assert.equal(solved.boxes.get('text-overlay').height, 100);
});

test('media with intrinsic dimensions preserves aspect ratio when its container fills width', () => {
  const base = createSceneNodeBase('media', 'Hero image', { x: 0, y: 0, width: 1600, height: 900 });
  const image = {
    ...base,
    id: 'media-hero',
    mediaType: 'image',
    assetId: 'asset-hero',
    alt: 'Product hero',
    intrinsicSize: { width: 1600, height: 900 },
    preserveAspectRatio: true,
    layout: { ...base.layout, sizingX: 'fill', sizingY: 'hug' }
  };
  const root = frame('frame-media-root', 'vertical', [image], {
    sizingX: 'fill', sizingY: 'hug',
    padding: { top: 20, right: 20, bottom: 20, left: 20 }
  });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 400 });
  assert.equal(solved.boxes.get('media-hero').width, 360);
  assert.equal(solved.boxes.get('media-hero').height, 202.5);
  assert.equal(solved.boxes.get(root.id).height, 242.5);
});

test('a browser text measurer can replace deterministic fallback metrics after fonts load', () => {
  const copy = text('text-browser-measured', '真实字体测量', 'fill', 'hug');
  const root = frame('frame-measured-root', 'vertical', [copy], {
    sizingX: 'fill', sizingY: 'hug',
    padding: { top: 20, right: 20, bottom: 20, left: 20 }
  });
  const requests = [];
  const solved = solveSceneLayout(documentWithRoot(root), {
    rootNodeId: root.id,
    viewportWidth: 400,
    textMeasurer(request) {
      requests.push(request);
      return { width: request.availableWidth, height: 77 };
    }
  });
  assert.equal(solved.boxes.get('text-browser-measured').height, 77);
  assert.equal(solved.boxes.get(root.id).height, 117);
  assert.ok(requests.some((request) => request.nodeId === 'text-browser-measured' && request.availableWidth === 360));
  assert.equal(requests.at(-1).typography.fontFamily, 'Inter');

  assert.throws(() => solveSceneLayout(documentWithRoot(root), {
    rootNodeId: root.id,
    viewportWidth: 400,
    textMeasurer: () => ({ width: Number.NaN, height: 20 })
  }), /Text measurer returned an invalid size/);
});

test('deterministic fallback measurement treats CJK and emoji as full-width glyphs', () => {
  const label = text('text-cjk-label', '产品', 'hug', 'hug');
  label.appearance.typography.fontSize = 14;
  label.appearance.typography.lineHeight = 1.5;
  const root = frame('frame-root', 'vertical', [label, text('text-cjk-body', 'AI 构建真实结构，人只需审阅、批注和锁定关键设计决策。', 'fill', 'hug')], {
    sizingX: 'fill', sizingY: 'hug', padding: { top: 0, right: 0, bottom: 0, left: 0 }
  });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 220 });
  assert.ok(solved.boxes.get('text-cjk-label').width > 28);
  assert.equal(solved.boxes.get('text-cjk-label').height, 21);
  assert.ok(solved.boxes.get('text-cjk-body').height > 24);
});

test('deterministic fallback keeps currency and numeric labels wide enough for browser fonts', () => {
  const root = frame('frame-root', 'horizontal', [
    text('text-price', '¥760', 'hug', 'hug'),
    text('text-index', '01', 'hug', 'hug')
  ], { sizingX: 'fill', sizingY: 'hug', padding: { top: 0, right: 0, bottom: 0, left: 0 } });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 300 });
  assert.ok(solved.boxes.get('text-price').width >= 45);
  assert.ok(solved.boxes.get('text-index').width >= 19);
});

test('deterministic fallback keeps compact ranges with Unicode dashes on one line', () => {
  const root = frame('frame-root', 'horizontal', [text('text-week', 'W10—12', 'hug', 'hug')], {
    sizingX: 'fill', sizingY: 'hug', padding: { top: 0, right: 0, bottom: 0, left: 0 }
  });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 300 });
  assert.ok(solved.boxes.get('text-week').width >= 70);
  assert.equal(solved.boxes.get('text-week').height, 24);
});

test('a max-width fill child stays centered on 4K and 8K parents without false overflow', () => {
  const content = frame('frame-bounded-content', 'vertical', [text('text-bounded', 'Bounded content', 'fill', 'hug')], {
    sizingX: 'fill', sizingY: 'hug', maxWidth: 1600,
    padding: { top: 20, right: 20, bottom: 20, left: 20 }
  });
  const root = frame('frame-wide-root', 'vertical', [content], {
    sizingX: 'fill', sizingY: 'hug', alignItems: 'center',
    padding: { top: 0, right: 0, bottom: 0, left: 0 }
  });
  const document = documentWithRoot(root);
  for (const viewportWidth of [3840, 7680]) {
    const solved = solveSceneLayout(document, { rootNodeId: root.id, viewportWidth });
    const contentBox = solved.boxes.get(content.id);
    assert.equal(contentBox.width, 1600);
    assert.equal(contentBox.x, (viewportWidth - 1600) / 2);
    assert.equal(solved.boxes.get(root.id).overflowX, false);
  }
});

test('media is remeasured after horizontal fill and grid columns allocate final width', () => {
  const mediaBase = createSceneNodeBase('media', 'Responsive image', { x: 0, y: 0, width: 1600, height: 900 });
  const makeMedia = (id) => ({
    ...structuredClone(mediaBase), id, mediaType: 'image', assetId: `asset-${id}`,
    intrinsicSize: { width: 1600, height: 900 }, preserveAspectRatio: true,
    layout: { ...mediaBase.layout, sizingX: 'fill', sizingY: 'hug' }
  });
  const split = frame('frame-split-media', 'horizontal', [makeMedia('media-a'), makeMedia('media-b')], {
    sizingX: 'fill', sizingY: 'hug', padding: { top: 0, right: 0, bottom: 0, left: 0 }, gap: { row: 20, column: 20 }
  });
  const splitSolved = solveSceneLayout(documentWithRoot(split), { rootNodeId: split.id, viewportWidth: 820 });
  assert.equal(splitSolved.boxes.get('media-a').width, 400);
  assert.equal(splitSolved.boxes.get('media-a').height, 225);

  const grid = gridFrame('frame-grid-media', [makeMedia('media-grid-a'), makeMedia('media-grid-b')], ['1fr', '1fr'], {
    sizingX: 'fill', sizingY: 'hug', padding: { top: 0, right: 0, bottom: 0, left: 0 }, gap: { row: 20, column: 20 }
  });
  const gridSolved = solveSceneLayout(documentWithRoot(grid), { rootNodeId: grid.id, viewportWidth: 820 });
  assert.equal(gridSolved.boxes.get('media-grid-a').width, 400);
  assert.equal(gridSolved.boxes.get('media-grid-a').height, 225);
  assert.equal(gridSolved.boxes.get(grid.id).height, 225);
});

test('nested hugging containers are remeasured after their final column width is known', () => {
  const copy = frame('frame-copy', 'vertical', [
    text('text-copy', 'A long nested paragraph must wrap after the parent column receives its final width, and the parent must grow to the wrapped text height.', 'fill', 'hug')
  ], {
    sizingX: 'fill', sizingY: 'hug',
    padding: { top: 12, right: 12, bottom: 12, left: 12 }
  });
  const sibling = frame('frame-sibling', 'vertical', [text('text-sibling', 'Sibling', 'fill', 'hug')], {
    sizingX: 'fill', sizingY: 'hug'
  });
  const horizontal = frame('frame-horizontal', 'horizontal', [copy, sibling], {
    width: 360, sizingX: 'fill', sizingY: 'hug',
    padding: { top: 10, right: 10, bottom: 10, left: 10 },
    gap: { row: 10, column: 20 }
  });
  const horizontalSolved = solveSceneLayout(documentWithRoot(horizontal), { rootNodeId: horizontal.id, viewportWidth: 360 });
  assert.ok(horizontalSolved.boxes.get('text-copy').height > 24);
  assert.equal(horizontalSolved.boxes.get('frame-copy').height, horizontalSolved.boxes.get('text-copy').height + 24);
  assert.equal(horizontalSolved.boxes.get('frame-horizontal').height, horizontalSolved.boxes.get('frame-copy').height + 20);

  const gridCopy = structuredClone(copy);
  gridCopy.id = 'frame-grid-copy';
  gridCopy.children[0].id = 'text-grid-copy';
  const grid = gridFrame('frame-nested-grid', [gridCopy, sibling], ['1fr', '1fr'], {
    width: 360, sizingX: 'fill', sizingY: 'hug',
    padding: { top: 10, right: 10, bottom: 10, left: 10 },
    gap: { row: 10, column: 20 }
  });
  const gridSolved = solveSceneLayout(documentWithRoot(grid), { rootNodeId: grid.id, viewportWidth: 360 });
  assert.ok(gridSolved.boxes.get('text-grid-copy').height > 24);
  assert.equal(gridSolved.boxes.get('frame-grid-copy').height, gridSolved.boxes.get('text-grid-copy').height + 24);
  assert.equal(gridSolved.boxes.get('frame-nested-grid').height, gridSolved.boxes.get('frame-grid-copy').height + 20);
});

test('a nested grid reports its natural row height to every hugging ancestor', () => {
  const cards = Array.from({ length: 4 }, (_, index) => frame(`frame-card-${index}`, 'vertical', [
    text(`text-card-${index}`, `Card ${index} contains enough responsive copy to wrap naturally inside a narrow grid column.`, 'fill', 'hug')
  ], { sizingX: 'fill', sizingY: 'hug' }));
  const grid = gridFrame('frame-grid', cards, ['repeat(auto-fit, minmax(140px, 1fr))'], {
    sizingX: 'fill', sizingY: 'hug',
    padding: { top: 10, right: 10, bottom: 10, left: 10 },
    gap: { row: 12, column: 12 }
  });
  const root = frame('frame-root', 'vertical', [grid], {
    sizingX: 'fill', sizingY: 'hug',
    padding: { top: 20, right: 20, bottom: 20, left: 20 }
  });
  const solved = solveSceneLayout(documentWithRoot(root), { rootNodeId: root.id, viewportWidth: 390 });
  assert.equal(solved.boxes.get('frame-root').height, solved.boxes.get('frame-grid').height + 40);
  assert.equal(solved.boxes.get('frame-grid').overflowY, false);
  assert.equal(solved.boxes.get('frame-root').overflowY, false);
  assert.ok(solved.boxes.get('frame-card-2').y > solved.boxes.get('frame-card-0').y);
});
