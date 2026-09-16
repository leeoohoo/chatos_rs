import test from 'node:test';
import assert from 'node:assert/strict';
import {
  initialWorkspaceArtboards,
  reconcileWorkspaceArtboards,
  updateWorkspaceArtboardById
} from '../dist/workspace-artboard-model.test.mjs';

const document = {
  viewport: { width: 1200, height: 940, background: '#fff' },
  breakpoints: {
    desktop: { width: 1200, height: 940 },
    tablet: { width: 768, height: 1024 },
    mobile: { width: 390, height: 844 }
  },
  pages: [{ id: 'legacy-home', name: 'Legacy', slug: '/' }]
};

function root(id, width, sizingX = 'fixed') {
  return {
    id,
    frame: { x: 0, y: 0, width, height: 768 },
    layout: { mode: 'auto', sizingX },
    children: []
  };
}

const scene = {
  pages: [
    { id: 'home-desktop', name: 'Desktop 1440', children: [root('desktop-root', 1440)] },
    { id: 'home-mobile', name: 'Mobile 390', children: [root('mobile-root', 390)] }
  ]
};

test('new Scene workspaces use each fixed page root as the artboard viewport', () => {
  const artboards = initialWorkspaceArtboards(document, scene);
  assert.deepEqual(
    artboards.map(({ pageId, viewportWidth, viewportHeight, x }) => ({ pageId, viewportWidth, viewportHeight, x })),
    [
      { pageId: 'home-desktop', viewportWidth: 1440, viewportHeight: 900, x: 0 },
      { pageId: 'home-mobile', viewportWidth: 390, viewportHeight: 844, x: 0 }
    ]
  );
});

test('persisted artboard sizes remain independent from fixed Scene root geometry', () => {
  const stored = [
    { artboardId: 'artboard-home-desktop', pageId: 'home-desktop', surfaceKind: 'page', viewportWidth: 1200, viewportHeight: 900, x: 0, y: 0 },
    { artboardId: 'artboard-home-mobile', pageId: 'home-mobile', surfaceKind: 'page', viewportWidth: 1200, viewportHeight: 900, x: 1360, y: 0 }
  ];
  const artboards = reconcileWorkspaceArtboards(document, stored, scene);
  assert.deepEqual(
    artboards.map(({ pageId, viewportWidth, viewportHeight, x }) => ({ pageId, viewportWidth, viewportHeight, x })),
    [
      { pageId: 'home-desktop', viewportWidth: 1200, viewportHeight: 900, x: 0 },
      { pageId: 'home-mobile', viewportWidth: 1200, viewportHeight: 900, x: 0 }
    ]
  );
});

test('reconciliation discards obsolete world-space placement without resetting user sizes', () => {
  const stored = [
    { artboardId: 'artboard-home-desktop', pageId: 'home-desktop', surfaceKind: 'page', viewportWidth: 2560, viewportHeight: 1080, x: 0, y: 0 },
    { artboardId: 'artboard-home-mobile', pageId: 'home-mobile', surfaceKind: 'page', viewportWidth: 390, viewportHeight: 844, x: 1600, y: 0 }
  ];
  const artboards = reconcileWorkspaceArtboards(document, stored, scene);
  assert.deepEqual(
    artboards.map(({ pageId, viewportWidth, viewportHeight, x }) => ({ pageId, viewportWidth, viewportHeight, x })),
    [
      { pageId: 'home-desktop', viewportWidth: 2560, viewportHeight: 1080, x: 0 },
      { pageId: 'home-mobile', viewportWidth: 390, viewportHeight: 844, x: 0 }
    ]
  );
});

test('responsive fill roots keep the user-selected workspace viewport', () => {
  const responsiveScene = {
    pages: [{ id: 'home', name: 'Responsive', children: [root('home-root', 1440, 'fill')] }]
  };
  const stored = [
    { artboardId: 'artboard-home', pageId: 'home', surfaceKind: 'page', viewportWidth: 1200, viewportHeight: 900, x: 20, y: 30 }
  ];
  assert.deepEqual(reconcileWorkspaceArtboards(document, stored, responsiveScene), [
    { ...stored[0], x: 0, y: 0 }
  ]);
});

test('artboard-scoped toolbar updates only the explicitly active artboard', () => {
  const artboards = initialWorkspaceArtboards(document, scene);
  const updated = updateWorkspaceArtboardById(artboards, 'artboard-home-mobile', {
    viewportWidth: 430,
    viewportHeight: 932
  });
  assert.deepEqual(
    updated.map(({ pageId, viewportWidth, viewportHeight }) => ({ pageId, viewportWidth, viewportHeight })),
    [
      { pageId: 'home-desktop', viewportWidth: 1440, viewportHeight: 900 },
      { pageId: 'home-mobile', viewportWidth: 430, viewportHeight: 932 }
    ]
  );
  assert.notEqual(updated[0], artboards[0]);
  assert.notEqual(updated[1], artboards[1]);
});

test('widening one focused artboard never changes another artboard', () => {
  const artboards = initialWorkspaceArtboards(document, scene);
  const updated = updateWorkspaceArtboardById(artboards, 'artboard-home-desktop', {
    viewportWidth: 2560,
    viewportHeight: 1080
  });
  assert.deepEqual(
    updated.map(({ pageId, viewportWidth, viewportHeight, x }) => ({ pageId, viewportWidth, viewportHeight, x })),
    [
      { pageId: 'home-desktop', viewportWidth: 2560, viewportHeight: 1080, x: 0 },
      { pageId: 'home-mobile', viewportWidth: 390, viewportHeight: 844, x: 0 }
    ]
  );
});
