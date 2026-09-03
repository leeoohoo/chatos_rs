import assert from 'node:assert/strict';
import test from 'node:test';
import { applyWebDesignPatch, assertWebDesignDocument } from '../dist/schema.test.mjs';
import {
  autoLayoutContainer,
  breakpointFor,
  cloneComponentSubtrees,
  componentsForPage,
  createSymbolFromSelection,
  detachSymbolInstance,
  flattenComponentTree,
  fitContentCanvasToComponents,
  growPageToFitContent,
  instantiateSymbol,
  moveComponentsWithDescendants,
  reflowPageForViewport,
  resolveComponent,
  setSymbolOverride,
  syncSymbolInstances,
  updateSymbolFromInstance,
  snapComponentFrame
} from '../dist/editor-model.test.mjs';
import { componentDefaults, createLandingPage } from '../dist/templates.test.mjs';
import { createBlockPreset, createPageTemplate, WEB_DESIGN_BLOCK_PRESETS, WEB_DESIGN_PAGE_TEMPLATES } from '../dist/component-library.test.mjs';
import { ANTD_CATEGORIES, ANTD_COMPONENTS, ANTD_OFFICIAL_COMPONENT_COUNT, ANTD_VERSION, applyAntdComponentVariant, createAntdComponent, variantsForAntdComponent } from '../dist/antd-library.test.mjs';
import { editableSlotsForAntdComponent, isAntdContentContainer } from '../dist/antd-slots.test.mjs';
import { CHAKRA_CATEGORIES, CHAKRA_COMPONENTS, CHAKRA_VERSION, applyChakraComponentVariant, createChakraComponent, variantsForChakraComponent } from '../dist/chakra-library.test.mjs';
import { SHADCN_CATEGORIES, SHADCN_COMPONENTS, SHADCN_VERSION, applyShadcnComponentVariant, createShadcnComponent, variantsForShadcnComponent } from '../dist/shadcn-library.test.mjs';
import { editableSlotsForUiComponent, growUiContentContainersToFit, isUiContentContainer } from '../dist/library-slots.test.mjs';
import { UI_LIBRARIES, applyUiLibraryVariant, createComponentFromUiLibrary } from '../dist/ui-libraries.test.mjs';
import { matchViewportPreset, viewportDimensions, viewportPresetsForDevice, WEB_DESIGN_VIEWPORT_PRESETS } from '../dist/viewport-presets.test.mjs';
import { exportDocumentHtmlFiles, exportPageHtml } from '../dist/html-exporter.test.mjs';
import { exportReactComponent } from '../dist/react-exporter.test.mjs';
import { exportVueComponent } from '../dist/vue-exporter.test.mjs';

test('landing page template is a valid editable document', () => {
  const document = createLandingPage('Test Website');
  assertWebDesignDocument(document);
  assert.equal(document.title, 'Test Website');
  assert.ok(document.components.length >= 8);
  assert.equal(document.pages.length, 1);
  assert.ok(document.components.some((component) => component.id === 'hero-heading'));
  assert.equal(breakpointFor(document, 'mobile').width, 390);
  assert.equal(resolveComponent(document.components.find((component) => component.id === 'hero-heading'), 'mobile').style.fontSize, 39);
});

test('canvas height grows to fit visible page content without shrinking other device breakpoints', () => {
  const document = createLandingPage();
  const desktopBefore = breakpointFor(document, 'desktop');
  const tabletBefore = breakpointFor(document, 'tablet');
  const textarea = componentDefaults('textarea', 32, desktopBefore.height + 25);
  textarea.pageId = 'home';
  textarea.height = 140;
  document.components = [textarea];

  const grown = growPageToFitContent(document, 'home', 'desktop');
  const expectedHeight = Math.ceil((textarea.y + textarea.height + 80) / 100) * 100;
  assert.equal(breakpointFor(grown, 'desktop').height, expectedHeight);
  assert.equal(grown.viewport.height, expectedHeight);
  assert.deepEqual(breakpointFor(grown, 'tablet'), tabletBefore);

  const unchanged = growPageToFitContent(grown, 'home', 'desktop');
  assert.equal(unchanged, grown);
});

