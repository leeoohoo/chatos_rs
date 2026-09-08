import { createBlankSceneDocument, createSceneNodeBase } from '../../dist/v2-scene-schema.test.mjs';

export function nestedWebsite() {
  const document = createBlankSceneDocument('Scene graph website');
  document.documentId = 'scene-test';
  document.pages[0].id = 'page-home';

  const heading = {
    ...createSceneNodeBase('text', 'Hero heading', { x: 0, y: 0, width: 560, height: 120 }, 'ai'),
    id: 'text-hero-heading',
    role: 'hero-heading',
    content: 'Design intent before coordinates',
    appearance: {
      ...createSceneNodeBase('text', 'appearance-source', { x: 0, y: 0, width: 1, height: 1 }).appearance,
      fills: [{ type: 'solid', visible: true, opacity: 1, color: '#111111' }],
      typography: { fontFamily: 'Inter', fontSize: 64, fontWeight: 700, lineHeight: 1.05, letterSpacing: -1.5, textAlign: 'left' }
    },
    layout: {
      ...createSceneNodeBase('text', 'layout-source', { x: 0, y: 0, width: 1, height: 1 }).layout,
      sizingX: 'fill',
      sizingY: 'hug'
    },
    aiPolicy: { editable: true, lockedFields: ['content'], intent: 'Primary value proposition' }
  };
  const group = {
    ...createSceneNodeBase('group', 'Hero copy', { x: 80, y: 80, width: 560, height: 120 }),
    id: 'group-hero-copy',
    layout: {
      ...createSceneNodeBase('group', 'layout-source', { x: 0, y: 0, width: 1, height: 1 }).layout,
      sizingX: 'fill',
      sizingY: 'hug'
    },
    children: [heading]
  };
  const frame = {
    ...createSceneNodeBase('frame', 'Desktop frame', { x: 0, y: 0, width: 1440, height: 900 }, 'ai'),
    id: 'frame-desktop',
    layout: {
      mode: 'auto',
      direction: 'vertical',
      wrap: false,
      padding: { top: 80, right: 80, bottom: 80, left: 80 },
      gap: { row: 32, column: 32 },
      alignItems: 'stretch',
      justifyContent: 'start',
      sizingX: 'fill',
      sizingY: 'hug',
      minHeight: 600,
      position: 'flow',
      clipContent: false
    },
    children: [group]
  };
  const section = {
    ...createSceneNodeBase('section', 'Responsive directions', { x: 0, y: 0, width: 1600, height: 1100 }),
    id: 'section-responsive',
    children: [frame]
  };
  document.pages[0].children = [section];
  document.variableCollections = [{
    id: 'variables-brand',
    name: 'Brand',
    modes: [{ id: 'mode-light', name: 'Light' }, { id: 'mode-dark', name: 'Dark' }],
    variables: [{ id: 'variable-surface', name: 'Surface', type: 'color', valuesByMode: { 'mode-light': '#FFFFFF', 'mode-dark': '#111111' } }]
  }];
  return document;
}