test('canvas growth respects device overrides, hidden components, and excluded overlay content', () => {
  const document = createLandingPage();
  const desktopBefore = breakpointFor(document, 'desktop');
  const mobileBefore = breakpointFor(document, 'mobile');
  const component = componentDefaults('textarea', 20, 20);
  component.pageId = 'home';
  component.responsive = {
    mobile: { x: 20, y: mobileBefore.height + 40, width: 350, height: 180 }
  };
  document.components = [component];

  const mobileGrown = growPageToFitContent(document, 'home', 'mobile');
  assert.ok(breakpointFor(mobileGrown, 'mobile').height > mobileBefore.height);
  assert.deepEqual(breakpointFor(mobileGrown, 'desktop'), desktopBefore);
  assert.equal(mobileGrown.viewport.height, desktopBefore.height);

  component.hidden = true;
  const hiddenDocument = { ...document, components: [component] };
  assert.equal(growPageToFitContent(hiddenDocument, 'home', 'mobile'), hiddenDocument);

  component.hidden = false;
  const excludedDocument = { ...document, components: [component] };
  assert.equal(growPageToFitContent(excludedDocument, 'home', 'mobile', { excludedComponentIds: new Set([component.id]) }), excludedDocument);
});

test('nested content canvas grows in both dimensions from visible component bounds', () => {
  const first = componentDefaults('input', 120, 90);
  first.width = 300;
  first.height = 40;
  const second = componentDefaults('table', 610, 540);
  second.width = 420;
  second.height = 260;
  const hidden = componentDefaults('textarea', 5000, 9000);
  hidden.hidden = true;

  assert.deepEqual(fitContentCanvasToComponents([], 'desktop', {
    minimumWidth: 440,
    minimumHeight: 320,
    originX: 100,
    originY: 70
  }), { width: 440, height: 320 });

  assert.deepEqual(fitContentCanvasToComponents([first, second, hidden], 'desktop', {
    minimumWidth: 440,
    minimumHeight: 320,
    originX: 100,
    originY: 70,
    padding: 48,
    step: 80
  }), { width: 1040, height: 800 });

  second.responsive = { mobile: { x: 350, y: 420, width: 280, height: 180 } };
  assert.deepEqual(fitContentCanvasToComponents([second], 'mobile', {
    minimumWidth: 320,
    minimumHeight: 300,
    originX: 100,
    originY: 70,
    padding: 48,
    step: 80
  }), { width: 640, height: 640 });
});

test('viewport presets expose real CSS viewport sizes and support rotation', () => {
  assert.equal(viewportPresetsForDevice('desktop').length, 14);
  assert.equal(viewportPresetsForDevice('tablet').length, 5);
  assert.equal(viewportPresetsForDevice('mobile').length, 5);
  assert.equal(WEB_DESIGN_VIEWPORT_PRESETS.every((preset) => preset.width >= 320 && preset.height >= 320), true);

  const iphone = WEB_DESIGN_VIEWPORT_PRESETS.find((preset) => preset.id === 'iphone-16');
  assert.deepEqual(viewportDimensions(iphone, 'default'), { width: 393, height: 852 });
  assert.deepEqual(viewportDimensions(iphone, 'rotated'), { width: 852, height: 393 });
  assert.deepEqual(matchViewportPreset('mobile', 852), { preset: iphone, orientation: 'rotated' });
  assert.equal(matchViewportPreset('mobile', 401), undefined);

  const eightK = WEB_DESIGN_VIEWPORT_PRESETS.find((preset) => preset.id === 'desktop-8k');
  assert.deepEqual(viewportDimensions(eightK, 'default'), { width: 7680, height: 4320 });
  assert.equal(eightK.group, 'large-display');

  const document = createLandingPage();
  document.breakpoints.desktop.preview = { presetId: 'desktop-8k', orientation: 'default', viewportHeight: 4320 };
  assertWebDesignDocument(document);
  const resized = applyWebDesignPatch(document, [{ op: 'set_breakpoint', device: 'desktop', width: 3840, height: 1800 }]);
  assert.deepEqual(resized.breakpoints.desktop.preview, document.breakpoints.desktop.preview);
});

test('responsive constraints reflow nested components without overflowing a narrower viewport', () => {
  const document = createLandingPage();
  const section = componentDefaults('section', 384, 100);
  section.id = 'responsive-section';
  section.pageId = 'home';
  section.width = 6912;
  section.height = 500;
  const button = componentDefaults('button', 3740, 260);
  button.id = 'responsive-button';
  button.pageId = 'home';
  button.parentId = section.id;
  button.width = 200;
  button.constraints = { desktop: { horizontal: 'auto' } };
  document.components = [section, button];

  const narrowed = reflowPageForViewport(document, 'home', 'desktop', 7680, 1200);
  const sectionFrame = resolveComponent(narrowed.components.find((component) => component.id === section.id), 'desktop');
  const buttonFrame = resolveComponent(narrowed.components.find((component) => component.id === button.id), 'desktop');
  assert.deepEqual({ x: sectionFrame.x, width: sectionFrame.width }, { x: 60, width: 1080 });
  assert.deepEqual({ x: buttonFrame.x, width: buttonFrame.width }, { x: 500, width: 200 });
  assert.ok(sectionFrame.x + sectionFrame.width <= 1200);
  assert.ok(buttonFrame.x >= sectionFrame.x && buttonFrame.x + buttonFrame.width <= sectionFrame.x + sectionFrame.width);
  assertWebDesignDocument(narrowed);
});

test('explicit left, center, right, stretch, and scale constraints remain predictable', () => {
  const document = createLandingPage();
  const modes = ['left', 'center', 'right', 'stretch', 'scale'];
  document.components = modes.map((horizontal, index) => {
    const component = componentDefaults('section', 100, 40 + index * 100);
    component.id = `constraint-${horizontal}`;
    component.pageId = 'home';
    component.width = 1000;
    component.height = 80;
    component.constraints = { desktop: { horizontal } };
    return component;
  });
  const widened = reflowPageForViewport(document, 'home', 'desktop', 1200, 1920);
  const frame = (mode) => resolveComponent(widened.components.find((component) => component.id === `constraint-${mode}`), 'desktop');
  assert.deepEqual({ x: frame('left').x, width: frame('left').width }, { x: 100, width: 1000 });
  assert.deepEqual({ x: frame('center').x, width: frame('center').width }, { x: 460, width: 1000 });
  assert.deepEqual({ x: frame('right').x, width: frame('right').width }, { x: 820, width: 1000 });
  assert.deepEqual({ x: frame('stretch').x, width: frame('stretch').width }, { x: 100, width: 1720 });
  assert.deepEqual({ x: frame('scale').x, width: frame('scale').width }, { x: 160, width: 1600 });

  const patched = applyWebDesignPatch(document, [{
    op: 'update_component', componentId: 'constraint-left', device: 'desktop',
    changes: { constraints: { desktop: { horizontal: 'right' } } }
  }]);
  assert.equal(patched.components.find((component) => component.id === 'constraint-left').constraints.desktop.horizontal, 'right');
  assertWebDesignDocument(patched);
});

test('product component library provides complete valid defaults', () => {
  const types = ['section', 'text', 'heading', 'button', 'link', 'image', 'icon', 'logo', 'card', 'input', 'textarea', 'select', 'checkbox', 'switch', 'divider', 'badge', 'avatar', 'list', 'table', 'video'];
  const document = createLandingPage();
  const components = types.map((type, index) => ({ ...componentDefaults(type, 20 + index * 5, 20 + index * 5), pageId: 'home', zIndex: index + 20 }));
  document.components = components;
  assertWebDesignDocument(document);
  assert.deepEqual(components.map((component) => component.type), types);
  assert.equal(components.every((component) => component.name && component.width >= 16 && component.height >= 16), true);
});

test('finished block presets insert valid responsive component trees', () => {
  let document = createLandingPage();
  for (const preset of WEB_DESIGN_BLOCK_PRESETS) {
    const block = createBlockPreset(document, 'home', preset.id);
    assert.ok(block.components.length >= 5);
    assert.ok(block.rootIds.length >= 1);
    assert.equal(block.components.every((component) => component.pageId === 'home' && component.responsive?.mobile), true);
    document = { ...document, components: [...document.components, ...block.components] };
    assertWebDesignDocument(document);
  }
  assert.deepEqual(WEB_DESIGN_BLOCK_PRESETS.map((preset) => preset.id), ['navbar', 'hero', 'features', 'pricing', 'faq', 'contact', 'footer']);
});

test('complete page templates replace the active page and remain valid on every device', () => {
  for (const template of WEB_DESIGN_PAGE_TEMPLATES) {
    const document = createLandingPage();
    const legacyHomeComponent = document.components[0];
    delete legacyHomeComponent.pageId;
    document.pages.push({ id: 'about', name: '关于', slug: '/about' });
    const aboutComponent = structuredClone(document.components[1]);
    aboutComponent.id = `about-${template.id}`;
    aboutComponent.pageId = 'about';
    aboutComponent.parentId = undefined;
    document.components.push(aboutComponent);

    const page = createPageTemplate(document, 'home', template.id);
    assert.equal(page.components.find((component) => page.rootIds.includes(component.id))?.y, 40);
    const replaced = { ...document, components: [aboutComponent, ...page.components] };
    assertWebDesignDocument(replaced);
    assert.equal(page.components.some((component) => component.id === legacyHomeComponent.id), false);
    assert.equal(replaced.components.some((component) => component.id === aboutComponent.id), true);
    assert.equal(page.components.every((component) => component.pageId === 'home'), true);
    for (const device of ['desktop', 'tablet', 'mobile']) {
      const frames = page.components.map((component) => resolveComponent(component, device));
      assert.equal(frames.every((frame) => frame.width >= 16 && frame.height >= 16), true);
      assert.ok(Math.max(...frames.map((frame) => frame.y + frame.height)) > breakpointFor(document, device).height);
    }
  }
  assert.deepEqual(WEB_DESIGN_PAGE_TEMPLATES.map((template) => template.id), ['saas', 'launch', 'business']);
});

test('Ant Design 6.6.2 catalog matches the current official component baseline', () => {
  const document = createLandingPage();
  const components = ANTD_COMPONENTS.map((definition, index) => {
    const component = createAntdComponent(definition.id, 20 + (index % 5) * 180, 20 + Math.floor(index / 5) * 90);
    component.pageId = 'home';
    component.zIndex = index + 1;
    return component;
  });
  document.components = components;
  assert.equal(ANTD_VERSION, '6.6.2');
  assert.equal(ANTD_OFFICIAL_COMPONENT_COUNT, 72);
  assert.equal(ANTD_COMPONENTS.length, ANTD_OFFICIAL_COMPONENT_COUNT);
  assert.equal(ANTD_COMPONENTS.every((component) => variantsForAntdComponent(component.id).length >= 2), true);
  assert.deepEqual([...new Set(ANTD_COMPONENTS.map((component) => component.category))], ANTD_CATEGORIES);
  assert.deepEqual(ANTD_COMPONENTS.filter((component) => component.introduced).map((component) => [component.id, component.introduced]), [['Listy', '6.6.0'], ['BorderBeam', '6.4.0']]);
  assert.equal(ANTD_COMPONENTS.find((component) => component.id === 'List')?.status, 'deprecated');
  assert.equal(ANTD_COMPONENTS.every((component) => component.docsUrl?.startsWith('https://ant.design/components/')), true);
  assert.equal(components.every((component) => component.library?.name === 'antd' && component.library.version === ANTD_VERSION), true);
  assertWebDesignDocument(document);

  const html = exportPageHtml(document, 'home');
  assert.match(html, /data-library="antd"/);
  assert.match(html, /data-library-component="Button"/);
  const react = exportReactComponent(document);
  assert.match(react.content, /import \* as Antd from 'antd'/);
  assert.match(react.content, /AntDesignComponent/);
});

test('Ant Design variants and structured sample data remain editable and persistent', () => {
  const input = applyAntdComponentVariant(createAntdComponent('Input', 20, 20), 'search');
  assert.equal(input.library.variant, 'search');
  assert.equal(input.library.props.enterButton, true);
  assert.equal(variantsForAntdComponent('Input').length, 9);
  assert.equal(variantsForAntdComponent('Select').length, 9);
  assert.equal(variantsForAntdComponent('List').length, 8);
  assert.equal(variantsForAntdComponent('Listy').length, 8);
  assert.equal(variantsForAntdComponent('BorderBeam').length, 5);
  assert.equal(variantsForAntdComponent('Menu').length, 4);
  assert.equal(variantsForAntdComponent('Drawer').length, 7);
  assert.equal(variantsForAntdComponent('Table').length, 7);

  const select = createAntdComponent('Select', 20, 80);
  select.library.props.options = [{ value: 'custom', label: '自定义选项' }];
  const multiple = applyAntdComponentVariant(select, 'multiple');
  assert.equal(multiple.library.variant, 'multiple');
  assert.equal(multiple.library.props.mode, 'multiple');
  assert.deepEqual(multiple.library.props.options, [{ value: 'custom', label: '自定义选项' }]);

  const document = createLandingPage();
  input.pageId = 'home';
  multiple.pageId = 'home';
  document.components = [input, multiple];
  assertWebDesignDocument(document);

  const expandedFamilies = ['List', 'Table', 'Menu', 'Tabs', 'Collapse', 'Form', 'Drawer', 'Modal', 'Slider', 'Tree'];
  document.components = expandedFamilies.flatMap((componentId, familyIndex) => variantsForAntdComponent(componentId).map((variant, variantIndex) => {
    const component = applyAntdComponentVariant(createAntdComponent(componentId, 20 + variantIndex * 40, 20 + familyIndex * 40), variant.id);
    component.pageId = 'home';
    component.zIndex = familyIndex * 10 + variantIndex + 1;
    return component;
  }));
  assertWebDesignDocument(document);

  document.components = ANTD_COMPONENTS.flatMap((definition, familyIndex) => variantsForAntdComponent(definition.id).map((variant, variantIndex) => {
    const component = applyAntdComponentVariant(createAntdComponent(definition.id, 20 + variantIndex * 30, 20 + familyIndex * 30), variant.id);
    component.pageId = 'home';
    component.zIndex = familyIndex * 10 + variantIndex + 1;
    return component;
  }));
  assertWebDesignDocument(document);
});

test('Chakra UI and shadcn/ui catalogs create valid independently bound components', () => {
  assert.equal(CHAKRA_VERSION, '3.37.0');
  assert.equal(SHADCN_VERSION, 'registry-2026.09');
  assert.equal(CHAKRA_COMPONENTS.length, 114);
  const officialShadcnComponents = [
    'Accordion', 'Alert', 'AlertDialog', 'AspectRatio', 'Attachment', 'Avatar', 'Badge', 'Breadcrumb', 'Bubble', 'Button',
    'ButtonGroup', 'Calendar', 'Card', 'Carousel', 'Chart', 'Checkbox', 'Collapsible', 'Combobox', 'Command', 'ContextMenu',
    'DataTable', 'DatePicker', 'Dialog', 'Direction', 'Drawer', 'DropdownMenu', 'Empty', 'Field', 'HoverCard', 'Input',
    'InputGroup', 'InputOTP', 'Item', 'Kbd', 'Label', 'Marker', 'Menubar', 'Message', 'MessageScroller', 'NativeSelect',
    'NavigationMenu', 'Pagination', 'Popover', 'Progress', 'Questionnaire', 'RadioGroup', 'Resizable', 'ScrollArea', 'Select',
    'Separator', 'Sheet', 'Sidebar', 'Skeleton', 'Slider', 'Spinner', 'Switch', 'Table', 'Tabs', 'Textarea', 'Toast',
    'Toggle', 'ToggleGroup', 'Tooltip', 'Typography'
  ];
  assert.deepEqual(SHADCN_COMPONENTS.map((component) => component.id).sort(), officialShadcnComponents.sort());
  assert.deepEqual([...new Set(CHAKRA_COMPONENTS.map((component) => component.category))], CHAKRA_CATEGORIES);
  assert.deepEqual([...new Set(SHADCN_COMPONENTS.map((component) => component.category))], SHADCN_CATEGORIES);
  assert.deepEqual(UI_LIBRARIES.map((library) => library.id), ['antd', 'chakra', 'shadcn']);

  const chakraInput = applyChakraComponentVariant(createChakraComponent('Input', 20, 20), 'subtle');
  const shadcnButton = applyShadcnComponentVariant(createShadcnComponent('Button', 20, 80), 'destructive');
  assert.equal(chakraInput.library.name, 'chakra');
  assert.equal(chakraInput.library.props.variant, 'subtle');
  assert.equal(shadcnButton.library.name, 'shadcn');
  assert.equal(shadcnButton.library.props.variant, 'destructive');
  assert.ok(variantsForChakraComponent('Button').length >= 5);
  assert.deepEqual(variantsForChakraComponent('List').map((variant) => variant.id), [
    'basic', 'ordered', 'icon-check', 'icon-info', 'nested', 'custom-marker', 'plain', 'align-end'
  ]);
  assert.equal(CHAKRA_COMPONENTS.every((definition) => variantsForChakraComponent(definition.id).length >= 2), true);
  assert.ok(variantsForShadcnComponent('Button').length >= 6);
  assert.equal(SHADCN_COMPONENTS.every((definition) => variantsForShadcnComponent(definition.id).length >= 2), true);

  for (const definition of CHAKRA_COMPONENTS) {
    const variants = variantsForChakraComponent(definition.id);
    assert.equal(new Set(variants.map((variant) => variant.id)).size, variants.length);
    for (const variant of variants) {
      const component = applyChakraComponentVariant(createChakraComponent(definition.id, 24, 24), variant.id);
      assert.equal(component.library.name, 'chakra');
      assert.equal(component.library.component, definition.id);
      assert.equal(component.library.variant, variant.id);
      const variantDocument = createLandingPage();
      component.pageId = 'home';
      variantDocument.components = [component];
      assertWebDesignDocument(variantDocument);
    }
  }

  for (const definition of SHADCN_COMPONENTS) {
    const variants = variantsForShadcnComponent(definition.id);
    assert.equal(new Set(variants.map((variant) => variant.id)).size, variants.length);
    for (const variant of variants) {
      const component = applyShadcnComponentVariant(createShadcnComponent(definition.id, 24, 24), variant.id);
      assert.equal(component.library.name, 'shadcn');
      assert.equal(component.library.component, definition.id);
      assert.equal(component.library.variant, variant.id);
      const variantDocument = createLandingPage();
      component.pageId = 'home';
      variantDocument.components = [component];
      assertWebDesignDocument(variantDocument);
    }
  }

  const document = createLandingPage();
  document.components = UI_LIBRARIES.flatMap((library, libraryIndex) => library.components.map((definition, index) => {
    const component = createComponentFromUiLibrary(library.id, definition.id, 20 + (index % 5) * 180, 20 + libraryIndex * 2000 + Math.floor(index / 5) * 90);
    component.pageId = 'home';
    component.zIndex = libraryIndex * 100 + index + 1;
    return applyUiLibraryVariant(component, component.library.variant ?? 'default');
  }));
  assertWebDesignDocument(document);
});

test('content slots use the same contract across Ant Design, Chakra UI, and shadcn/ui', () => {
  const chakraDrawer = createChakraComponent('Drawer', 80, 80);
  const chakraTabs = createChakraComponent('Tabs', 80, 140);
  const shadcnSheet = createShadcnComponent('Sheet', 80, 200);
  const shadcnAccordion = createShadcnComponent('Accordion', 80, 260);
  assert.deepEqual(editableSlotsForUiComponent(chakraDrawer).map((slot) => slot.id), ['content']);
  assert.deepEqual(editableSlotsForUiComponent(chakraTabs).map((slot) => slot.id), ['tab-overview', 'tab-features', 'tab-settings']);
  assert.deepEqual(editableSlotsForUiComponent(shadcnSheet).map((slot) => slot.id), ['content']);
  assert.deepEqual(editableSlotsForUiComponent(shadcnAccordion).map((slot) => slot.id), ['panel-design', 'panel-ai', 'panel-export']);
  assert.equal(isUiContentContainer(chakraDrawer), true);
  assert.equal(isUiContentContainer(shadcnSheet), true);
});

test('content-bearing Ant Design components expose editable single and multi-region slots', () => {
  const drawer = createAntdComponent('Drawer', 120, 80);
  const modal = createAntdComponent('Modal', 120, 140);
  const card = createAntdComponent('Card', 120, 200);
  const tabs = createAntdComponent('Tabs', 120, 260);
  const collapse = createAntdComponent('Collapse', 120, 320);
  const layout = createAntdComponent('Layout', 120, 380);
  const app = createAntdComponent('App', 120, 440);
  const configProvider = createAntdComponent('ConfigProvider', 120, 500);
  const borderBeam = createAntdComponent('BorderBeam', 120, 560);
  assert.deepEqual(editableSlotsForAntdComponent(drawer).map((slot) => slot.id), ['content']);
  assert.deepEqual(editableSlotsForAntdComponent(modal).map((slot) => slot.id), ['content']);
  assert.deepEqual(editableSlotsForAntdComponent(card).map((slot) => slot.id), ['content']);
  assert.deepEqual(editableSlotsForAntdComponent(tabs).map((slot) => slot.id), ['tab-1', 'tab-2', 'tab-3']);
  assert.deepEqual(editableSlotsForAntdComponent(collapse).map((slot) => slot.id), ['panel-1', 'panel-2']);
  assert.deepEqual(editableSlotsForAntdComponent(layout).map((slot) => slot.id), ['header', 'content', 'sider']);
  assert.deepEqual(editableSlotsForAntdComponent(app).map((slot) => slot.id), ['content']);
  assert.deepEqual(editableSlotsForAntdComponent(configProvider).map((slot) => slot.id), ['content']);
  assert.deepEqual(editableSlotsForAntdComponent(borderBeam).map((slot) => slot.id), ['content']);
  assert.equal(isAntdContentContainer(drawer), true);

  const input = createAntdComponent('Input', 132, 152);
  drawer.pageId = 'home';
  input.pageId = 'home';
  input.parentId = drawer.id;
  input.slot = 'content';
  const document = createLandingPage();
  document.components = [drawer, input];
  assertWebDesignDocument(document);
});

test('content containers grow with their nested canvas while overlay and scroll viewports stay fixed', () => {
  const document = createLandingPage();
  const form = createAntdComponent('Form', 100, 200);
  form.pageId = 'home';
  form.width = 360;
  form.height = 320;
  const wideChild = createAntdComponent('Typography', 820, 540);
  wideChild.pageId = 'home';
  wideChild.parentId = form.id;
  wideChild.slot = 'content';
  wideChild.width = 420;
  wideChild.height = 180;

  const drawer = createAntdComponent('Drawer', 80, 80);
  drawer.pageId = 'home';
  const drawerChild = createAntdComponent('Typography', 900, 900);
  drawerChild.pageId = 'home';
  drawerChild.parentId = drawer.id;
  drawerChild.slot = 'content';
  drawerChild.width = 600;
  drawerChild.height = 500;

  document.components = [form, wideChild, drawer, drawerChild];
  const grown = growUiContentContainersToFit(document, 'home', 'desktop');
  const grownForm = resolveComponent(grown.components.find((component) => component.id === form.id), 'desktop');
  const unchangedDrawer = resolveComponent(grown.components.find((component) => component.id === drawer.id), 'desktop');
  assert.ok(grownForm.width > form.width);
  assert.ok(grownForm.height > form.height);
  assert.equal(grownForm.width, 1200);
  assert.equal(grownForm.height, 640);
  assert.equal(unchangedDrawer.width, drawer.width);
  assert.equal(unchangedDrawer.height, drawer.height);
});

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

test('reusable components can be saved, validated, and instantiated on another page', () => {
  const document = createLandingPage();
  document.pages.push({ id: 'about', name: '关于', slug: '/about' });
  const symbol = createSymbolFromSelection(document, ['hero-section'], 'Hero 组件');
  const withSymbol = applyWebDesignPatch(document, [{ op: 'upsert_symbol', symbol }]);
  const instance = instantiateSymbol(withSymbol, symbol, 'about');
  const next = { ...withSymbol, components: [...withSymbol.components, ...instance.components] };
  assertWebDesignDocument(next);
  assert.equal(symbol.components.length, 5);
  assert.equal(instance.components.every((component) => component.pageId === 'about'), true);
  assert.equal(instance.components.every((component) => component.symbolId === symbol.id), true);
  assert.equal(instance.components.find((component) => component.id === instance.rootIds[0]).x, 80);
  assert.equal(instance.components.find((component) => component.id === instance.rootIds[0]).y, 80);
  const appended = instantiateSymbol(withSymbol, symbol, 'home');
  assert.ok(appended.components.find((component) => component.id === appended.rootIds[0]).y > 800);
});

test('reusable component instances synchronize definitions while preserving overrides and placement', () => {
  const base = createLandingPage();
  base.pages.push({ id: 'about', name: '关于', slug: '/about' });
  const symbol = createSymbolFromSelection(base, ['hero-section'], 'Hero 组件');
  const withSymbol = { ...base, symbols: [symbol] };
  const aboutInstance = instantiateSymbol(withSymbol, symbol, 'about');
  const homeInstance = instantiateSymbol(withSymbol, symbol, 'home');
  let document = { ...withSymbol, components: [...withSymbol.components, ...aboutInstance.components, ...homeInstance.components] };
  const aboutHeading = document.components.find((component) => component.symbolInstanceId === aboutInstance.components[0].symbolInstanceId && component.symbolComponentId === 'hero-heading');
  const aboutRoot = document.components.find((component) => component.id === aboutInstance.rootIds[0]);
  const homeHeading = document.components.find((component) => component.symbolInstanceId === homeInstance.components[0].symbolInstanceId && component.symbolComponentId === 'hero-heading');
  const aboutRootBefore = resolveComponent(aboutRoot, 'desktop');
  aboutHeading.content = '关于页保留标题';
  Object.assign(aboutHeading, setSymbolOverride(aboutHeading, 'content', true));
  aboutRoot.width = 777;
  Object.assign(aboutRoot, setSymbolOverride(aboutRoot, 'frame', true));
  document.symbols[0].components.find((component) => component.id === 'hero-heading').content = '组件库新标题';
  document.symbols[0].components.find((component) => component.id === 'hero-heading').style.color = '#123456';

  document = syncSymbolInstances(document, symbol.id);
  assert.equal(document.components.find((component) => component.id === aboutHeading.id).content, '关于页保留标题');
  assert.equal(document.components.find((component) => component.id === homeHeading.id).content, '组件库新标题');
  assert.equal(document.components.find((component) => component.id === homeHeading.id).style.color, '#123456');
  assert.equal(document.components.find((component) => component.id === aboutRoot.id).width, 777);
  assert.equal(document.components.find((component) => component.id === aboutRoot.id).x, aboutRootBefore.x);
  assert.equal(document.components.find((component) => component.id === aboutRoot.id).y, aboutRootBefore.y);
  assertWebDesignDocument(document);

  document.components.find((component) => component.id === homeHeading.id).content = '从首页实例更新';
  document = updateSymbolFromInstance(document, homeHeading.id);
  assert.equal(document.symbols[0].components.find((component) => component.id === 'hero-heading').content, '从首页实例更新');
  assert.equal(document.components.find((component) => component.id === aboutHeading.id).content, '关于页保留标题');

  const detached = detachSymbolInstance(document, aboutHeading.id);
  const detachedGroup = detached.components.filter((component) => component.symbolInstanceId === aboutHeading.symbolInstanceId);
  assert.equal(detachedGroup.length, 0);
  assert.equal(detached.components.find((component) => component.id === aboutHeading.id).symbolId, undefined);
  assertWebDesignDocument(detached);
});
