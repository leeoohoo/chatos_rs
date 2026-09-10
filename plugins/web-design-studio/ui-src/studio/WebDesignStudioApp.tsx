import { useEffect, useMemo, useReducer, useRef, useState, type CSSProperties, type DragEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import { flushSync } from 'react-dom';
import {
  autoLayoutContainer,
  breakpointFor,
  cloneComponentSubtrees,
  constrainComponentFrame,
  componentsForPage,
  createSymbolFromSelection,
  deriveResponsivePageFromDevice,
  detachSymbolInstance,
  descendantIds,
  flattenComponentTree,
  fitContentCanvasToComponents,
  growPageToFitContent,
  moveComponentsWithDescendants,
  reflowPageForViewport,
  instantiateSymbol,
  resolveComponent,
  selectedRootIds,
  setSymbolOverride,
  snapComponentFrame,
  syncSymbolInstances,
  updateComponentFrame,
  updateComponentStyle,
  updateSymbolFromInstance,
  type ResolvedWebDesignComponent,
  type SnapGuides
} from '../../src/editor-model';
import { exportPageHtml } from '../../src/html-exporter';
import { exportReactComponent } from '../../src/react-exporter';
import { exportVueComponent } from '../../src/vue-exporter';
import {
  componentsInSlot,
  editableSlotsForUiComponent,
  growUiContentContainersToFit,
  isOverlayUiContentContainer,
  isUiContentContainer,
  slotIdForDescendant,
  visibleComponentsInSlot
} from '../../src/library-slots';
import { applyUiLibraryVariant, createComponentFromUiLibrary, uiLibraryByName, UI_LIBRARIES, variantsForBoundComponent } from '../../src/ui-libraries';
import type { UiComponentVariant, UiEditableSlot } from '../../src/ui-library';
import { officialRuntimePresentation } from '../library-runtime/registry';
import { WEB_DESIGN_THEME_PRESETS, type WebDesignThemePreset } from '../../src/design-themes';
import { componentDefaults } from '../../src/templates';
import {
  matchViewportPreset,
  viewportDimensions,
  viewportPresetsForDevice,
  type WebDesignViewportOrientation
} from '../../src/viewport-presets';
import {
  pagesForDocument,
  tokensForDocument,
  type WebComponentStyle,
  type WebComponentConstraints,
  type WebComponentVisualState,
  type WebComponentType,
  type WebDesignAsset,
  type WebDesignComponent,
  type WebDesignDevice,
  type WebDesignDocument,
  type WebDesignJsonValue,
  type WebDesignLibraryName,
  type WebDesignProject,
  type WebHorizontalConstraint,
  type WebDesignSymbol,
  type WebDesignTokens,
  type WebSymbolOverride
} from '../../src/schema';
import {
  createRepository,
  type DesignRepository,
  type DesignSummary,
  type GenerationPlanSummary,
  type GenerationStepReview,
  type SceneAnnotationAiContext
} from './repository';
import { LibraryCanvasComponent } from './LibraryCanvasComponent';
import { componentEffectStyleToCss, componentStyleToCss, mergeComponentStyles } from './component-style';
import { CanvasComponent as WorkspaceCanvasComponent, CanvasComponentContent as WorkspaceCanvasComponentContent } from './CanvasComponent';
import { libraryPreviewSelection, type LibraryPreviewPointerEvent, type LibraryPreviewSelection } from '../library-runtime/element-selection';
import { WorkspaceBottomToolbar, WorkspaceNavigationBar, WorkspacePanelResizeHandle } from './WorkspaceShellChrome';
import { buildPrototypeFlowConnections, prototypeFlowPath, scenePrototypeFlowSources } from './prototype-flow-model';
import { DEFAULT_WORKSPACE_SHELL, parseWorkspaceShellState, workspaceShellGridStyle, workspaceShellReducer, workspaceShellShortcut, type WorkspaceArea, type WorkspaceTool } from './workspace-shell-model';
import {
  fitWorkspaceRect,
  fitWorkspaceWidth,
  panWorkspaceCamera,
  unionWorkspaceRects,
  workspaceArtboardRenderTier,
  workspaceViewportReady,
  workspaceZoomFromWheel,
  zoomWorkspaceCameraAt,
  type WorkspaceCamera
} from '../../src/v2/workspace-camera';
import type { WorkspaceArtboardPlacement, WorkspacePlacementDocument, WorkspaceSurfaceKind } from '../../src/v2/workspace-placement-store';
import { indexSceneDocument, isSceneContainer, isSceneSlotContainer, type SceneDocument, type SceneNode, type ScenePrototypeLink, type SceneResponsiveNodeOverride, type SceneVariableCollection } from '../../src/v2/scene-schema';
import type { SceneEditorCommand } from '../../src/v2/scene-editor-command';
import type { SceneHistoryStatus } from '../../src/v2/scene-store';
import { inspectorCapabilities as resolveInspectorCapabilities } from './inspector-model';
import { SelectionOverlay, type SelectionOverlayItem } from './SelectionOverlay';
import { SceneArtboardCanvas, sceneArtboardContentHeight, sceneArtboardSelectionBounds } from './SceneArtboardCanvas';
import { createSceneBasicShape, createSceneLibraryInstance } from './scene-node-factory';
import { editableSlotsForSceneLibraryNode, resolveSceneInsertionTarget, type SceneInsertionFocus, type SceneInsertionTarget } from './scene-insertion-target';
import { createSceneSnippet, instantiateSceneSnippet, parseSceneSnippets, type SceneSnippet } from './scene-snippet-library';
import {
  deepestSelectionChild,
  normalizedSelectionRect,
  selectionCandidatesAtPoint,
  selectionNodesInRect,
  type EditorSelectableNode,
  type EditorSelectionCandidate,
  type EditorSelectionRect
} from './selection-model';

type BasicShapeId = 'rectangle' | 'ellipse' | 'line';

const SCENE_RESPONSIVE_EDITOR_RULES = {
  tablet: { ruleId: 'editor:tablet', ruleName: '人工平板布局', minWidth: 768, maxWidth: 1200 },
  mobile: { ruleId: 'editor:mobile', ruleName: '人工手机布局', maxWidth: 768 }
} as const;

const SLOT_EDITOR_HEADER_HEIGHT = 72;
const SLOT_EDITOR_CANVAS_INSETS = { top: 27, right: 9, bottom: 9, left: 9 } as const;

function slotEditorFrameBounds(canvasSize: { width: number; height: number }) {
  return {
    x: 0,
    y: 0,
    width: canvasSize.width + SLOT_EDITOR_CANVAS_INSETS.left + SLOT_EDITOR_CANVAS_INSETS.right,
    height: SLOT_EDITOR_HEADER_HEIGHT + SLOT_EDITOR_CANVAS_INSETS.top + canvasSize.height + SLOT_EDITOR_CANVAS_INSETS.bottom
  };
}

const palette: Array<{ id: BasicShapeId; type: WebComponentType; label: string; icon: string; keywords: string[] }> = [
  { id: 'rectangle', type: 'section', label: '矩形', icon: '▭', keywords: ['rectangle', '矩形', '容器'] },
  { id: 'ellipse', type: 'section', label: '圆形', icon: '○', keywords: ['ellipse', 'circle', '圆形'] },
  { id: 'line', type: 'divider', label: '直线', icon: '—', keywords: ['line', 'divider', '直线'] }
];

const fillPresets = [
  '#FFFFFF', '#F5F5F7', '#1D1D1F', '#007AFF', '#7C3AED', '#EC4899',
  'linear-gradient(135deg,#007AFF,#5AC8FA)',
  'linear-gradient(135deg,#7C3AED,#EC4899)',
  'radial-gradient(circle at 30% 20%,rgba(255,255,255,.75),transparent 28%),linear-gradient(145deg,#111827,#312E81)'
];

const shadowPresets = [
  '',
  '0 8px 24px rgba(15,23,42,.10)',
  '0 18px 48px rgba(15,23,42,.18)',
  '0 28px 80px rgba(15,23,42,.28)',
  '0 0 60px rgba(99,102,241,.45)',
  'inset 0 0 0 1px rgba(255,255,255,.18)'
];

function basicShapeDefaults(shapeId: BasicShapeId, x: number, y: number): WebDesignComponent {
  if (shapeId === 'line') {
    const line = componentDefaults('divider', x, y);
    line.id = `shape-line-${crypto.randomUUID().slice(0, 8)}`;
    line.name = '直线';
    line.width = 320;
    line.style = { borderColor: '#8E8E93', borderWidth: 1 };
    return line;
  }
  const shape = componentDefaults('section', x, y);
  shape.id = `shape-${shapeId}-${crypto.randomUUID().slice(0, 8)}`;
  shape.name = shapeId === 'ellipse' ? '圆形' : '矩形';
  shape.width = shapeId === 'ellipse' ? 160 : 260;
  shape.height = shapeId === 'ellipse' ? 160 : 160;
  shape.style = { background: '#EAF3FF', borderColor: '#A8CCFF', borderWidth: 1, borderRadius: shapeId === 'ellipse' ? 999 : 16 };
  return shape;
}

function contentContainerAncestor(document: WebDesignDocument, component: WebDesignComponent): WebDesignComponent | undefined {
  const byId = new Map(document.components.map((candidate) => [candidate.id, candidate]));
  let parent = component.parentId ? byId.get(component.parentId) : undefined;
  while (parent) {
    if (isUiContentContainer(parent)) return parent;
    parent = parent.parentId ? byId.get(parent.parentId) : undefined;
  }
  return undefined;
}

function overlayContentContainerAncestor(document: WebDesignDocument, component: WebDesignComponent): WebDesignComponent | undefined {
  const byId = new Map(document.components.map((candidate) => [candidate.id, candidate]));
  let parent = component.parentId ? byId.get(component.parentId) : undefined;
  while (parent) {
    if (isOverlayUiContentContainer(parent)) return parent;
    parent = parent.parentId ? byId.get(parent.parentId) : undefined;
  }
  return undefined;
}

function growCanvasForDevice(document: WebDesignDocument, pageId: string, device: WebDesignDevice, minimumHeight?: number): WebDesignDocument {
  const fitted = growUiContentContainersToFit(document, pageId, device);
  const excludedComponentIds = new Set(componentsForPage(fitted, pageId)
    .filter((component) => overlayContentContainerAncestor(fitted, component))
    .map((component) => component.id));
  return growPageToFitContent(fitted, pageId, device, { excludedComponentIds, minimumHeight });
}

function growAllCanvases(document: WebDesignDocument): WebDesignDocument {
  return pagesForDocument(document).reduce((current, page) =>
    (['desktop', 'tablet', 'mobile'] as const).reduce((next, device) => growCanvasForDevice(next, page.id, device), current), document);
}

function createSlotStarterComponents(
  container: WebDesignComponent,
  slot: UiEditableSlot,
  template: 'form' | 'details',
  pageId: string,
  device: WebDesignDevice
): WebDesignComponent[] {
  const containerFrame = resolveComponent(container, device);
  const availableWidth = Math.max(220, slot.width - 24);
  const libraryName = container.library?.name ?? 'antd';
  const componentNames = libraryName === 'chakra'
    ? { title: 'Heading', input: 'Input', select: 'NativeSelect', textarea: 'Textarea', details: 'Table', divider: 'Separator', button: 'Button' }
    : libraryName === 'shadcn'
      ? { title: 'Typography', input: 'Input', select: 'Select', textarea: 'Textarea', details: 'DataTable', divider: 'Separator', button: 'Button' }
      : { title: 'Typography', input: 'Input', select: 'Select', textarea: 'Input', details: 'Descriptions', divider: 'Divider', button: 'Button' };
  const variants = libraryName === 'chakra'
    ? { title: 'default', input: 'outline', select: 'outline', textarea: 'default', details: 'default', divider: 'default', button: 'solid' }
    : libraryName === 'shadcn'
      ? { title: 'default', input: 'default', select: 'default', textarea: 'default', details: 'default', divider: 'default', button: 'default' }
      : { title: 'title', input: 'outlined', select: 'outlined', textarea: 'textarea', details: 'bordered', divider: 'plain', button: 'primary' };
  const rows = template === 'form'
    ? [
      { definition: componentNames.title, variant: variants.title, name: '表单标题', content: '完善信息', x: 12, y: 12, width: availableWidth, height: 44 },
      { definition: componentNames.input, variant: variants.input, name: '姓名输入', content: '请输入姓名', x: 12, y: 76, width: availableWidth, height: 40 },
      { definition: componentNames.select, variant: variants.select, name: '类型选择', content: '请选择类型', x: 12, y: 132, width: availableWidth, height: 40 },
      { definition: componentNames.textarea, variant: variants.textarea, name: '详细说明', content: '请输入详细说明', x: 12, y: 188, width: availableWidth, height: 96 },
      { definition: componentNames.button, variant: variants.button, name: '提交按钮', content: '保存修改', x: 12, y: 304, width: 120, height: 40 }
    ]
    : [
      { definition: componentNames.title, variant: variants.title, name: '详情标题', content: '产品详情', x: 12, y: 12, width: availableWidth, height: 44 },
      { definition: componentNames.details, variant: variants.details, name: '详情信息', content: '', x: 12, y: 72, width: availableWidth, height: 170 },
      { definition: componentNames.divider, variant: variants.divider, name: '内容分隔线', content: '', x: 12, y: 258, width: availableWidth, height: 24 },
      { definition: componentNames.button, variant: variants.button, name: '确认按钮', content: '确认', x: 12, y: 300, width: 100, height: 40 }
    ];
  return rows.map((row, index) => {
    let child = createComponentFromUiLibrary(libraryName, row.definition, container.x + row.x, container.y + row.y);
    child = applyUiLibraryVariant(child, row.variant);
    child.name = row.name;
    child.content = row.content;
    child.pageId = pageId;
    child.parentId = container.id;
    child.slot = slot.id;
    child.zIndex = index + 1;
    child.width = row.width;
    child.height = row.height;
    if (device !== 'desktop') child = updateComponentFrame(child, device, {
      x: containerFrame.x + row.x,
      y: containerFrame.y + row.y,
      width: row.width,
      height: row.height
    });
    return child;
  });
}

function materializeExistingSlotContent(
  container: WebDesignComponent,
  slot: UiEditableSlot,
  pageId: string,
  device: WebDesignDevice
): WebDesignComponent[] {
  if (slot.id !== 'content' || container.library?.component !== 'Card' || !container.content.trim()) return [];
  const libraryName = container.library.name;
  const textDefinition = libraryName === 'chakra' ? 'Text' : 'Typography';
  const textVariant = libraryName === 'antd' ? 'paragraph' : libraryName === 'chakra' ? 'body' : 'default';
  const containerFrame = resolveComponent(container, device);
  let content = createComponentFromUiLibrary(libraryName, textDefinition, containerFrame.x + 12, containerFrame.y + 12);
  content = applyUiLibraryVariant(content, textVariant);
  content.name = '卡片正文';
  content.content = container.content;
  content.pageId = pageId;
  content.parentId = container.id;
  content.slot = slot.id;
  content.zIndex = 1;
  content.width = Math.max(120, slot.width - 24);
  content.height = Math.max(56, Math.min(96, slot.height - 24));
  if (device !== 'desktop') content = updateComponentFrame(content, device, {
    x: containerFrame.x + 12,
    y: containerFrame.y + 12,
    width: content.width,
    height: content.height
  });
  return [content];
}

function visibleCssColor(value: string): boolean {
  if (!value || value === 'transparent') return false;
  const alpha = value.match(/rgba?\([^)]*[,/]\s*([\d.]+)\s*\)$/)?.[1];
  return alpha === undefined || Number(alpha) > 0;
}

function cssPixels(value: string): number {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function svgDataUrl(element: Element, color: string): string {
  const clone = element.cloneNode(true) as SVGElement;
  clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  if (color) clone.setAttribute('color', color);
  for (const node of [clone, ...clone.querySelectorAll('*')]) {
    for (const attribute of ['fill', 'stroke']) {
      if (node.getAttribute(attribute) === 'currentColor') node.setAttribute(attribute, color);
    }
  }
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(new XMLSerializer().serializeToString(clone))}`;
}

async function materializeOfficialDemoContent(
  container: WebDesignComponent,
  slot: UiEditableSlot,
  pageId: string,
  device: WebDesignDevice
): Promise<WebDesignComponent[]> {
  if (slot.id !== 'content' || !container.library?.props.registryDemo || container.library.props.editorDetachedContent === true) return [];
  let frame: HTMLIFrameElement | null = null;
  for (let attempt = 0; attempt < 25; attempt += 1) {
    const host = document.querySelector<HTMLElement>(`[data-component-id="${CSS.escape(container.id)}"]`);
    frame = host?.querySelector('iframe') ?? null;
    if (frame?.contentDocument?.getElementById('root')?.childElementCount) break;
    await new Promise((resolve) => window.setTimeout(resolve, 80));
  }
  const runtimeDocument = frame?.contentDocument;
  const runtimeWindow = frame?.contentWindow;
  const runtimeRoot = runtimeDocument?.getElementById('root');
  if (!runtimeDocument || !runtimeWindow || !runtimeRoot || !frame) return [];
  const viewportWidth = runtimeDocument.documentElement.clientWidth || frame.clientWidth;
  const viewportHeight = runtimeDocument.documentElement.clientHeight || frame.clientHeight;
  const contentWidth = Math.max(viewportWidth, runtimeDocument.documentElement.scrollWidth, runtimeDocument.body.scrollWidth, runtimeRoot.scrollWidth);
  const contentHeight = Math.max(viewportHeight, runtimeDocument.documentElement.scrollHeight, runtimeDocument.body.scrollHeight, runtimeRoot.scrollHeight);
  if (viewportWidth <= 0 || viewportHeight <= 0) return [];
  const containerFrame = resolveComponent(container, device);
  const scaleX = containerFrame.width / viewportWidth;
  const scaleY = scaleX;
  const candidates: Array<{
    component: WebDesignComponent;
    element: Element;
    priority: number;
    order: number;
    area: number;
    canParent: boolean;
  }> = [];
  const elements = [...runtimeRoot.querySelectorAll<HTMLElement>('*')];

  elements.forEach((element, order) => {
    const computed = runtimeWindow.getComputedStyle(element);
    if (computed.display === 'none' || computed.visibility === 'hidden' || Number(computed.opacity) === 0) return;
    const bounds = element.getBoundingClientRect();
    const left = Math.max(0, bounds.left);
    const top = Math.max(0, bounds.top);
    const right = Math.min(contentWidth, bounds.right);
    const bottom = Math.min(contentHeight, bounds.bottom);
    if (right - left < 3 || bottom - top < 3) return;
    const tag = element.tagName.toLowerCase();
    const interactiveAncestor = element.closest('button,a,[role="button"]');
    const background = computed.backgroundImage !== 'none'
      ? `${computed.backgroundImage}${visibleCssColor(computed.backgroundColor) ? `, ${computed.backgroundColor}` : ''}`
      : visibleCssColor(computed.backgroundColor) ? computed.backgroundColor : undefined;
    const borderWidth = Math.max(cssPixels(computed.borderTopWidth), cssPixels(computed.borderRightWidth), cssPixels(computed.borderBottomWidth), cssPixels(computed.borderLeftWidth));
    const borderVisible = borderWidth > 0 && computed.borderTopStyle !== 'none' && visibleCssColor(computed.borderTopColor);
    const shadow = computed.boxShadow !== 'none' ? computed.boxShadow : undefined;
    const isInteractive = tag === 'button' || tag === 'a' || element.getAttribute('role') === 'button';
    const isImage = tag === 'img' || tag === 'svg';
    if (interactiveAncestor && interactiveAncestor !== element && !isImage) return;
    const isInput = ['input', 'textarea', 'select'].includes(tag);
    const directText = [...element.childNodes]
      .filter((node) => node.nodeType === Node.TEXT_NODE)
      .map((node) => node.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim();
    const isText = !isInteractive && !isImage && !isInput
      && (/^(h[1-6]|p|label|small|strong|em|blockquote|figcaption|span)$/.test(tag)
        || tag === 'div' && element.childElementCount === 0)
      && directText.length > 0;
    const isSurface = !isInteractive && !isImage && !isInput && !isText
      && (background !== undefined || borderVisible || shadow !== undefined);
    if (!isInteractive && !isImage && !isInput && !isText && !isSurface) return;
    if (isSurface && bounds.width >= contentWidth * .98 && bounds.height >= contentHeight * .98 && !borderVisible && !shadow) return;

    const type: WebComponentType = isImage ? 'image'
      : isInput ? tag === 'textarea' ? 'textarea' : tag === 'select' ? 'select' : 'input'
        : isInteractive ? tag === 'a' ? 'link' : 'button'
          : isText ? /^h[1-6]$/.test(tag) ? 'heading' : 'text'
            : 'section';
    const content = isImage
      ? tag === 'svg'
        ? svgDataUrl(element, computed.color)
        : (element as HTMLImageElement).currentSrc || (element as HTMLImageElement).src
      : isInput
        ? (element as HTMLInputElement).value || element.getAttribute('placeholder') || ''
        : isInteractive ? element.innerText.trim() : isText ? directText : '';
    if (isText && !content) return;
    const fontSize = cssPixels(computed.fontSize);
    const lineHeightPixels = cssPixels(computed.lineHeight);
    const fontWeight = Number.parseInt(computed.fontWeight, 10);
    const textAlign = ['left', 'center', 'right'].includes(computed.textAlign) ? computed.textAlign as WebComponentStyle['textAlign'] : undefined;
    const component = componentDefaults(type, containerFrame.x + left * scaleX, containerFrame.y + top * scaleY);
    component.id = `detached-${type}-${crypto.randomUUID().slice(0, 8)}`;
    component.name = isImage ? (element.getAttribute('alt') || '图片')
      : isInteractive ? (content || '操作')
        : isText ? content.slice(0, 28)
          : '容器背景';
    component.pageId = pageId;
    component.parentId = container.id;
    component.slot = slot.id;
    component.width = Math.max(3, (right - left) * scaleX);
    component.height = Math.max(3, (bottom - top) * scaleY);
    component.content = content;
    component.style = {
      background,
      color: (isText || isInteractive || isInput) && visibleCssColor(computed.color) ? computed.color : undefined,
      borderColor: borderVisible ? computed.borderTopColor : undefined,
      borderWidth: borderVisible ? borderWidth : undefined,
      borderStyle: borderVisible ? 'solid' : undefined,
      borderRadius: Math.max(cssPixels(computed.borderTopLeftRadius), cssPixels(computed.borderTopRightRadius), cssPixels(computed.borderBottomRightRadius), cssPixels(computed.borderBottomLeftRadius)) * Math.min(scaleX, scaleY),
      padding: 0,
      fontSize: fontSize > 0 ? fontSize * Math.min(scaleX, scaleY) : undefined,
      fontWeight: Number.isFinite(fontWeight) ? fontWeight : computed.fontWeight === 'bold' ? 700 : undefined,
      textAlign,
      lineHeight: fontSize > 0 && lineHeightPixels > 0 ? lineHeightPixels / fontSize : undefined,
      letterSpacing: computed.letterSpacing === 'normal' ? undefined : cssPixels(computed.letterSpacing) * scaleX,
      opacity: Number(computed.opacity) < 1 ? Number(computed.opacity) : undefined,
      shadow,
      overflow: computed.overflow === 'hidden' ? 'hidden' : undefined,
      objectFit: isImage && ['cover', 'contain', 'fill', 'none', 'scale-down'].includes(computed.objectFit) ? computed.objectFit as WebComponentStyle['objectFit'] : undefined,
      objectPosition: isImage ? computed.objectPosition : undefined,
      customCss: computed.fontFamily ? { fontFamily: computed.fontFamily } : undefined
    };
    if (tag === 'a' && (element as HTMLAnchorElement).href) component.interaction = { type: 'url', target: (element as HTMLAnchorElement).href };
    const area = component.width * component.height;
    candidates.push({
      component,
      element,
      priority: isSurface ? 0 : isImage ? 1 : 2,
      order,
      area,
      canParent: isSurface || isInteractive
    });
  });

  const seen = new Set<string>();
  const materialized = candidates
    .sort((left, right) => left.priority - right.priority || (left.priority === 0 ? right.area - left.area : left.order - right.order))
    .filter(({ component }) => {
      const key = [component.type, Math.round(component.x), Math.round(component.y), Math.round(component.width), Math.round(component.height), component.content].join('|');
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  const byElement = new Map(materialized.map((candidate) => [candidate.element, candidate]));
  return materialized.map(({ component, element }, index) => {
      let ancestor = element.parentElement;
      while (ancestor && ancestor !== runtimeRoot) {
        const parent = byElement.get(ancestor);
        if (parent?.canParent) {
          component.parentId = parent.component.id;
          component.slot = undefined;
          break;
        }
        ancestor = ancestor.parentElement;
      }
      component.zIndex = index + 1;
      if (device !== 'desktop') return updateComponentFrame(component, device, { x: component.x, y: component.y, width: component.width, height: component.height });
      return component;
    });
}

const deviceOptions: Array<{ device: WebDesignDevice; label: string; icon: string }> = [
  { device: 'desktop', label: '桌面', icon: '▰' },
  { device: 'tablet', label: '平板', icon: '▯' },
  { device: 'mobile', label: '手机', icon: '▯' }
];

const horizontalConstraintOptions: Array<{ id: WebHorizontalConstraint; label: string; description: string }> = [
  { id: 'auto', label: '智能响应', description: '根据组件大小和位置自动选择缩放或锚定方式' },
  { id: 'left', label: '左侧固定', description: '保持宽度和左边距' },
  { id: 'center', label: '水平居中', description: '保持宽度并相对容器居中' },
  { id: 'right', label: '右侧固定', description: '保持宽度和右边距' },
  { id: 'stretch', label: '左右拉伸', description: '保持左右边距，宽度跟随容器' },
  { id: 'scale', label: '等比缩放', description: '位置和宽度随容器同比例变化' }
];

type ViewportSelection = {
  presetId?: string;
  orientation: WebDesignViewportOrientation;
  customHeight: number;
};

const STUDIO_PROJECT_QUERY = 'studio-project';
const STUDIO_DESIGN_QUERY = 'studio-design';

function studioLocationSelection() {
  const params = new URLSearchParams(window.location.search);
  return {
    projectId: params.get(STUDIO_PROJECT_QUERY) || undefined,
    documentId: params.get(STUDIO_DESIGN_QUERY) || undefined
  };
}

function replaceStudioLocation(projectId?: string, documentId?: string) {
  const url = new URL(window.location.href);
  if (projectId) url.searchParams.set(STUDIO_PROJECT_QUERY, projectId);
  else url.searchParams.delete(STUDIO_PROJECT_QUERY);
  if (projectId && documentId) url.searchParams.set(STUDIO_DESIGN_QUERY, documentId);
  else url.searchParams.delete(STUDIO_DESIGN_QUERY);
  window.history.replaceState(null, '', url);
}

const DEFAULT_VIEWPORT_SELECTIONS: Record<WebDesignDevice, ViewportSelection> = {
  desktop: { presetId: 'desktop-responsive', orientation: 'default', customHeight: 900 },
  tablet: { presetId: 'tablet-responsive', orientation: 'default', customHeight: 1024 },
  mobile: { presetId: 'mobile-responsive', orientation: 'default', customHeight: 844 }
};

function viewportSelectionsForDocument(document: WebDesignDocument): Record<WebDesignDevice, ViewportSelection> {
  return Object.fromEntries((['desktop', 'tablet', 'mobile'] as const).map((device) => {
    const breakpoint = breakpointFor(document, device);
    if (breakpoint.preview) {
      const persistedPreset = viewportPresetsForDevice(device).find((preset) => preset.id === breakpoint.preview?.presetId);
      return [device, {
        presetId: persistedPreset?.id,
        orientation: breakpoint.preview.orientation,
        customHeight: breakpoint.preview.viewportHeight
      }];
    }
    const match = matchViewportPreset(device, breakpoint.width);
    if (!match) return [device, { ...DEFAULT_VIEWPORT_SELECTIONS[device], presetId: undefined }];
    return [device, {
      presetId: match.preset.id,
      orientation: match.orientation,
      customHeight: viewportDimensions(match.preset, match.orientation).height
    }];
  })) as Record<WebDesignDevice, ViewportSelection>;
}

function editableDocumentPayload(document: WebDesignDocument): string {
  const { revision: _revision, createdAt: _createdAt, updatedAt: _updatedAt, ...editable } = document;
  return JSON.stringify(editable);
}

type Interaction = {
  kind: 'move' | 'resize';
  componentId: string;
  pointerX: number;
  pointerY: number;
  frame: ResolvedWebDesignComponent;
  selectedIds: string[];
  snapshot: WebDesignDocument;
  scale: number;
  scoped: boolean;
};

type CanvasPan = {
  pointerX: number;
  pointerY: number;
  camera: WorkspaceCamera;
};

type CanvasMarquee = {
  pointerId: number;
  startClientX: number;
  startClientY: number;
  startPoint: { x: number; y: number };
  canvas: HTMLElement;
  nodes: EditorSelectableNode[];
  initialIds: string[];
  initialPrimaryId?: string;
  additive: boolean;
  moved: boolean;
};

type WorkspaceArtboardDrag = {
  artboardId: string;
  pointerX: number;
  pointerY: number;
  x: number;
  y: number;
};

type LayerAction = 'front' | 'forward' | 'backward' | 'back';
type AlignAction = 'left' | 'center' | 'right' | 'top' | 'middle' | 'bottom';
type LibraryTab = 'components' | WebDesignLibraryName | 'my' | 'layers';
type VariantPickerTarget = { library: WebDesignLibraryName; componentId: string; replaceComponentId?: string };
type VariantPickerPointerDrag = Omit<LibraryPreviewPointerEvent, 'phase'> & {
  libraryName: WebDesignLibraryName;
  definitionId: string;
  variantId: string;
  startClientX: number;
  startClientY: number;
  dragging: boolean;
};
type EditingSlot = { componentId: string; slotId: string };
type SceneContentFocus = SceneInsertionFocus & { pageId: string };
type SelectionCandidatePopover = {
  clientX: number;
  clientY: number;
  candidates: EditorSelectionCandidate[];
};
type InspectorVisualState = 'default' | WebComponentVisualState;
type InspectorTab = 'design' | 'prototype' | 'ai' | 'review';

const PERSONAL_SYMBOLS_STORAGE_KEY = 'web-design-studio:personal-symbols:v1';
const SCENE_SNIPPETS_STORAGE_KEY = 'web-design-studio:scene-snippets:v2';

const WORKSPACE_ARTBOARD_GAP = 160;
const WORKSPACE_ARTBOARD_HEADER_HEIGHT = 48;

const WORKSPACE_SURFACE_LABELS: Record<WorkspaceSurfaceKind, string> = {
  page: '页面',
  modal: '弹窗',
  drawer: '抽屉',
  popover: '浮层',
  menu: '菜单',
  state: '界面状态'
};

const WORKSPACE_SURFACE_SIZES: Record<Exclude<WorkspaceSurfaceKind, 'page' | 'state'>, { width: number; height: number }> = {
  modal: { width: 720, height: 720 },
  drawer: { width: 520, height: 900 },
  popover: { width: 420, height: 360 },
  menu: { width: 320, height: 440 }
};

function deviceForWorkspaceArtboard(document: WebDesignDocument, artboard: WorkspaceArtboardPlacement): WebDesignDevice {
  if (artboard.surfaceKind !== 'page') return 'desktop';
  return (['desktop', 'tablet', 'mobile'] as const).reduce((closest, candidate) => (
    Math.abs(breakpointFor(document, candidate).width - artboard.viewportWidth)
      < Math.abs(breakpointFor(document, closest).width - artboard.viewportWidth) ? candidate : closest
  ), 'desktop' as WebDesignDevice);
}

function workspaceViewportHeight(document: WebDesignDocument, device: WebDesignDevice): number {
  const responsive = breakpointFor(document, device);
  if (responsive.preview?.viewportHeight) return responsive.preview.viewportHeight;
  const preset = matchViewportPreset(device, responsive.width);
  return preset
    ? viewportDimensions(preset.preset, preset.orientation).height
    : Math.min(responsive.height, device === 'desktop' ? 1080 : device === 'tablet' ? 1024 : 844);
}

function workspacePageSources(document: WebDesignDocument, scene?: SceneDocument) {
  return scene
    ? scene.pages.map((page) => ({ id: page.id, surfaceKind: 'page' as WorkspaceSurfaceKind }))
    : pagesForDocument(document).map((page) => ({ id: page.id, surfaceKind: page.surfaceKind ?? 'page' }));
}

function initialWorkspaceArtboards(document: WebDesignDocument, scene?: SceneDocument): WorkspaceArtboardPlacement[] {
  let x = 0;
  return workspacePageSources(document, scene).map((page) => {
    const responsive = breakpointFor(document, 'desktop');
    const artboard: WorkspaceArtboardPlacement = {
      artboardId: `artboard-${page.id}`,
      pageId: page.id,
      surfaceKind: page.surfaceKind,
      viewportWidth: responsive.width,
      viewportHeight: workspaceViewportHeight(document, 'desktop'),
      x,
      y: 0
    };
    x += responsive.width + WORKSPACE_ARTBOARD_GAP;
    return artboard;
  });
}

function reconcileWorkspaceArtboards(document: WebDesignDocument, stored: readonly WorkspaceArtboardPlacement[], scene?: SceneDocument): WorkspaceArtboardPlacement[] {
  const pages = workspacePageSources(document, scene);
  const pageIds = new Set(pages.map((page) => page.id));
  const seenPages = new Set<string>();
  const valid = stored.filter((artboard) => {
    if (!pageIds.has(artboard.pageId) || seenPages.has(artboard.pageId)) return false;
    seenPages.add(artboard.pageId);
    return true;
  }).map((artboard) => ({ ...artboard }));
  let right = valid.length === 0
    ? 0
    : Math.max(...valid.map((artboard) => artboard.x + artboard.viewportWidth)) + WORKSPACE_ARTBOARD_GAP;
  const desktop = breakpointFor(document, 'desktop');
  for (const page of pages) {
    if (seenPages.has(page.id)) continue;
    valid.push({
      artboardId: `artboard-${page.id}`,
      pageId: page.id,
      surfaceKind: page.surfaceKind,
      viewportWidth: desktop.width,
      viewportHeight: workspaceViewportHeight(document, 'desktop'),
      x: right,
      y: 0
    });
    right += desktop.width + WORKSPACE_ARTBOARD_GAP;
  }
  return valid;
}

function workspaceArtboardBounds(document: WebDesignDocument, artboards: readonly WorkspaceArtboardPlacement[], scene?: SceneDocument) {
  return unionWorkspaceRects(artboards.map((artboard) => workspaceArtboardContentBounds(document, artboard, scene)))
    ?? { x: 0, y: 0, width: 1, height: 1 };
}

function workspaceArtboardContentBounds(document: WebDesignDocument, artboard: WorkspaceArtboardPlacement, scene?: SceneDocument) {
  const targetDevice = deviceForWorkspaceArtboard(document, artboard);
  const contentHeight = scene
    ? sceneArtboardContentHeight(scene, artboard.pageId, artboard.viewportWidth, artboard.viewportHeight)
    : componentsForPage(document, artboard.pageId).reduce((maximum, component) => {
      const frame = resolveComponent(component, targetDevice);
      return frame.hidden ? maximum : Math.max(maximum, frame.y + frame.height + 80);
    }, artboard.viewportHeight);
  return {
    x: artboard.x,
    y: artboard.y - WORKSPACE_ARTBOARD_HEADER_HEIGHT,
    width: artboard.viewportWidth,
    height: contentHeight + WORKSPACE_ARTBOARD_HEADER_HEIGHT
  };
}

function workspaceArtboardSignature(artboards: readonly WorkspaceArtboardPlacement[]): string {
  return JSON.stringify(artboards);
}

function loadPersonalSymbols(): WebDesignSymbol[] {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(PERSONAL_SYMBOLS_STORAGE_KEY) ?? '[]') as unknown;
    return Array.isArray(parsed) ? parsed.filter((item): item is WebDesignSymbol => Boolean(item && typeof item === 'object' && 'id' in item && 'components' in item)) : [];
  } catch {
    return [];
  }
}

const VARIANT_PROP_LABELS: Record<string, Record<string, string> | string> = {
  size: { small: '小尺寸', middle: '中尺寸', large: '大尺寸', default: '标准尺寸' },
  type: { primary: '主色', default: '默认', dashed: '虚线', text: '文本', link: '链接', inner: '内嵌' },
  variant: { outlined: '描边', filled: '填充', borderless: '无边框', underlined: '下划线', solid: '实心', ghost: '幽灵', link: '链接', text: '文本' },
  bordered: { true: '有边框', false: '无边框' },
  hoverable: { true: '悬浮反馈', false: '静态' },
  disabled: { true: '禁用状态', false: '可用状态' },
  danger: { true: '危险操作', false: '普通操作' },
  block: { true: '撑满容器', false: '内容宽度' },
  loading: { true: '加载状态', false: '完成状态' },
  multiple: { true: '多选', false: '单选' },
  showSearch: { true: '可搜索', false: '基础选择' },
  allowClear: { true: '可清除', false: '不可清除' },
  direction: { vertical: '纵向', horizontal: '横向' },
  orientation: { vertical: '纵向', horizontal: '横向' },
  shape: { circle: '圆形', round: '圆角', square: '方形', default: '标准外形' },
  showcase: {
    basic: '基础内容', compact: '高密度', borderless: '融合背景', hoverable: '悬浮交互', cover: '图片封面',
    actions: '底部操作', meta: '头像信息', grid: '宫格数据', inner: '嵌套层级', loading: '骨架加载'
  }
};

const INTERNAL_LIBRARY_PROPS = new Set(['componentSlug', 'registryDemo', 'registryElement', 'editorDetachedContent']);

function inspectableLibraryProps(props: Record<string, WebDesignJsonValue>) {
  return Object.entries(props).filter(([key]) => !INTERNAL_LIBRARY_PROPS.has(key));
}

function bindLibraryPreviewElement(component: WebDesignComponent, libraryDisplayName: string, selection: LibraryPreviewSelection): WebDesignComponent {
  if (!component.library) return component;
  return {
    ...component,
    name: `${libraryDisplayName} · ${selection.label}`,
    content: selection.label,
    width: Math.max(6, Math.round(selection.width)),
    height: Math.max(6, Math.round(selection.height)),
    library: { ...component.library, props: { ...component.library.props, registryElement: { ...selection } } }
  };
}

function variantDifferenceLabels(variant: UiComponentVariant): string[] {
  const labels: string[] = [];
  if (variant.width) labels.push(`${variant.width}×${variant.height ?? '自适应'}`);
  for (const [key, value] of Object.entries(variant.props)) {
    const configured = VARIANT_PROP_LABELS[key];
    if (typeof configured === 'string') labels.push(`${configured} ${String(value)}`);
    else if (configured) labels.push(configured[String(value)] ?? `${key}=${String(value)}`);
    else if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') labels.push(`${key}=${String(value)}`);
  }
  return [...new Set(labels)].slice(0, 4);
}

const INTERACTIVE_COMPONENT_PREVIEWS = new Set([
  'ActionBar', 'AlertDialog', 'ColorPicker', 'Combobox', 'ContextMenu', 'DatePicker', 'Dialog', 'Drawer', 'Dropdown', 'DropdownMenu', 'Editable', 'FloatingPanel',
  'HoverCard', 'Menu', 'Modal', 'OverlayManager', 'Popover', 'Portal', 'Select', 'Sheet', 'Toast', 'ToggleTip', 'Tooltip', 'Tour'
]);

const OPEN_OVERLAY_PREVIEWS = new Set([
  'ActionBar', 'AlertDialog', 'AutoComplete', 'Cascader', 'Combobox', 'ContextMenu', 'DatePicker', 'Dialog', 'Drawer', 'Dropdown', 'DropdownMenu', 'FloatingPanel',
  'HoverCard', 'Menu', 'Modal', 'OverlayManager', 'Popconfirm', 'Popover', 'Portal', 'Select', 'Sheet', 'Toast', 'ToggleTip', 'Tooltip', 'Tour', 'TreeSelect'
]);

const WIDE_VARIANT_PREVIEWS = new Set(['OverlayManager']);

function variantIsInteractive(variant: UiComponentVariant, componentId: string): boolean {
  return INTERACTIVE_COMPONENT_PREVIEWS.has(componentId)
    || variant.props.motion === true
    || ['hoverable', 'showSearch', 'multiple', 'allowClear', 'draggable', 'collapsible', 'editable', 'autoplay'].some((key) => variant.props[key] === true);
}

function LazyVariantPreview({ children, minHeight }: { children: ReactNode; minHeight: number }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const host = hostRef.current;
    if (!host || typeof IntersectionObserver === 'undefined') {
      setVisible(true);
      return;
    }
    const observer = new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) return;
      setVisible(true);
      observer.disconnect();
    }, { rootMargin: '600px 0px' });
    observer.observe(host);
    return () => observer.disconnect();
  }, []);
  return <div ref={hostRef} className="lazy-variant-preview" style={{ minHeight }}>
    {visible ? children : <span>滚动到此处时载入官方示例</span>}
  </div>;
}

function SelectableVariantCard({ component, previewHeight, className, interactive, variantLabel, differences, tokens, onPickItem, onPickPointerEvent }: {
  component: WebDesignComponent;
  previewHeight: number;
  className: string;
  interactive: boolean;
  variantLabel: string;
  differences: string[];
  tokens?: WebDesignTokens;
  onPickItem: (selection: LibraryPreviewSelection) => void;
  onPickPointerEvent: (event: LibraryPreviewPointerEvent) => void;
}) {
  const [contentHeight, setContentHeight] = useState<number>();
  const surfaceHeight = Math.max(previewHeight, contentHeight ?? 0);
  const cardHeight = surfaceHeight + 58;
  return <article className={`variant-preview-card ${interactive ? 'interactive-variant' : ''}`} style={{ minHeight: cardHeight, height: cardHeight }}>
    <div data-library-portal-host className={className} style={{ minHeight: surfaceHeight, height: surfaceHeight }}>
      <LazyVariantPreview minHeight={surfaceHeight}>
        <LibraryCanvasComponent component={component} preview showcase tokens={tokens} pickItems onPickItem={onPickItem} onPickPointerEvent={onPickPointerEvent} onContentHeight={setContentHeight} />
      </LazyVariantPreview>
    </div>
    <footer><div className="variant-preview-description"><strong>{variantLabel}</strong><span>{differences.map((difference) => <small key={difference}>{difference}</small>)}</span></div><span className="variant-item-pick-help">点击预览选择 · 拖到画布放置</span></footer>
  </article>;
}

export function WebDesignStudioApp() {
  const [repository, setRepository] = useState<DesignRepository>();
  const [documents, setDocuments] = useState<DesignSummary[]>([]);
  const [activeProject, setActiveProject] = useState<WebDesignProject>();
  const [document, setDocument] = useState<WebDesignDocument>();
  const [sceneDocument, setSceneDocument] = useState<SceneDocument>();
  const [sceneHistory, setSceneHistory] = useState<SceneHistoryStatus>();
  const [sceneLoadState, setSceneLoadState] = useState<'idle' | 'loading' | 'ready' | 'missing'>('idle');
  const [sceneReloadToken, setSceneReloadToken] = useState(0);
  const [ready, setReady] = useState(false);
  const [screen, setScreen] = useState<'project' | 'editor'>('project');
  const [persistedRevision, setPersistedRevision] = useState(0);
  const [selectedId, setSelectedId] = useState<string>();
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [selectionCandidatePopover, setSelectionCandidatePopover] = useState<SelectionCandidatePopover>();
  const [marqueeRect, setMarqueeRect] = useState<EditorSelectionRect>();
  const [pageId, setPageId] = useState('home');
  const [clipboard, setClipboard] = useState<{ document: WebDesignDocument; componentIds: string[] }>();
  const [sceneClipboard, setSceneClipboard] = useState<SceneNode[]>([]);
  const [snapGuides, setSnapGuides] = useState<SnapGuides>({});
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [preview, setPreview] = useState(false);
  const [previewOverlayPageId, setPreviewOverlayPageId] = useState<string>();
  const [interactionMode, setInteractionMode] = useState(false);
  const [device, setDevice] = useState<WebDesignDevice>('desktop');
  const [viewportSelections, setViewportSelections] = useState<Record<WebDesignDevice, ViewportSelection>>(() => structuredClone(DEFAULT_VIEWPORT_SELECTIONS));
  const [workspaceCamera, setWorkspaceCamera] = useState<WorkspaceCamera>({ x: 0, y: 0, zoom: 0.82 });
  const [workspacePlacement, setWorkspacePlacement] = useState<WorkspacePlacementDocument>();
  const [activeArtboardId, setActiveArtboardId] = useState<string>();
  const [newSurfaceKind, setNewSurfaceKind] = useState<WorkspaceSurfaceKind>('page');
  const [prototypeLinksVisible, setPrototypeLinksVisible] = useState(true);
  const [past, setPast] = useState<WebDesignDocument[]>([]);
  const [future, setFuture] = useState<WebDesignDocument[]>([]);
  const [toast, setToast] = useState<string>();
  const [annotationText, setAnnotationText] = useState('');
  const [aiInstruction, setAiInstruction] = useState('');
  const [sceneAiContext, setSceneAiContext] = useState<SceneAnnotationAiContext>();
  const [sceneAnnotationPreparingId, setSceneAnnotationPreparingId] = useState<string>();
  const [generationPlan, setGenerationPlan] = useState<GenerationPlanSummary>();
  const [generationReview, setGenerationReview] = useState<GenerationStepReview>();
  const [generationLoading, setGenerationLoading] = useState(false);
  const [generationAction, setGenerationAction] = useState<string>();
  const [generationRejectionReason, setGenerationRejectionReason] = useState('');
  const [paletteQuery, setPaletteQuery] = useState('');
  const [libraryTab, setLibraryTab] = useState<LibraryTab>('antd');
  const [personalSymbols, setPersonalSymbols] = useState<WebDesignSymbol[]>(loadPersonalSymbols);
  const [sceneSnippets, setSceneSnippets] = useState<SceneSnippet[]>(() => parseSceneSnippets(window.localStorage.getItem(SCENE_SNIPPETS_STORAGE_KEY)));
  const [sceneVariablesDraft, setSceneVariablesDraft] = useState('[]');
  const [variantPickerTarget, setVariantPickerTarget] = useState<VariantPickerTarget>();
  const [sceneContentFocus, setSceneContentFocus] = useState<SceneContentFocus>();
  const [variantPickerDrag, setVariantPickerDrag] = useState<VariantPickerPointerDrag>();
  const [themePickerOpen, setThemePickerOpen] = useState(false);
  const [projectLibraryOpen, setProjectLibraryOpen] = useState(false);
  const [newDesignOpen, setNewDesignOpen] = useState(false);
  const [newDesignName, setNewDesignName] = useState('');
  const [editingSlot, setEditingSlot] = useState<EditingSlot>();
  const [inspectorVisualState, setInspectorVisualState] = useState<InspectorVisualState>('default');
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>('design');
  const [workspaceShell, dispatchWorkspaceShell] = useReducer(
    workspaceShellReducer,
    DEFAULT_WORKSPACE_SHELL,
    () => parseWorkspaceShellState(window.localStorage.getItem('web-design-studio.workspace-shell.v1'))
  );
  const interaction = useRef<Interaction | undefined>(undefined);
  const canvasPan = useRef<CanvasPan | undefined>(undefined);
  const canvasMarquee = useRef<CanvasMarquee | undefined>(undefined);
  const workspaceArtboardDrag = useRef<WorkspaceArtboardDrag | undefined>(undefined);
  const spacePressed = useRef(false);
  const workspaceCameraContext = useRef<string | undefined>(undefined);
  const workspaceCameraBeforeSlot = useRef<WorkspaceCamera | undefined>(undefined);
  const slotCameraContext = useRef<string | undefined>(undefined);
  const persistedWorkspaceArtboards = useRef<string>('');
  const variantPickerDragRef = useRef<VariantPickerPointerDrag | undefined>(undefined);
  const documentRef = useRef<WebDesignDocument | undefined>(undefined);
  const sceneDocumentRef = useRef<SceneDocument | undefined>(undefined);
  const assetInput = useRef<HTMLInputElement | null>(null);
  const canvasStage = useRef<HTMLElement | null>(null);
  const canvasScroll = useRef<HTMLDivElement | null>(null);
  const zoom = workspaceCamera.zoom;
  const previewZoom = useRef(zoom);
  const interactionZoom = useRef(zoom);
  const [canvasPanning, setCanvasPanning] = useState(false);
  const [canvasPanReady, setCanvasPanReady] = useState(false);

  useEffect(() => { documentRef.current = document; }, [document]);
  useEffect(() => { sceneDocumentRef.current = sceneDocument; }, [sceneDocument]);
  useEffect(() => { setSceneAiContext(undefined); }, [sceneDocument?.documentId, selectedId]);

  useEffect(() => {
    const stage = canvasStage.current;
    const viewport = canvasScroll.current;
    if (!stage || !viewport || screen !== 'editor') return;
    const onWheel = (event: WheelEvent) => {
      if (preview) return;
      const isPinchZoom = event.ctrlKey || event.metaKey;
      const target = event.target;
      const isInsideCanvasStage = target instanceof Node && stage.contains(target);
      if (isPinchZoom) {
        event.preventDefault();
        event.stopPropagation();
      }
      if (!isInsideCanvasStage) return;
      if (!isPinchZoom && !(target instanceof Node && viewport.contains(target))) return;
      event.preventDefault();
      event.stopPropagation();
      if (isPinchZoom) {
        const bounds = viewport.getBoundingClientRect();
        const anchor = {
          x: Math.min(viewport.clientWidth, Math.max(0, event.clientX - bounds.left)),
          y: Math.min(viewport.clientHeight, Math.max(0, event.clientY - bounds.top))
        };
        setWorkspaceCamera((current) => zoomWorkspaceCameraAt(
          current,
          workspaceZoomFromWheel(current.zoom, event.deltaY),
          anchor
        ));
        return;
      }
      setWorkspaceCamera((current) => panWorkspaceCamera(current, { x: -event.deltaX, y: -event.deltaY }));
    };
    window.addEventListener('wheel', onWheel, { capture: true, passive: false });
    return () => window.removeEventListener('wheel', onWheel, { capture: true });
  }, [screen, preview]);

  const selected = useMemo(
    () => document?.components.find((component) => component.id === selectedId),
    [document, selectedId]
  );
  const selectedSceneEntry = useMemo(() => {
    if (!sceneDocument || !selectedId) return undefined;
    return indexSceneDocument(sceneDocument).get(selectedId);
  }, [sceneDocument, selectedId]);
  const selectedSceneNode: SceneNode | undefined = selectedSceneEntry?.node;
  useEffect(() => {
    if (!sceneContentFocus || !sceneDocument) return;
    const entry = indexSceneDocument(sceneDocument).get(sceneContentFocus.nodeId);
    if (!entry || entry.pageId !== sceneContentFocus.pageId) setSceneContentFocus(undefined);
  }, [sceneContentFocus, sceneDocument]);
  const sceneResponsiveRuleSpec = device === 'desktop' ? undefined : SCENE_RESPONSIVE_EDITOR_RULES[device];
  const selectedSceneResponsiveOverride = useMemo(() => {
    if (!sceneDocument || !selectedSceneNode || !sceneResponsiveRuleSpec) return undefined;
    return sceneDocument.responsiveRules.find((rule) => rule.id === sceneResponsiveRuleSpec.ruleId)
      ?.nodeOverrides.find((override) => override.nodeId === selectedSceneNode.id);
  }, [sceneDocument, sceneResponsiveRuleSpec, selectedSceneNode]);
  const selectedScenePositionEditable = useMemo(() => {
    if (!sceneDocument || !selectedSceneEntry) return false;
    if (sceneDocument.pages.some((page) => page.id === selectedSceneEntry.parentId)) return true;
    const parent = indexSceneDocument(sceneDocument).get(selectedSceneEntry.parentId)?.node;
    return Boolean(parent && (parent.layout.mode === 'free' || selectedSceneEntry.node.layout.position === 'absolute'));
  }, [sceneDocument, selectedSceneEntry]);
  const activeScenePage = useMemo(
    () => sceneDocument?.pages.find((page) => page.id === pageId),
    [sceneDocument, pageId]
  );
  const sceneEditingActive = sceneLoadState === 'loading' || sceneLoadState === 'missing' || Boolean(sceneDocument);
  const selectedFrame = useMemo(
    () => selected ? resolveComponent(selected, device) : undefined,
    [selected, device]
  );
  const selectedIdSet = useMemo(() => new Set(selectedIds), [selectedIds]);
  const breakpoint = useMemo(
    () => document ? breakpointFor(document, device) : { width: 1200, height: 940 },
    [document, device]
  );
  const viewportPresets = useMemo(() => viewportPresetsForDevice(device), [device]);
  const viewportSelection = viewportSelections[device];
  const viewportPreset = viewportPresets.find((preset) => preset.id === viewportSelection.presetId);
  const previewViewportHeight = viewportPreset
    ? viewportDimensions(viewportPreset, viewportSelection.orientation).height
    : viewportSelection.customHeight;
  const renderedCanvasHeight = sceneDocument
    ? sceneArtboardContentHeight(sceneDocument, pageId, breakpoint.width, Math.max(breakpoint.height, previewViewportHeight))
    : Math.max(breakpoint.height, previewViewportHeight);
  const scaledCanvasWidth = breakpoint.width * zoom;
  const scaledCanvasHeight = renderedCanvasHeight * zoom;
  const pages = useMemo(() => {
    if (sceneDocument) {
      const artboardsByPage = new Map(workspacePlacement?.artboards.map((artboard) => [artboard.pageId, artboard]));
      return sceneDocument.pages.map((page) => ({
        id: page.id,
        name: page.name,
        slug: `/${page.id}`,
        surfaceKind: artboardsByPage.get(page.id)?.surfaceKind ?? 'page' as WorkspaceSurfaceKind
      }));
    }
    return document ? pagesForDocument(document) : [];
  }, [document, sceneDocument, workspacePlacement?.artboards]);
  const activeWorkspaceArtboard = useMemo(
    () => workspacePlacement?.artboards.find((artboard) => artboard.artboardId === activeArtboardId),
    [activeArtboardId, workspacePlacement?.artboards]
  );
  const routePages = useMemo(() => pages.filter((page) => (page.surfaceKind ?? 'page') === 'page'), [pages]);
  const previewOverlayPage = useMemo(() => pages.find((page) => page.id === previewOverlayPageId), [pages, previewOverlayPageId]);
  const previewOverlayArtboard = useMemo(() => workspacePlacement?.artboards.find((artboard) => artboard.pageId === previewOverlayPageId), [workspacePlacement?.artboards, previewOverlayPageId]);
  const prototypeConnections = useMemo(() => {
    if (!document || !workspacePlacement) return [];
    if (!sceneDocument) return [];
    return buildPrototypeFlowConnections(
      scenePrototypeFlowSources(sceneDocument, workspacePlacement.artboards),
      workspacePlacement.artboards
    );
  }, [device, document, sceneDocument, workspacePlacement]);
  const selectedPrototypeTarget = useMemo(() => {
    const targetPageId = selectedSceneNode?.prototypeLink?.targetPageId
      ?? (selected?.interaction?.type === 'page' ? selected.interaction.target : undefined);
    return targetPageId ? pages.find((page) => page.id === targetPageId) : undefined;
  }, [pages, selected, selectedSceneNode]);
  const activeProjectDocuments = useMemo(() => {
    const ids = new Set(activeProject?.designIds ?? []);
    return documents.filter((item) => ids.has(item.documentId));
  }, [activeProject?.designIds, documents]);
  const tokens = useMemo(() => document ? tokensForDocument(document) : undefined, [document]);
  const currentPage = useMemo(() => pages.find((page) => page.id === pageId) ?? pages[0], [pages, pageId]);
  const pageComponents = useMemo(() => document && currentPage ? componentsForPage(document, currentPage.id) : [], [document, currentPage]);
  const editingContainer = useMemo(() => editingSlot ? document?.components.find((component) => component.id === editingSlot.componentId) : undefined, [document, editingSlot]);
  const editingSlotDefinition = useMemo(() => editingContainer && editingSlot
    ? editableSlotsForUiComponent(editingContainer).find((slot) => slot.id === editingSlot.slotId)
    : undefined, [editingContainer, editingSlot]);
  const editingSlotComponents = useMemo(() => document && editingSlot
    ? componentsInSlot(document, editingSlot.componentId, editingSlot.slotId)
    : [], [document, editingSlot]);
  const editingVisibleComponents = useMemo(() => document && editingSlot
    ? visibleComponentsInSlot(document, editingSlot.componentId, editingSlot.slotId)
    : [], [document, editingSlot]);
  const editingSlotCanvasSize = useMemo(() => {
    if (!editingContainer || !editingSlotDefinition) return undefined;
    const containerFrame = resolveComponent(editingContainer, device);
    return fitContentCanvasToComponents(editingVisibleComponents, device, {
      minimumWidth: editingSlotDefinition.width,
      minimumHeight: editingSlotDefinition.height,
      originX: containerFrame.x,
      originY: containerFrame.y
    });
  }, [editingContainer, editingSlotDefinition, editingVisibleComponents, device]);

  useEffect(() => {
    if (!editingSlot || !editingSlotCanvasSize || screen !== 'editor' || preview) return;
    const context = `${editingSlot.componentId}:${editingSlot.slotId}:${device}`;
    if (slotCameraContext.current === context) return;
    const frame = window.requestAnimationFrame(() => {
      const viewport = canvasScroll.current;
      if (!viewport) return;
      slotCameraContext.current = context;
      setWorkspaceCamera(fitWorkspaceRect(
        slotEditorFrameBounds(editingSlotCanvasSize),
        { width: viewport.clientWidth, height: viewport.clientHeight },
        { top: 112, right: 72, bottom: 88, left: 72 },
        2.5
      ));
    });
    return () => window.cancelAnimationFrame(frame);
  }, [screen, preview, editingSlot?.componentId, editingSlot?.slotId, device, editingSlotCanvasSize?.width, editingSlotCanvasSize?.height]);
  const inspectedFrame = useMemo(() => {
    if (!selectedFrame || !editingContainer || !editingSlot || !selected || slotIdForDescendant(document!, selected, editingContainer.id) !== editingSlot.slotId) return selectedFrame;
    const containerFrame = resolveComponent(editingContainer, device);
    return { ...selectedFrame, x: selectedFrame.x - containerFrame.x, y: selectedFrame.y - containerFrame.y };
  }, [selectedFrame, editingContainer, editingSlot, selected, document, device]);
  const inspectedStyle = useMemo(() => inspectorVisualState === 'default'
    ? inspectedFrame?.style
    : mergeComponentStyles(inspectedFrame?.style ?? {}, selected?.states?.[inspectorVisualState]), [inspectorVisualState, inspectedFrame?.style, selected?.states]);

  useEffect(() => setInspectorVisualState('default'), [selectedId]);

  useEffect(() => {
    window.localStorage.setItem(PERSONAL_SYMBOLS_STORAGE_KEY, JSON.stringify(personalSymbols));
  }, [personalSymbols]);

  useEffect(() => {
    window.localStorage.setItem(SCENE_SNIPPETS_STORAGE_KEY, JSON.stringify(sceneSnippets));
  }, [sceneSnippets]);

  useEffect(() => {
    setSceneVariablesDraft(JSON.stringify(sceneDocument?.variableCollections ?? [], null, 2));
  }, [sceneDocument?.documentId, sceneDocument?.revision]);

  useEffect(() => {
    window.localStorage.setItem('web-design-studio.workspace-shell.v1', JSON.stringify(workspaceShell));
  }, [workspaceShell]);

  useEffect(() => {
    if (!document?.symbols?.length) return;
    setPersonalSymbols((current) => {
      const byId = new Map(current.map((symbol) => [symbol.id, symbol]));
      let changed = false;
      for (const symbol of document.symbols ?? []) {
        if (!byId.has(symbol.id)) {
          byId.set(symbol.id, structuredClone(symbol));
          changed = true;
        }
      }
      return changed ? [...byId.values()] : current;
    });
  }, [document?.documentId]);

  useEffect(() => {
    void (async () => {
      const repo = await createRepository();
      setRepository(repo);
      const [items, projectItems, runtimeContext] = await Promise.all([repo.list(), repo.listProjects(), repo.runtimeContext()]);
      setDocuments(items);
      const requested = studioLocationSelection();
      const requestedProjectId = runtimeContext.defaultProjectId ?? projectItems[0]?.projectId;
      if (requestedProjectId) {
        const project = await repo.readProject(requestedProjectId);
        setActiveProject(project);
        if (requested.documentId && project.designIds.includes(requested.documentId)) {
          openDocument(await repo.read(requested.documentId));
          setScreen('editor');
          replaceStudioLocation(project.projectId, requested.documentId);
        } else {
          setScreen('project');
          replaceStudioLocation(project.projectId);
        }
      }
      setReady(true);
    })().catch((error) => {
      setReady(true);
      showToast(error instanceof Error ? error.message : String(error));
    });
  }, []);

  useEffect(() => {
    if (!repository || !document || screen !== 'editor') {
      sceneDocumentRef.current = undefined;
      setSceneDocument(undefined);
      setSceneHistory(undefined);
      setSceneLoadState('idle');
      return;
    }
    let cancelled = false;
    setSceneLoadState('loading');
    setSceneDocument(undefined);
    setSceneHistory(undefined);
    void repository.readScene(document.documentId).then(async (nextScene) => {
      const history = await repository.readSceneHistory(document.documentId).catch(() => ({ undoCount: 0, redoCount: 0 }));
      if (cancelled) return;
      sceneDocumentRef.current = nextScene;
      setSceneDocument(nextScene);
      setSceneHistory(history);
      setSceneLoadState('ready');
    }).catch(() => {
      if (cancelled) return;
      sceneDocumentRef.current = undefined;
      setSceneDocument(undefined);
      setSceneHistory(undefined);
      setSceneLoadState('missing');
    });
    return () => { cancelled = true; };
  }, [repository, document?.documentId, screen, sceneReloadToken]);

  useEffect(() => {
    if (!repository || !document || screen !== 'editor' || repository.mode !== 'server') {
      setGenerationPlan(undefined);
      setGenerationReview(undefined);
      return;
    }
    let cancelled = false;
    const refresh = async (showLoading: boolean) => {
      if (showLoading) setGenerationLoading(true);
      try {
        const plan = await repository.readGenerationPlan(document.documentId);
        if (cancelled) return;
        setGenerationPlan(plan);
        const activeStep = plan?.activeStep;
        if (activeStep?.stepId && activeStep.activeAttemptId) {
          const review = await repository.inspectGenerationStep(document.documentId, activeStep.stepId, activeStep.activeAttemptId);
          if (!cancelled) setGenerationReview(review);
        } else if (!cancelled) {
          setGenerationReview(undefined);
        }
      } catch (error) {
        if (!cancelled && showLoading) showToast(error instanceof Error ? error.message : String(error));
      } finally {
        if (!cancelled && showLoading) setGenerationLoading(false);
      }
    };
    void refresh(true);
    const timer = window.setInterval(() => void refresh(false), 3000);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [repository, document?.documentId, screen]);

  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      const activeArtboard = workspaceArtboardDrag.current;
      if (activeArtboard) {
        const dx = (event.clientX - activeArtboard.pointerX) / workspaceCamera.zoom;
        const dy = (event.clientY - activeArtboard.pointerY) / workspaceCamera.zoom;
        setWorkspacePlacement((current) => current ? {
          ...current,
          artboards: current.artboards.map((artboard) => artboard.artboardId === activeArtboard.artboardId
            ? { ...artboard, x: Math.round(activeArtboard.x + dx), y: Math.round(activeArtboard.y + dy) }
            : artboard)
        } : current);
        return;
      }
      const activePan = canvasPan.current;
      if (activePan) {
        setWorkspaceCamera(panWorkspaceCamera(activePan.camera, {
          x: event.clientX - activePan.pointerX,
          y: event.clientY - activePan.pointerY
        }));
        return;
      }
      const activeMarquee = canvasMarquee.current;
      if (activeMarquee && event.pointerId === activeMarquee.pointerId) {
        const distance = Math.hypot(event.clientX - activeMarquee.startClientX, event.clientY - activeMarquee.startClientY);
        if (!activeMarquee.moved && distance < 4) return;
        activeMarquee.moved = true;
        const bounds = activeMarquee.canvas.getBoundingClientRect();
        const scaleX = bounds.width / Math.max(1, activeMarquee.canvas.offsetWidth);
        const scaleY = bounds.height / Math.max(1, activeMarquee.canvas.offsetHeight);
        const point = {
          x: (event.clientX - bounds.left) / Math.max(scaleX, .0001),
          y: (event.clientY - bounds.top) / Math.max(scaleY, .0001)
        };
        const rect = normalizedSelectionRect(activeMarquee.startPoint, point);
        const matchedIds = selectionNodesInRect(activeMarquee.nodes, rect).map((node) => node.id);
        const nextIds = activeMarquee.additive
          ? [...activeMarquee.initialIds, ...matchedIds.filter((id) => !activeMarquee.initialIds.includes(id))]
          : matchedIds;
        setMarqueeRect(rect);
        setSelectedIds(nextIds);
        setSelectedId(matchedIds.at(-1) ?? activeMarquee.initialPrimaryId);
        return;
      }
      const active = interaction.current;
      if (!active) return;
      const dx = (event.clientX - active.pointerX) / active.scale;
      const dy = (event.clientY - active.pointerY) / active.scale;
      if (active.kind === 'move') {
        const movingIds = selectedRootIds(active.snapshot, active.selectedIds).flatMap((id) => [id, ...descendantIds(active.snapshot, id)]);
        const candidate = { ...active.frame, x: active.frame.x + dx, y: active.frame.y + dy };
        const snapped = active.scoped
          ? { frame: candidate, guides: {} as SnapGuides }
          : snapComponentFrame(active.snapshot, active.componentId, device, candidate, movingIds);
        setSnapGuides(snapped.guides);
        changeLiveWithCanvasGrowth(() => {
          const moved = moveComponentsWithDescendants(
            active.snapshot,
            active.selectedIds,
            device,
            snapped.frame.x - active.frame.x,
            snapped.frame.y - active.frame.y
          );
          const moving = new Set(movingIds);
          return { ...moved, components: moved.components.map((component) => moving.has(component.id) ? setSymbolOverride(component, 'frame', true) : component) };
        });
      } else {
        changeLiveWithCanvasGrowth(() => ({
          ...active.snapshot,
          components: active.snapshot.components.map((component) => component.id === active.componentId
            ? setSymbolOverride(updateComponentFrame(component, device, constrainComponentFrame(component, device, {
              width: Math.max(24, active.frame.width + dx),
              height: Math.max(24, active.frame.height + dy)
            })), 'frame', true)
            : component)
        }));
      }
    };
    const onUp = (event: PointerEvent) => {
      if (workspaceArtboardDrag.current) workspaceArtboardDrag.current = undefined;
      if (canvasPan.current) {
        canvasPan.current = undefined;
        setCanvasPanning(false);
      }
      const activeMarquee = canvasMarquee.current;
      if (activeMarquee && event.pointerId === activeMarquee.pointerId) {
        if (!activeMarquee.moved && !activeMarquee.additive) {
          setSelectedId(undefined);
          setSelectedIds([]);
        }
        canvasMarquee.current = undefined;
        setMarqueeRect(undefined);
        return;
      }
      const active = interaction.current;
      if (!active) return;
      setPast((items) => [...items.slice(-59), active.snapshot]);
      setFuture([]);
      interaction.current = undefined;
      setSnapGuides({});
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onUp);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onUp);
    };
  }, [device, zoom, pageId, editingSlot, workspaceCamera.zoom]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches('input, textarea, select, [contenteditable="true"]')) return;
      if (event.code === 'Space' && !preview) {
        event.preventDefault();
        spacePressed.current = true;
        setCanvasPanReady(true);
        return;
      }
      const command = event.metaKey || event.ctrlKey;
      const shellAction = workspaceShellShortcut(event.key, command);
      if (event.key === 'Escape' && selectionCandidatePopover) {
        event.preventDefault();
        setSelectionCandidatePopover(undefined);
      } else if (event.key === 'Escape' && preview) {
        event.preventDefault();
        toggleFullPreview();
      } else if (event.key === 'Escape' && interactionMode) {
        event.preventDefault();
        toggleInteractionMode();
      } else if (command && event.key.toLowerCase() === 's') {
        event.preventDefault();
        void save();
      } else if (command && !event.shiftKey && event.key.toLowerCase() === 'z') {
        event.preventDefault();
        undo();
      } else if (command && event.shiftKey && event.key.toLowerCase() === 'z') {
        event.preventDefault();
        redo();
      } else if (command && event.shiftKey && event.key.toLowerCase() === 'g') {
        event.preventDefault();
        ungroupSelected();
      } else if (command && event.key.toLowerCase() === 'g') {
        event.preventDefault();
        groupSelected();
      } else if (command && event.key.toLowerCase() === 'd') {
        event.preventDefault();
        duplicateSelected();
      } else if (command && event.key.toLowerCase() === 'c' && selectedIds.length > 0) {
        event.preventDefault();
        copySelected();
      } else if (command && event.key.toLowerCase() === 'v' && (sceneEditingActive ? sceneClipboard.length > 0 : Boolean(clipboard))) {
        event.preventDefault();
        pasteClipboard();
      } else if (event.key === 'Enter' && selectedId && !event.shiftKey) {
        event.preventDefault();
        selectSelectionChild();
      } else if (event.key === 'Enter' && selectedId && event.shiftKey) {
        event.preventDefault();
        selectSelectionParent();
      } else if (shellAction) {
        event.preventDefault();
        if (shellAction.type === 'select-tool') activateWorkspaceTool(shellAction.tool);
        else dispatchWorkspaceShell(shellAction);
      } else if ((event.key === 'Backspace' || event.key === 'Delete') && selectedIds.length > 0) {
        event.preventDefault();
        if (sceneEditingActive) void deleteSceneSelection();
        else deleteSelected();
      } else if (selectedIds.length > 0 && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) {
        event.preventDefault();
        const amount = event.shiftKey ? 10 : 1;
        const dx = event.key === 'ArrowLeft' ? -amount : event.key === 'ArrowRight' ? amount : 0;
        const dy = event.key === 'ArrowUp' ? -amount : event.key === 'ArrowDown' ? amount : 0;
        if (sceneEditingActive) void nudgeSceneSelection(dx, dy);
        else nudgeSelected(dx, dy);
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.code !== 'Space') return;
      spacePressed.current = false;
      setCanvasPanReady(false);
    };
    const onBlur = () => {
      spacePressed.current = false;
      canvasPan.current = undefined;
      setCanvasPanReady(false);
      setCanvasPanning(false);
    };
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', onBlur);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
    };
  }, [selectedId, selectedIds, selectionCandidatePopover, past, future, dirty, saving, repository, persistedRevision, device, clipboard, sceneClipboard, sceneEditingActive, sceneDocument, pageId, preview, interactionMode, zoom, breakpoint.width, editingSlot]);

  useEffect(() => {
    if (screen !== 'editor' || preview || !document || !repository) {
      workspaceCameraContext.current = undefined;
      setWorkspacePlacement(undefined);
      return;
    }
    if (sceneLoadState === 'idle' || sceneLoadState === 'loading') return;
    if (editingSlot) return;
    const context = document.documentId;
    if (workspaceCameraContext.current === context) return;
    let cancelled = false;
    void repository.readWorkspace(document.documentId).then(async (storedPlacement) => {
      if (cancelled) return;
      const activeScene = sceneLoadState === 'ready' ? sceneDocument : undefined;
      const reconciledArtboards = storedPlacement.artboards.length === 0
        ? initialWorkspaceArtboards(document, activeScene)
        : reconcileWorkspaceArtboards(document, storedPlacement.artboards, activeScene);
      const placementChanged = workspaceArtboardSignature(reconciledArtboards) !== workspaceArtboardSignature(storedPlacement.artboards);
      const placement = placementChanged
        ? await repository.saveWorkspaceArtboards(document.documentId, reconciledArtboards)
        : storedPlacement;
      if (cancelled) return;
      const viewport = canvasScroll.current;
      const camera = storedPlacement.artboards.length === 0 && viewport
        ? fitWorkspaceRect(
            workspaceArtboardBounds(document, placement.artboards, sceneDocumentRef.current),
            { width: viewport.clientWidth, height: viewport.clientHeight },
            { top: 92, right: 64, bottom: 92, left: 64 }
          )
        : placement.camera;
      workspaceCameraContext.current = context;
      persistedWorkspaceArtboards.current = workspaceArtboardSignature(placement.artboards);
      setWorkspacePlacement(placement);
      const first = placement.artboards[0];
      setActiveArtboardId(first?.artboardId);
      if (first) {
        setDevice(deviceForWorkspaceArtboard(document, first));
        setPageId(first.pageId);
      }
      setWorkspaceCamera(camera);
    }).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
    return () => { cancelled = true; };
  }, [screen, preview, editingSlot, document?.documentId, repository, sceneDocument, sceneLoadState]);

  useEffect(() => {
    if (!document || screen !== 'editor' || preview || editingSlot) return;
    if (!repository) return;
    const context = document.documentId;
    if (workspaceCameraContext.current !== context) return;
    const timeout = window.setTimeout(() => {
      void repository.saveWorkspaceCamera(document.documentId, workspaceCamera)
        .catch((error) => showToast(error instanceof Error ? error.message : String(error)));
    }, 120);
    return () => window.clearTimeout(timeout);
  }, [document?.documentId, repository, screen, preview, editingSlot, workspaceCamera]);

  useEffect(() => {
    if (!document || !repository || !workspacePlacement || screen !== 'editor' || preview || editingSlot) return;
    if (workspaceCameraContext.current !== document.documentId) return;
    const signature = workspaceArtboardSignature(workspacePlacement.artboards);
    if (signature === persistedWorkspaceArtboards.current) return;
    const timeout = window.setTimeout(() => {
      void repository.saveWorkspaceArtboards(document.documentId, workspacePlacement.artboards).then((saved) => {
        persistedWorkspaceArtboards.current = workspaceArtboardSignature(saved.artboards);
        setWorkspacePlacement((current) => current ? { ...current, revision: saved.revision, updatedAt: saved.updatedAt } : current);
      }).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
    }, 120);
    return () => window.clearTimeout(timeout);
  }, [document?.documentId, repository, workspacePlacement?.artboards, screen, preview, editingSlot]);

  function showToast(message: string) {
    setToast(message);
    window.setTimeout(() => setToast((current) => current === message ? undefined : current), 2600);
  }

  function chooseLibraryTab(tab: LibraryTab) {
    setLibraryTab(tab);
    const area: WorkspaceArea = tab === 'layers' ? 'layers' : tab === 'components' ? 'tools' : tab === 'my' ? 'my' : 'assets';
    dispatchWorkspaceShell({ type: 'select-area', area });
  }

  function activateWorkspaceArea(area: WorkspaceArea) {
    dispatchWorkspaceShell({ type: 'select-area', area });
    if (area === 'layers' || area === 'variables' || area === 'ai') setLibraryTab('layers');
    else if (area === 'tools') setLibraryTab('components');
    else if (area === 'my') setLibraryTab('my');
    else if (libraryTab === 'layers' || libraryTab === 'components' || libraryTab === 'my') setLibraryTab('antd');
  }

  function activateWorkspaceTool(tool: WorkspaceTool) {
    dispatchWorkspaceShell({ type: 'select-tool', tool });
    if (tool === 'ai') {
      activateWorkspaceArea('ai');
      return;
    }
    if (tool === 'insert') {
      activateWorkspaceArea('tools');
      showToast('从左侧选择元素，再拖到画布');
      return;
    }
    if (tool === 'comment') {
      setInspectorTab('ai');
      if (!workspaceShell.rightPanelOpen) dispatchWorkspaceShell({ type: 'toggle-right-panel' });
      showToast(selected ? '可以在右侧添加批注或让 AI 修改' : '批注工具已开启，请点击画布中的组件');
    }
  }

  function setCurrent(next: WebDesignDocument) {
    documentRef.current = next;
    setDocument(next);
  }

  function openDocument(next: WebDesignDocument) {
    const opened = structuredClone(next);
    workspaceCameraBeforeSlot.current = undefined;
    slotCameraContext.current = undefined;
    workspaceCameraContext.current = undefined;
    persistedWorkspaceArtboards.current = '';
    setWorkspacePlacement(undefined);
    setActiveArtboardId(undefined);
    sceneDocumentRef.current = undefined;
    setSceneDocument(undefined);
    setSceneHistory(undefined);
    setSceneLoadState('loading');
    setSceneReloadToken((value) => value + 1);
    setCurrent(opened);
    setPersistedRevision(next.revision);
    setSelectedId(undefined);
    setSelectedIds([]);
    setDirty(false);
    setPast([]);
    setFuture([]);
    setDevice('desktop');
    setViewportSelections(viewportSelectionsForDocument(opened));
    setPageId(pagesForDocument(next)[0].id);
    setEditingSlot(undefined);
  }

  function commit(updater: (current: WebDesignDocument) => WebDesignDocument) {
    const current = documentRef.current;
    if (!current) return;
    const next = updater(current);
    setPast((items) => [...items.slice(-59), structuredClone(current)]);
    setFuture([]);
    setCurrent(next);
    setDirty(true);
  }

  function commitWithCanvasGrowth(
    updater: (current: WebDesignDocument) => WebDesignDocument,
    targetDevices: readonly WebDesignDevice[] = [device],
    targetPageId = pageId
  ) {
    commit((current) => targetDevices.reduce(
      (next, targetDevice) => growCanvasForDevice(next, targetPageId, targetDevice),
      updater(current)
    ));
  }

  function changeLive(updater: (current: WebDesignDocument) => WebDesignDocument) {
    const current = documentRef.current;
    if (!current) return;
    setCurrent(updater(current));
    setDirty(true);
  }

  function changeLiveWithCanvasGrowth(
    updater: (current: WebDesignDocument) => WebDesignDocument,
    targetPageId = pageId,
    targetDevice = device
  ) {
    changeLive((current) => growCanvasForDevice(
      updater(current),
      targetPageId,
      targetDevice,
      breakpointFor(current, targetDevice).height
    ));
  }

  function historyDocument(snapshot: WebDesignDocument, current: WebDesignDocument): WebDesignDocument {
    return { ...structuredClone(snapshot), revision: current.revision, createdAt: current.createdAt, updatedAt: current.updatedAt };
  }

  function applySceneDocument(next: SceneDocument) {
    sceneDocumentRef.current = next;
    setSceneDocument(next);
    setSceneLoadState('ready');
  }

  async function refreshSceneHistory(documentId: string) {
    if (!repository) return;
    setSceneHistory(await repository.readSceneHistory(documentId));
  }

  async function refreshGenerationState(showLoading = false) {
    const currentDocument = documentRef.current;
    if (!repository || !currentDocument || repository.mode !== 'server') return;
    if (showLoading) setGenerationLoading(true);
    try {
      const plan = await repository.readGenerationPlan(currentDocument.documentId);
      setGenerationPlan(plan);
      const activeStep = plan?.activeStep;
      if (activeStep?.stepId && activeStep.activeAttemptId) {
        setGenerationReview(await repository.inspectGenerationStep(currentDocument.documentId, activeStep.stepId, activeStep.activeAttemptId));
      } else {
        setGenerationReview(undefined);
      }
    } finally {
      if (showLoading) setGenerationLoading(false);
    }
  }

  async function runGenerationReviewAction(action: 'accept' | 'reject' | 'rollback' | 'pause' | 'resume') {
    const currentDocument = documentRef.current;
    const plan = generationPlan;
    if (!repository || !currentDocument || !plan || generationAction) return;
    const review = generationReview;
    setGenerationAction(action);
    try {
      if (action === 'accept') {
        if (!review?.candidate) throw new Error('当前步骤还没有可接受的设计候选。');
        let result = await repository.acceptGenerationStep(currentDocument.documentId, plan.revision, review.step.stepId, review.candidate.attemptId);
        if (result.status === 'requires-protection-review') {
          const approved = window.confirm('这个候选会修改你人工调整过的字段。是否明确允许本次覆盖？');
          if (!approved) return;
          result = await repository.acceptGenerationStep(currentDocument.documentId, result.plan.revision, review.step.stepId, review.candidate.attemptId, true);
        }
        showToast(result.status === 'committed' ? '已接受这一小步，AI 可以继续下一步' : `候选状态：${result.status}`);
        setSceneReloadToken((value) => value + 1);
      } else if (action === 'reject') {
        if (!review?.candidate) throw new Error('当前步骤还没有可退回的设计候选。');
        const reason = generationRejectionReason.trim();
        if (!reason) throw new Error('请写明视觉问题，AI 才能有针对性地重做。');
        await repository.rejectGenerationStep(currentDocument.documentId, plan.revision, review.step.stepId, review.candidate.attemptId, reason);
        setGenerationRejectionReason('');
        showToast('已退回这一小步，AI 将按视觉意见重做');
      } else if (action === 'rollback') {
        if (!review) throw new Error('没有可回滚的步骤。');
        await repository.rollbackGenerationStep(currentDocument.documentId, plan.revision, review.step.stepId);
        setSceneReloadToken((value) => value + 1);
        showToast('已回滚最近接受的 AI 步骤');
      } else if (action === 'pause') {
        await repository.pauseGeneration(currentDocument.documentId, plan.revision);
        showToast('AI 设计流程已暂停');
      } else {
        await repository.resumeGeneration(currentDocument.documentId, plan.revision);
        showToast('AI 设计流程已继续');
      }
      await refreshGenerationState();
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
      await refreshGenerationState().catch(() => undefined);
    } finally {
      setGenerationAction(undefined);
    }
  }

  async function commitSceneCommand(command: SceneEditorCommand, reason?: string): Promise<SceneDocument> {
    const scene = sceneDocumentRef.current;
    if (!repository || !scene) throw new Error('当前设计还没有可编辑的 Scene。');
    try {
      const result = await repository.editScene(scene.documentId, {
        transactionId: `studio:${crypto.randomUUID()}`,
        expectedRevision: scene.revision,
        reason,
        command
      });
      applySceneDocument(result.document);
      await refreshSceneHistory(scene.documentId);
      return result.document;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (/revision|版本|更新到/i.test(message)) {
        const latest = await repository.readScene(scene.documentId).catch(() => undefined);
        if (latest) {
          applySceneDocument(latest);
          await refreshSceneHistory(scene.documentId).catch(() => undefined);
        }
      }
      throw error;
    }
  }

  async function undoScene() {
    const scene = sceneDocumentRef.current;
    if (!repository || !scene || !sceneHistory?.undoCount) return;
    try {
      const next = await repository.undoScene(scene.documentId, scene.revision);
      applySceneDocument(next);
      await refreshSceneHistory(scene.documentId);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function redoScene() {
    const scene = sceneDocumentRef.current;
    if (!repository || !scene || !sceneHistory?.redoCount) return;
    try {
      const next = await repository.redoScene(scene.documentId, scene.revision);
      applySceneDocument(next);
      await refreshSceneHistory(scene.documentId);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function undo() {
    if (sceneEditingActive) {
      void undoScene();
      return;
    }
    const current = documentRef.current;
    const previous = past[past.length - 1];
    if (!current || !previous) return;
    setPast((items) => items.slice(0, -1));
    setFuture((items) => [structuredClone(current), ...items].slice(0, 60));
    setCurrent(historyDocument(previous, current));
    setDirty(true);
  }

  function redo() {
    if (sceneEditingActive) {
      void redoScene();
      return;
    }
    const current = documentRef.current;
    const next = future[0];
    if (!current || !next) return;
    setFuture((items) => items.slice(1));
    setPast((items) => [...items.slice(-59), structuredClone(current)]);
    setCurrent(historyDocument(next, current));
    setDirty(true);
  }

  function updateComponent(componentId: string, updater: (component: WebDesignComponent) => WebDesignComponent) {
    commitWithCanvasGrowth((current) => ({ ...current, components: current.components.map((component) => component.id === componentId ? updater(component) : component) }));
  }

  async function save(force = false, silent = false) {
    const current = documentRef.current;
    if (!repository || !current || saving || (!dirty && !force)) return;
    setSaving(true);
    try {
      const snapshot = structuredClone(current);
      const saved = await repository.save(snapshot, persistedRevision);
      if (editableDocumentPayload(saved) !== editableDocumentPayload(snapshot)) {
        throw new Error('保存返回的数据改变了当前设计，已停止应用该结果以保护画布布局。');
      }
      setCurrent(saved);
      setPersistedRevision(saved.revision);
      setDirty(false);
      setDocuments(await repository.list());
      if (!silent) showToast('已保存');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  }

  async function refresh() {
    const current = documentRef.current;
    if (!repository || !current) return;
    if (dirty && !window.confirm('当前有未保存修改，确定刷新并丢弃吗？')) return;
    try {
      openDocument(await repository.read(current.documentId));
      setDocuments(await repository.list());
      showToast('已读取 AI 或其他编辑器的最新修改');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function createNew() {
    if (!activeProject) return;
    setNewDesignName('');
    setNewDesignOpen(true);
    setProjectLibraryOpen(false);
  }

  async function refreshCatalog() {
    if (!repository) return;
    setDocuments(await repository.list());
  }

  async function createDesignFromSheet() {
    if (!repository || !activeProject || !newDesignName.trim()) return;
    try {
      const created = await repository.createInProject(activeProject.projectId, newDesignName, true);
      setActiveProject(await repository.readProject(activeProject.projectId));
      await refreshCatalog();
      setNewDesignOpen(false);
      setNewDesignName('');
      openDocument(created);
      setScreen('editor');
      replaceStudioLocation(activeProject.projectId, created.documentId);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function openProjectDocument(documentId: string) {
    if (!repository) return;
    if (dirty && !window.confirm('切换设计会丢弃未保存修改，确定继续吗？')) return;
    try {
      openDocument(await repository.read(documentId));
      setProjectLibraryOpen(false);
      setScreen('editor');
      replaceStudioLocation(activeProject?.projectId, documentId);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function goToActiveProject() {
    if (dirty && !window.confirm('返回项目首页会丢弃未保存修改，确定继续吗？')) return;
    setDocument(undefined);
    setDirty(false);
    setScreen('project');
    setProjectLibraryOpen(false);
    replaceStudioLocation(activeProject?.projectId);
  }

  async function deleteProjectDocument(target: DesignSummary) {
    if (!repository || !activeProject) return;
    if (!window.confirm(`确定永久删除“${target.title}”吗？`)) return;
    try {
      await repository.remove(target.documentId);
      setActiveProject(await repository.readProject(activeProject.projectId));
      if (document?.documentId === target.documentId) setDocument(undefined);
      await refreshCatalog();
      setScreen('project');
      replaceStudioLocation(activeProject.projectId);
      showToast(`已删除“${target.title}”`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function onPaletteDrag(event: DragEvent, shapeId: BasicShapeId) {
    event.dataTransfer.setData('application/x-web-design-shape', shapeId);
    event.dataTransfer.effectAllowed = 'copy';
  }

  function addUiLibraryComponent(libraryName: WebDesignLibraryName, definitionId: string, x: number, y: number, variantId?: string, targetSlot = editingSlot, registryElement?: LibraryPreviewSelection): WebDesignComponent | undefined {
    const current = documentRef.current;
    if (!current) return;
    const library = uiLibraryByName(libraryName);
    if (!library) return;
    const container = targetSlot ? current.components.find((candidate) => candidate.id === targetSlot.componentId) : undefined;
    const containerFrame = container ? resolveComponent(container, device) : undefined;
    const componentX = containerFrame ? containerFrame.x + x : x;
    const componentY = containerFrame ? containerFrame.y + y : y;
    let component = createComponentFromUiLibrary(libraryName, definitionId, componentX, componentY);
    if (variantId) component = applyUiLibraryVariant(component, variantId);
    if (registryElement) component = bindLibraryPreviewElement(component, library.displayName, registryElement);
    component.pageId = pageId;
    if (container && targetSlot) {
      component.parentId = container.id;
      component.slot = targetSlot.slotId;
      component.zIndex = Math.max(0, ...current.components.filter((item) => item.parentId === container.id && item.slot === targetSlot.slotId).map((item) => item.zIndex)) + 1;
    } else {
      component.zIndex = Math.max(1, ...componentsForPage(current, pageId).filter((item) => !contentContainerAncestor(current, item)).map((item) => item.zIndex)) + 1;
    }
    if (device !== 'desktop') component = updateComponentFrame(component, device, { x: componentX, y: componentY });
    const starterSlot = !container && ['Drawer', 'Modal', 'Dialog', 'Sheet', 'AlertDialog'].includes(definitionId) ? editableSlotsForUiComponent(component)[0] : undefined;
    const starter = starterSlot ? createSlotStarterComponents(component, starterSlot, 'form', pageId, device) : [];
    commitWithCanvasGrowth((active) => withGeneratedResponsiveLayouts({ ...active, components: [...active.components, component, ...starter] }, pageId), ['desktop', 'tablet', 'mobile']);
    setSelectedId(component.id);
    setSelectedIds([component.id]);
    showToast(container ? `已添加到${editableSlotsForUiComponent(container).find((slot) => slot.id === targetSlot?.slotId)?.label ?? '组件内容'}` : `已插入 ${library.displayName} ${component.library?.component}`);
    return component;
  }

  function scenePageRoot(scene: SceneDocument, targetPageId: string) {
    const page = scene.pages.find((candidate) => candidate.id === targetPageId);
    const root = page?.children[0];
    if (!page || !root || !('children' in root) || !Array.isArray(root.children)) {
      throw new Error('当前画板没有可插入内容的 Scene 根节点。');
    }
    return { page, root };
  }

  async function insertSceneLibraryComponent(
    libraryName: WebDesignLibraryName,
    definitionId: string,
    x: number,
    y: number,
    variantId?: string,
    registryElement?: LibraryPreviewSelection,
    targetPageId = pageId,
    insertionTarget?: SceneInsertionTarget
  ): Promise<SceneNode | undefined> {
    const scene = sceneDocumentRef.current;
    if (!scene) return undefined;
    const { root } = scenePageRoot(scene, targetPageId);
    const node = createSceneLibraryInstance({
      nodeId: `library:${crypto.randomUUID()}`,
      libraryName,
      definitionId,
      variantId,
      x: Math.max(0, Math.round(x)),
      y: Math.max(0, Math.round(y)),
      registryElement
    });
    await commitSceneCommand({
      type: 'insert-node',
      parentId: insertionTarget?.nodeId ?? root.id,
      slot: insertionTarget?.slot,
      index: insertionTarget?.index ?? root.children.length,
      node
    }, `用户从 ${libraryName} 组件库插入 ${node.name}。`);
    setSelectedId(node.id);
    setSelectedIds([node.id]);
    const targetName = insertionTarget ? indexSceneDocument(scene).get(insertionTarget.nodeId)?.node.name : undefined;
    showToast(targetName ? `已插入 ${node.name} 到 ${targetName}` : `已插入 ${node.name}`);
    return node;
  }

  async function insertSceneBasicShape(
    shape: BasicShapeId,
    x: number,
    y: number,
    targetPageId: string,
    insertionTarget?: SceneInsertionTarget
  ): Promise<SceneNode | undefined> {
    const scene = sceneDocumentRef.current;
    if (!scene) return undefined;
    const { root } = scenePageRoot(scene, targetPageId);
    const node = createSceneBasicShape({
      nodeId: `shape:${crypto.randomUUID()}`,
      shape,
      x: Math.max(0, Math.round(x)),
      y: Math.max(0, Math.round(y))
    });
    await commitSceneCommand({
      type: 'insert-node',
      parentId: insertionTarget?.nodeId ?? root.id,
      slot: insertionTarget?.slot,
      index: insertionTarget?.index ?? root.children.length,
      node
    }, `用户插入基本图形 ${node.name}。`);
    setSelectedId(node.id);
    setSelectedIds([node.id]);
    showToast(`已插入${node.name}`);
    return node;
  }

  async function onSceneCanvasDrop(event: DragEvent<HTMLDivElement>, artboard: WorkspaceArtboardPlacement) {
    event.preventDefault();
    event.stopPropagation();
    if (preview || interactionMode || !sceneDocumentRef.current) return;
    activateWorkspaceArtboard(artboard);
    const bounds = event.currentTarget.getBoundingClientRect();
    const scaleX = bounds.width / Math.max(1, artboard.viewportWidth);
    const scaleY = bounds.height / Math.max(1, event.currentTarget.offsetHeight);
    const x = (event.clientX - bounds.left) / Math.max(scaleX, .0001);
    const y = (event.clientY - bounds.top) / Math.max(scaleY, .0001);
    const libraryPayload = event.dataTransfer.getData('application/x-web-design-library');
    try {
      const scene = sceneDocumentRef.current;
      const insertionTarget = resolveSceneInsertionTarget({
        document: scene,
        pageId: artboard.pageId,
        viewportWidth: artboard.viewportWidth,
        point: { x, y },
        preferred: sceneContentFocus?.pageId === artboard.pageId ? sceneContentFocus : undefined
      });
      if (libraryPayload) {
        const parsed = JSON.parse(libraryPayload) as VariantPickerTarget & {
          definitionId?: string;
          variantId?: string;
          registryElement?: LibraryPreviewSelection;
        };
        const definitionId = parsed.definitionId ?? parsed.componentId;
        if (uiLibraryByName(parsed.library)?.components.some((item) => item.id === definitionId)) {
          await insertSceneLibraryComponent(parsed.library, definitionId, insertionTarget.x, insertionTarget.y, parsed.variantId, parsed.registryElement, artboard.pageId, insertionTarget);
          setVariantPickerDrag(undefined);
          if (parsed.registryElement) setVariantPickerTarget(undefined);
          return;
        }
      }
      const shapeId = event.dataTransfer.getData('application/x-web-design-shape') as BasicShapeId;
      if (palette.some((item) => item.id === shapeId)) await insertSceneBasicShape(shapeId, insertionTarget.x, insertionTarget.y, artboard.pageId, insertionTarget);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function insertUiLibraryComponent(libraryName: WebDesignLibraryName, definitionId: string, variantId?: string, registryElement?: LibraryPreviewSelection) {
    const definition = uiLibraryByName(libraryName)?.components.find((candidate) => candidate.id === definitionId);
    if (!definition) return;
    if (sceneDocumentRef.current) {
      const targetArtboard = workspacePlacement?.artboards.find((candidate) => candidate.artboardId === activeArtboardId)
        ?? workspacePlacement?.artboards.find((candidate) => candidate.pageId === pageId);
      const targetPageId = targetArtboard?.pageId ?? pageId;
      const targetWidth = targetArtboard?.viewportWidth ?? breakpoint.width;
      const width = registryElement?.width ?? definition.width;
      const focusedTarget = sceneContentFocus?.pageId === targetPageId
        ? resolveSceneInsertionTarget({
          document: sceneDocumentRef.current,
          pageId: targetPageId,
          viewportWidth: targetWidth,
          point: { x: 0, y: 0 },
          preferred: sceneContentFocus
        })
        : undefined;
      setVariantPickerTarget(undefined);
      void insertSceneLibraryComponent(
        libraryName,
        definitionId,
        focusedTarget ? 24 : Math.max(24, Math.round((targetWidth - width) / 2)),
        focusedTarget ? 24 : 80,
        variantId,
        registryElement,
        targetPageId,
        focusedTarget ? { ...focusedTarget, x: 24, y: 24 } : undefined
      ).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
      return;
    }
    let inserted: WebDesignComponent | undefined;
    if (editingSlotCanvasSize) {
      const width = registryElement?.width ?? definition.width;
      inserted = addUiLibraryComponent(libraryName, definitionId, Math.max(12, Math.round((editingSlotCanvasSize.width - width) / 2)), 28, variantId, editingSlot, registryElement);
    } else {
      const width = registryElement?.width ?? definition.width;
      inserted = addUiLibraryComponent(libraryName, definitionId, Math.max(24, Math.round((breakpoint.width - width) / 2)), 80, variantId, editingSlot, registryElement);
    }
    setVariantPickerTarget(undefined);
    const compoundSlot = inserted?.library?.props.registryDemo && !inserted.library.props.registryElement
      ? editableSlotsForUiComponent(inserted).find((slot) => slot.id === 'content')
      : undefined;
    if (inserted && compoundSlot) {
      window.setTimeout(() => { void editComponentSlot(inserted!, compoundSlot.id, { compoundOnly: true }); }, 120);
    }
  }

  function chooseUiLibraryPreviewElement(libraryName: WebDesignLibraryName, definitionId: string, variantId: string, registryElement: LibraryPreviewSelection) {
    const replaceComponentId = variantPickerTarget?.replaceComponentId;
    if (!replaceComponentId) {
      insertUiLibraryComponent(libraryName, definitionId, variantId, registryElement);
      return;
    }
    const library = uiLibraryByName(libraryName);
    if (!library) return;
    const scene = sceneDocumentRef.current;
    const sceneNode = scene && replaceComponentId ? indexSceneDocument(scene).get(replaceComponentId)?.node : undefined;
    if (scene && sceneNode) {
      if (sceneNode.type !== 'library-instance') {
        showToast('当前 Scene 图层不是组件库实例，不能直接替换变体。');
        return;
      }
      const replacement = createSceneLibraryInstance({
        nodeId: sceneNode.id,
        libraryName,
        definitionId,
        variantId,
        x: sceneNode.frame.x,
        y: sceneNode.frame.y,
        registryElement
      });
      setVariantPickerTarget(undefined);
      void commitSceneCommand({
        type: 'update-node',
        nodeId: sceneNode.id,
        patches: [
          { path: ['name'], value: replacement.name },
          { path: ['library'], value: replacement.library },
          { path: ['component'], value: replacement.component },
          { path: ['variant'], value: replacement.variant ?? '' },
          { path: ['properties'], value: replacement.properties },
          { path: ['content'], value: replacement.content ?? '' },
          { path: ['frame', 'width'], value: replacement.frame.width },
          { path: ['frame', 'height'], value: replacement.frame.height }
        ]
      }, `用户把 Scene 组件替换为 ${registryElement.label}。`).then(() => {
        setSelectedId(sceneNode.id);
        setSelectedIds([sceneNode.id]);
        showToast(`已改为 ${registryElement.label}`);
      }).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
      return;
    }
    if (scene) {
      setVariantPickerTarget(undefined);
      showToast('要替换的 Scene 组件已经不存在，请重新选择。');
      return;
    }
    updateComponent(replaceComponentId, (component) => {
      let next = bindLibraryPreviewElement(applyUiLibraryVariant(component, variantId), library.displayName, registryElement);
      if (device !== 'desktop') next = updateComponentFrame(next, device, { width: next.width, height: next.height });
      return next;
    });
    setVariantPickerTarget(undefined);
    setSelectedId(replaceComponentId);
    setSelectedIds([replaceComponentId]);
    showToast(`已改为 ${registryElement.label}`);
  }

  function beginUiLibraryPreviewPointerDrag(
    libraryName: WebDesignLibraryName,
    definitionId: string,
    variantId: string,
    state: LibraryPreviewPointerEvent
  ) {
    const next: VariantPickerPointerDrag = {
      selection: state.selection,
      pointerId: state.pointerId,
      clientX: state.clientX,
      clientY: state.clientY,
      libraryName,
      definitionId,
      variantId,
      startClientX: state.clientX,
      startClientY: state.clientY,
      dragging: false
    };
    variantPickerDragRef.current = next;
    flushSync(() => setVariantPickerDrag(next));
  }

  function moveUiLibraryPreviewPointerDragAt(clientX: number, clientY: number) {
    const active = variantPickerDragRef.current;
    if (!active) return;
    const dragging = active.dragging
      || Math.hypot(clientX - active.startClientX, clientY - active.startClientY) >= 5;
    if (!dragging && clientX === active.clientX && clientY === active.clientY) return;
    const next = { ...active, clientX, clientY, dragging };
    variantPickerDragRef.current = next;
    setVariantPickerDrag(next);
  }

  function finishUiLibraryPreviewPointerDragAt(clientX: number, clientY: number, cancelled = false) {
    const active = variantPickerDragRef.current;
    if (!active) return;
    variantPickerDragRef.current = undefined;
    setVariantPickerDrag(undefined);
    if (cancelled) return;
    if (!active.dragging) {
      chooseUiLibraryPreviewElement(active.libraryName, active.definitionId, active.variantId, active.selection);
      return;
    }
    dropUiLibraryPreviewElement(active.libraryName, active.definitionId, active.variantId, active.selection, {
      clientX,
      clientY
    });
  }

  function handleUiLibraryPreviewPointerEvent(
    libraryName: WebDesignLibraryName,
    definitionId: string,
    variantId: string,
    event: LibraryPreviewPointerEvent
  ) {
    if (event.phase === 'start') beginUiLibraryPreviewPointerDrag(libraryName, definitionId, variantId, event);
    else if (event.phase === 'move') moveUiLibraryPreviewPointerDragAt(event.clientX, event.clientY);
    else finishUiLibraryPreviewPointerDragAt(event.clientX, event.clientY, event.phase === 'cancel');
  }

  function dropUiLibraryPreviewElement(
    libraryName: WebDesignLibraryName,
    definitionId: string,
    variantId: string,
    registryElement: LibraryPreviewSelection,
    point: { clientX: number; clientY: number }
  ) {
    variantPickerDragRef.current = undefined;
    setVariantPickerDrag(undefined);
    if (variantPickerTarget?.replaceComponentId) {
      chooseUiLibraryPreviewElement(libraryName, definitionId, variantId, registryElement);
      return;
    }
    const canvasSelector = editingSlot ? '.slot-design-canvas' : '.design-canvas:not(.slot-design-canvas)';
    const canvas = [...window.document.querySelectorAll<HTMLElement>(canvasSelector)].find((candidate) => {
      const bounds = candidate.getBoundingClientRect();
      return point.clientX >= bounds.left && point.clientX <= bounds.right
        && point.clientY >= bounds.top && point.clientY <= bounds.bottom;
    });
    if (!canvas) {
      showToast('请把元素拖到中间画布区域');
      return;
    }
    const bounds = canvas.getBoundingClientRect();
    const scaleX = bounds.width / Math.max(1, canvas.offsetWidth);
    const scaleY = bounds.height / Math.max(1, canvas.offsetHeight);
    const x = Math.max(0, Math.round((point.clientX - bounds.left) / Math.max(scaleX, .0001) - registryElement.width / 2));
    const y = Math.max(0, Math.round((point.clientY - bounds.top) / Math.max(scaleY, .0001) - registryElement.height / 2));
    if (sceneDocumentRef.current && !editingSlot) {
      const artboard = workspacePlacement?.artboards.find((candidate) => candidate.artboardId === canvas.dataset.artboardId);
      if (!artboard) {
        showToast('没有找到目标 Scene 画板，请重新拖入。');
        return;
      }
      activateWorkspaceArtboard(artboard);
      const insertionTarget = resolveSceneInsertionTarget({
        document: sceneDocumentRef.current,
        pageId: artboard.pageId,
        viewportWidth: artboard.viewportWidth,
        point: { x, y },
        preferred: sceneContentFocus?.pageId === artboard.pageId ? sceneContentFocus : undefined
      });
      void insertSceneLibraryComponent(libraryName, definitionId, insertionTarget.x, insertionTarget.y, variantId, registryElement, artboard.pageId, insertionTarget)
        .catch((error) => showToast(error instanceof Error ? error.message : String(error)));
    } else {
      addUiLibraryComponent(libraryName, definitionId, x, y, variantId, editingSlot, registryElement);
    }
    setVariantPickerTarget(undefined);
  }

  function enterSlotEditor(next: EditingSlot) {
    if (!editingSlot) workspaceCameraBeforeSlot.current = { ...workspaceCamera };
    setEditingSlot(next);
  }

  function resetSlotEditorCamera() {
    workspaceCameraBeforeSlot.current = undefined;
    slotCameraContext.current = undefined;
  }

  async function editComponentSlot(component: WebDesignComponent, slotId: string, options: { compoundOnly?: boolean } = {}): Promise<boolean> {
    if (interactionMode) setInteractionMode(false);
    const current = documentRef.current;
    const slot = editableSlotsForUiComponent(component).find((candidate) => candidate.id === slotId);
    let first = current ? componentsInSlot(current, component.id, slotId)[0] : undefined;
    if (current && slot && !first) {
      const officialDemo = await materializeOfficialDemoContent(component, slot, pageId, device);
      const officialRoots = officialDemo.filter((candidate) => candidate.parentId === component.id);
      if (options.compoundOnly && officialRoots.length < 2) return false;
      const materialized = officialDemo.length > 0 ? officialDemo : materializeExistingSlotContent(component, slot, pageId, device);
      if (options.compoundOnly && officialDemo.length === 0) return false;
      if (materialized.length > 0) {
        commitWithCanvasGrowth((active) => ({
          ...active,
          components: [
            ...active.components.map((candidate) => candidate.id === component.id ? {
              ...candidate,
              content: '',
              library: officialDemo.length > 0 && candidate.library ? {
                ...candidate.library,
                props: { ...candidate.library.props, editorDetachedContent: true }
              } : candidate.library
            } : candidate),
            ...materialized
          ]
        }));
        first = (officialDemo.length > 0
          ? officialDemo.filter((candidate) => candidate.parentId === component.id)
            .sort((left, right) => resolveComponent(left, device).y - resolveComponent(right, device).y)[0]
          : undefined) ?? materialized[0];
      }
    }
    enterSlotEditor({ componentId: component.id, slotId });
    setSelectedId(first?.id);
    setSelectedIds(first ? [first.id] : []);
    return true;
  }

  function exitSlotEditor() {
    const containerId = editingSlot?.componentId;
    const previousCamera = workspaceCameraBeforeSlot.current;
    resetSlotEditorCamera();
    setEditingSlot(undefined);
    if (previousCamera) setWorkspaceCamera(previousCamera);
    setSelectedId(containerId);
    setSelectedIds(containerId ? [containerId] : []);
  }

  function insertSlotTemplate(template: 'form' | 'details') {
    const current = documentRef.current;
    if (!current || !editingContainer || !editingSlotDefinition) return;
    const existing = componentsInSlot(current, editingContainer.id, editingSlotDefinition.id);
    if (existing.length > 0 && !window.confirm('当前内容区域已有组件，继续会在现有内容下方添加模板，是否继续？')) return;
    const starter = createSlotStarterComponents(editingContainer, editingSlotDefinition, template, pageId, device);
    const offsetY = existing.length === 0 ? 0 : Math.max(...existing.map((component) => resolveComponent(component, device).y - resolveComponent(editingContainer, device).y + resolveComponent(component, device).height)) + 24;
    const adjusted = offsetY === 0 ? starter : starter.map((component) => {
      const frame = resolveComponent(component, device);
      return updateComponentFrame(component, device, { y: frame.y + offsetY });
    });
    commitWithCanvasGrowth((active) => withGeneratedResponsiveLayouts({
      ...active,
      components: [...active.components, ...adjusted]
    }, pageId), ['desktop', 'tablet', 'mobile']);
    setSelectedId(adjusted[0]?.id);
    setSelectedIds(adjusted[0] ? [adjusted[0].id] : []);
    showToast(template === 'form' ? '已插入可编辑表单' : '已插入可编辑详情内容');
  }

  function onCanvasDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    const current = documentRef.current;
    if (!current || preview || interactionMode) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const x = Math.round((event.clientX - bounds.left) / zoom);
    const y = Math.round((event.clientY - bounds.top) / zoom);
    const libraryPayload = event.dataTransfer.getData('application/x-web-design-library');
    if (libraryPayload) {
      try {
        const parsed = JSON.parse(libraryPayload) as VariantPickerTarget & { definitionId?: string; variantId?: string; registryElement?: LibraryPreviewSelection };
        const definitionId = parsed.definitionId ?? parsed.componentId;
        if (uiLibraryByName(parsed.library)?.components.some((item) => item.id === definitionId)) {
          addUiLibraryComponent(parsed.library, definitionId, x, y, parsed.variantId, editingSlot, parsed.registryElement);
          setVariantPickerDrag(undefined);
          if (parsed.registryElement) setVariantPickerTarget(undefined);
          return;
        }
      } catch { /* Ignore malformed drag payloads. */ }
    }
    const shapeId = event.dataTransfer.getData('application/x-web-design-shape') as BasicShapeId;
    if (!palette.some((item) => item.id === shapeId)) return;
    let component = basicShapeDefaults(shapeId, x, y);
    component.pageId = pageId;
    if (editingSlot && editingContainer) {
      const parentFrame = resolveComponent(editingContainer, device);
      component.x = editingContainer.x + x;
      component.y = editingContainer.y + y;
      component.parentId = editingContainer.id;
      component.slot = editingSlot.slotId;
      component.zIndex = Math.max(0, ...current.components.filter((item) => item.parentId === editingContainer.id && item.slot === editingSlot.slotId).map((item) => item.zIndex)) + 1;
      if (device !== 'desktop') component = updateComponentFrame(component, device, { x: parentFrame.x + x, y: parentFrame.y + y });
    } else {
      if (device !== 'desktop') component = updateComponentFrame(component, device, { x, y });
      component.zIndex = Math.max(1, ...componentsForPage(current, pageId).filter((item) => !contentContainerAncestor(current, item)).map((item) => item.zIndex)) + 1;
    }
    commitWithCanvasGrowth((active) => withGeneratedResponsiveLayouts({ ...active, components: [...active.components, component] }, pageId), ['desktop', 'tablet', 'mobile']);
    setSelectedId(component.id);
    setSelectedIds([component.id]);
  }

  function beginInteraction(event: ReactPointerEvent, component: WebDesignComponent, kind: Interaction['kind']) {
    if (preview || interactionMode) return;
    if (spacePressed.current || event.button === 1) return;
    if (workspaceShell.activeTool === 'hand') return;
    event.preventDefault();
    event.stopPropagation();
    if ((event.metaKey || event.ctrlKey) && kind === 'move') {
      const current = documentRef.current;
      const canvas = (event.currentTarget as HTMLElement).closest<HTMLElement>('.design-canvas');
      if (!current || !canvas) return;
      const bounds = canvas.getBoundingClientRect();
      const point = {
        x: (event.clientX - bounds.left) / Math.max(bounds.width / Math.max(1, canvas.offsetWidth), .0001),
        y: (event.clientY - bounds.top) / Math.max(bounds.height / Math.max(1, canvas.offsetHeight), .0001)
      };
      const candidates = selectionCandidatesAtPoint(selectableNodesForCurrentEditor(current), point);
      if (candidates.length <= 1) {
        setSelectionCandidatePopover(undefined);
        selectComponent(candidates[0]?.id ?? component.id);
      } else {
        setSelectionCandidatePopover({ clientX: event.clientX, clientY: event.clientY, candidates });
      }
      return;
    }
    setSelectionCandidatePopover(undefined);
    if (workspaceShell.activeTool === 'comment') {
      setSelectedId(component.id);
      setSelectedIds([component.id]);
      setInspectorTab('ai');
      if (!workspaceShell.rightPanelOpen) dispatchWorkspaceShell({ type: 'toggle-right-panel' });
      showToast(`已选择“${component.name}”，请在右侧添加批注`);
      return;
    }
    if (event.shiftKey) {
      const next = selectedIds.includes(component.id) ? selectedIds.filter((id) => id !== component.id) : [...selectedIds, component.id];
      setSelectedIds(next);
      setSelectedId(next.includes(component.id) ? component.id : next[0]);
      return;
    }
    const nextSelectedIds = selectedIds.includes(component.id) ? selectedIds : [component.id];
    setSelectedId(component.id);
    setSelectedIds(nextSelectedIds);
    if (component.locked) {
      showToast('组件已锁定');
      return;
    }
    const current = documentRef.current;
    if (!current) return;
    interaction.current = {
      kind,
      componentId: component.id,
      pointerX: event.clientX,
      pointerY: event.clientY,
      frame: resolveComponent(component, device),
      selectedIds: nextSelectedIds,
      snapshot: structuredClone(current),
      scale: zoom,
      scoped: Boolean(editingSlot)
    };
  }

  function beginCanvasPan(event: ReactPointerEvent<HTMLDivElement>) {
    const handTool = event.button === 0 && workspaceShell.activeTool === 'hand';
    if (preview || (event.button !== 1 && !(event.button === 0 && spacePressed.current) && !handTool)) return;
    event.preventDefault();
    canvasPan.current = {
      pointerX: event.clientX,
      pointerY: event.clientY,
      camera: workspaceCamera
    };
    setCanvasPanning(true);
  }

  function beginCanvasMarquee(event: ReactPointerEvent<HTMLElement>) {
    if (preview || interactionMode || event.button !== 0 || spacePressed.current) return;
    if (workspaceShell.activeTool === 'hand' || workspaceShell.activeTool === 'comment') return;
    const current = documentRef.current;
    if (!current) return;
    event.preventDefault();
    setSelectionCandidatePopover(undefined);
    const canvas = event.currentTarget;
    const bounds = canvas.getBoundingClientRect();
    const scaleX = bounds.width / Math.max(1, canvas.offsetWidth);
    const scaleY = bounds.height / Math.max(1, canvas.offsetHeight);
    canvasMarquee.current = {
      pointerId: event.pointerId,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startPoint: {
        x: (event.clientX - bounds.left) / Math.max(scaleX, .0001),
        y: (event.clientY - bounds.top) / Math.max(scaleY, .0001)
      },
      canvas,
      nodes: selectableNodesForCurrentEditor(current),
      initialIds: event.shiftKey ? [...selectedIds] : [],
      initialPrimaryId: event.shiftKey ? selectedId : undefined,
      additive: event.shiftKey,
      moved: false
    };
    setMarqueeRect(undefined);
  }

  function updateSelected(changes: Partial<WebDesignComponent>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride({ ...component, ...changes }, 'content', true));
  }

  function updateSelectedFrame(changes: Partial<Pick<ResolvedWebDesignComponent, 'x' | 'y' | 'width' | 'height' | 'hidden'>>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => {
      const constrained = changes.width !== undefined || changes.height !== undefined ? constrainComponentFrame(component, device, changes) : undefined;
      return setSymbolOverride(updateComponentFrame(component, device, { ...changes, ...constrained }), 'frame', true);
    });
  }

  function updateInspectedFrame(changes: Partial<Pick<ResolvedWebDesignComponent, 'x' | 'y' | 'width' | 'height' | 'hidden'>>) {
    if (!editingContainer || !editingSlot || !selected || !documentRef.current || slotIdForDescendant(documentRef.current, selected, editingContainer.id) !== editingSlot.slotId) {
      updateSelectedFrame(changes);
      return;
    }
    const containerFrame = resolveComponent(editingContainer, device);
    const translated = { ...changes };
    if (changes.x !== undefined) translated.x = containerFrame.x + changes.x;
    if (changes.y !== undefined) translated.y = containerFrame.y + changes.y;
    updateSelectedFrame(translated);
  }

  function updateSelectedStyle(changes: Partial<WebComponentStyle>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => inspectorVisualState === 'default'
      ? setSymbolOverride(updateComponentStyle(component, device, changes), 'style', true)
      : setSymbolOverride({
        ...component,
        states: { ...component.states, [inspectorVisualState]: { ...component.states?.[inspectorVisualState], ...changes } }
      }, 'style', true));
  }

  function clearSelectedVisualState() {
    if (!selected || inspectorVisualState === 'default') return;
    updateComponent(selected.id, (component) => {
      const states = { ...component.states };
      delete states[inspectorVisualState];
      return setSymbolOverride({ ...component, states: Object.keys(states).length > 0 ? states : undefined }, 'style', true);
    });
  }

  function updateSelectedCustomCss(customCss: Record<string, string | number>) {
    updateSelectedStyle({ customCss: Object.keys(customCss).length > 0 ? customCss : undefined });
  }

  function updateSelectedHorizontalConstraint(horizontal: WebHorizontalConstraint) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride({
      ...component,
      constraints: { ...component.constraints, [device]: { ...component.constraints?.[device], horizontal } }
    }, 'frame', true));
  }

  function updateSelectedSizeConstraints(changes: Partial<WebComponentConstraints>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride({
      ...component,
      constraints: { ...component.constraints, [device]: { horizontal: component.constraints?.[device]?.horizontal ?? 'auto', ...component.constraints?.[device], ...changes } }
    }, 'frame', true));
  }

  function deleteSelected() {
    const current = documentRef.current;
    if (selectedIds.length === 0 || !current) return;
    const removed = new Set(selectedIds.flatMap((id) => [id, ...descendantIds(current, id)]));
    commit((active) => ({
      ...active,
      components: active.components
        .filter((component) => !removed.has(component.id))
        .map((component) => component.parentId && removed.has(component.parentId) ? { ...component, parentId: undefined } : component),
      requests: active.requests.filter((request) => !request.componentId || !removed.has(request.componentId))
    }));
    setSelectedId(undefined);
    if (editingSlot && removed.has(editingSlot.componentId)) {
      resetSlotEditorCamera();
      setEditingSlot(undefined);
    }
    setSelectedIds([]);
  }

  function duplicateSelected() {
    if (sceneEditingActive) {
      duplicateSceneSelection();
      return;
    }
    const current = documentRef.current;
    if (selectedIds.length === 0 || !current) return;
    const cloned = cloneComponentSubtrees(current, selectedIds, pageId, 20, current);
    commitWithCanvasGrowth((active) => ({ ...active, components: [...active.components, ...cloned.components] }));
    setSelectedId(cloned.rootIds[0]);
    setSelectedIds(cloned.rootIds);
  }

  function copySelected() {
    if (sceneEditingActive) {
      copySceneSelection();
      return;
    }
    const current = documentRef.current;
    if (!current || selectedIds.length === 0) return;
    setClipboard({ document: structuredClone(current), componentIds: [...selectedIds] });
    showToast(`已复制 ${selectedIds.length} 个组件`);
  }

  function pasteClipboard() {
    if (sceneEditingActive) {
      pasteSceneClipboard();
      return;
    }
    const current = documentRef.current;
    if (!current || !clipboard) return;
    const cloned = cloneComponentSubtrees(clipboard.document, clipboard.componentIds, pageId, 20, current);
    commitWithCanvasGrowth((active) => ({ ...active, components: [...active.components, ...cloned.components] }));
    setSelectedId(cloned.rootIds[0]);
    setSelectedIds(cloned.rootIds);
    showToast('已粘贴到当前页面');
  }

  function reorderSelected(action: LayerAction) {
    if (!selectedId) return;
    commit((current) => {
      const ordered = [...componentsForPage(current, pageId)].sort((left, right) => left.zIndex - right.zIndex);
      const index = ordered.findIndex((component) => component.id === selectedId);
      if (index < 0) return current;
      const [item] = ordered.splice(index, 1);
      const targetIndex = action === 'front' ? ordered.length : action === 'back' ? 0 : action === 'forward' ? Math.min(ordered.length, index + 1) : Math.max(0, index - 1);
      ordered.splice(targetIndex, 0, item);
      const zIndexes = new Map(ordered.map((component, zIndex) => [component.id, zIndex + 1]));
      return { ...current, components: current.components.map((component) => zIndexes.has(component.id) ? { ...component, zIndex: zIndexes.get(component.id)! } : component) };
    });
  }

  function alignSelected(action: AlignAction) {
    if (!inspectedFrame) return;
    const targetWidth = editingSlotCanvasSize?.width ?? breakpoint.width;
    const targetHeight = editingSlotCanvasSize?.height ?? breakpoint.height;
    const changes: Partial<ResolvedWebDesignComponent> = {};
    if (action === 'left') changes.x = 0;
    if (action === 'center') changes.x = Math.round((targetWidth - inspectedFrame.width) / 2);
    if (action === 'right') changes.x = targetWidth - inspectedFrame.width;
    if (action === 'top') changes.y = 0;
    if (action === 'middle') changes.y = Math.round((targetHeight - inspectedFrame.height) / 2);
    if (action === 'bottom') changes.y = targetHeight - inspectedFrame.height;
    updateInspectedFrame(changes);
  }

  function nudgeSelected(dx: number, dy: number) {
    if (selectedIds.length === 0 || selectedIds.some((id) => documentRef.current?.components.find((component) => component.id === id)?.locked)) return;
    commitWithCanvasGrowth((current) => {
      const moving = new Set(selectedRootIds(current, selectedIds).flatMap((id) => [id, ...descendantIds(current, id)]));
      const moved = moveComponentsWithDescendants(current, selectedIds, device, dx, dy);
      return { ...moved, components: moved.components.map((component) => moving.has(component.id) ? setSymbolOverride(component, 'frame', true) : component) };
    });
  }

  function toggleHidden(component: WebDesignComponent) {
    const resolved = resolveComponent(component, device);
    updateComponent(component.id, (current) => updateComponentFrame(current, device, { hidden: !resolved.hidden }));
  }

  function toggleLocked(component: WebDesignComponent) {
    updateComponent(component.id, (current) => ({ ...current, locked: !current.locked }));
  }

  function activateWorkspaceArtboard(artboard: WorkspaceArtboardPlacement) {
    const current = documentRef.current;
    if (!current) return;
    setActiveArtboardId(artboard.artboardId);
    setDevice(deviceForWorkspaceArtboard(current, artboard));
    setPageId(artboard.pageId);
    resetSlotEditorCamera();
    setEditingSlot(undefined);
    setSelectedId(undefined);
    setSelectedIds([]);
  }

  function beginWorkspaceArtboardMove(event: ReactPointerEvent, artboard: WorkspaceArtboardPlacement) {
    if (event.button !== 0 || preview || editingSlot) return;
    event.preventDefault();
    event.stopPropagation();
    activateWorkspaceArtboard(artboard);
    workspaceArtboardDrag.current = {
      artboardId: artboard.artboardId,
      pointerX: event.clientX,
      pointerY: event.clientY,
      x: artboard.x,
      y: artboard.y
    };
  }

  function updateActiveWorkspaceViewport(width: number, height: number) {
    if (!activeArtboardId) return;
    setWorkspacePlacement((current) => current ? {
      ...current,
      artboards: current.artboards.map((artboard) => artboard.artboardId === activeArtboardId
        ? { ...artboard, viewportWidth: width, viewportHeight: height }
        : artboard)
    } : current);
  }

  async function addWorkspaceSurface(surfaceKind: WorkspaceSurfaceKind = newSurfaceKind) {
    const current = documentRef.current;
    const scene = sceneDocumentRef.current;
    if (!current || !workspacePlacement) return;
    if (!scene) {
      showToast('请先让 AI 创建 Scene 设计，再添加新的独立画板');
      return;
    }
    const pageIndex = scene.pages.length + 1;
    const surfaceLabel = WORKSPACE_SURFACE_LABELS[surfaceKind];
    const nextPageId = `${surfaceKind}:${crypto.randomUUID()}`;
    const nextPageName = surfaceKind === 'page' ? `页面 ${pageIndex}` : `${surfaceLabel} ${pageIndex}`;
    const desktop = breakpointFor(current, 'desktop');
    const fixedSize = surfaceKind === 'page' || surfaceKind === 'state' ? undefined : WORKSPACE_SURFACE_SIZES[surfaceKind];
    const width = fixedSize?.width ?? desktop.width;
    const height = fixedSize?.height ?? workspaceViewportHeight(current, 'desktop');
    const right = workspacePlacement.artboards.length === 0
      ? 0
      : Math.max(...workspacePlacement.artboards.map((artboard) => artboard.x + artboard.viewportWidth)) + WORKSPACE_ARTBOARD_GAP;
    const artboard: WorkspaceArtboardPlacement = {
      artboardId: `artboard-${crypto.randomUUID().slice(0, 8)}`,
      pageId: nextPageId,
      surfaceKind,
      viewportWidth: width,
      viewportHeight: height,
      x: right,
      y: 0
    };
    try {
      await commitSceneCommand({
        type: 'create-page',
        pageId: nextPageId,
        name: nextPageName,
        rootNodeId: `root:${crypto.randomUUID()}`,
        width,
        height
      }, `用户创建独立${surfaceLabel}画板。`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
      return;
    }
    setWorkspacePlacement({ ...workspacePlacement, artboards: [...workspacePlacement.artboards, artboard] });
    setActiveArtboardId(artboard.artboardId);
    setPageId(nextPageId);
    setDevice(deviceForWorkspaceArtboard(current, artboard));
    resetSlotEditorCamera();
    setEditingSlot(undefined);
    setSelectedId(undefined);
    setSelectedIds([]);
    showToast(`已创建独立${surfaceLabel}画板，可以分多次让 AI 继续设计`);
  }

  function removeActiveWorkspaceArtboard() {
    if (!workspacePlacement || !activeArtboardId || workspacePlacement.artboards.length <= 1) {
      showToast('工作区至少保留一个画板');
      return;
    }
    const remaining = workspacePlacement.artboards.filter((artboard) => artboard.artboardId !== activeArtboardId);
    const next = remaining[0];
    setWorkspacePlacement({ ...workspacePlacement, artboards: remaining });
    if (next) activateWorkspaceArtboard(next);
  }

  function fitAllWorkspaceArtboards() {
    const current = documentRef.current;
    const viewport = canvasScroll.current;
    if (!current || !viewport || !workspacePlacement?.artboards.length) return;
    setWorkspaceCamera(fitWorkspaceRect(
      workspaceArtboardBounds(current, workspacePlacement.artboards, sceneDocumentRef.current),
      { width: viewport.clientWidth, height: viewport.clientHeight },
      { top: 92, right: 64, bottom: 92, left: 64 }
    ));
  }

  function fitWorkspaceArtboard(artboard: WorkspaceArtboardPlacement) {
    const current = documentRef.current;
    const viewport = canvasScroll.current;
    if (!current || !viewport) return;
    setWorkspaceCamera(fitWorkspaceRect(
      workspaceArtboardContentBounds(current, artboard, sceneDocumentRef.current),
      { width: viewport.clientWidth, height: viewport.clientHeight },
      { top: 92, right: 64, bottom: 92, left: 64 }
    ));
  }

  function fitActiveWorkspaceArtboard() {
    const artboard = workspacePlacement?.artboards.find((candidate) => candidate.artboardId === activeArtboardId);
    if (artboard) fitWorkspaceArtboard(artboard);
  }

  function fitSlotEditorContent() {
    const viewport = canvasScroll.current;
    if (!viewport || !editingSlotCanvasSize) return;
    setWorkspaceCamera(fitWorkspaceRect(
      slotEditorFrameBounds(editingSlotCanvasSize),
      { width: viewport.clientWidth, height: viewport.clientHeight },
      { top: 112, right: 72, bottom: 88, left: 72 },
      2.5
    ));
  }

  function fitWorkspaceSelection() {
    const current = documentRef.current;
    const viewport = canvasScroll.current;
    if (!current || !viewport || selectedIds.length === 0) return;
    const selected = new Set(selectedIds);
    if (editingSlot && editingContainer) {
      const containerFrame = resolveComponent(editingContainer, device);
      const bounds = unionWorkspaceRects(current.components.flatMap((component) => {
        if (!selected.has(component.id) || slotIdForDescendant(current, component, editingContainer.id) !== editingSlot.slotId) return [];
        const frame = resolveComponent(component, device);
        if (frame.hidden) return [];
        return [{
          x: SLOT_EDITOR_CANVAS_INSETS.left + frame.x - containerFrame.x,
          y: SLOT_EDITOR_HEADER_HEIGHT + SLOT_EDITOR_CANVAS_INSETS.top + frame.y - containerFrame.y,
          width: frame.width,
          height: frame.height
        }];
      }));
      if (!bounds) return;
      const padding = Math.max(24, Math.min(80, Math.max(bounds.width, bounds.height) * 0.12));
      setWorkspaceCamera(fitWorkspaceRect(
        { x: bounds.x - padding, y: bounds.y - padding, width: bounds.width + padding * 2, height: bounds.height + padding * 2 },
        { width: viewport.clientWidth, height: viewport.clientHeight },
        { top: 112, right: 96, bottom: 104, left: 96 },
        2.5
      ));
      return;
    }
    if (!workspacePlacement) return;
    if (sceneDocument) {
      const bounds = unionWorkspaceRects(workspacePlacement.artboards.flatMap((artboard) => {
        const local = sceneArtboardSelectionBounds(sceneDocument, artboard.pageId, artboard.viewportWidth, selectedIds);
        return local ? [{ x: artboard.x + local.x, y: artboard.y + local.y, width: local.width, height: local.height }] : [];
      }));
      if (!bounds) return;
      const padding = Math.max(24, Math.min(80, Math.max(bounds.width, bounds.height) * 0.12));
      setWorkspaceCamera(fitWorkspaceRect(
        { x: bounds.x - padding, y: bounds.y - padding, width: bounds.width + padding * 2, height: bounds.height + padding * 2 },
        { width: viewport.clientWidth, height: viewport.clientHeight },
        { top: 112, right: 96, bottom: 104, left: 96 },
        2.5
      ));
      return;
    }
    const firstPageId = pagesForDocument(current)[0].id;
    const boardByPage = new Map(workspacePlacement.artboards.map((artboard) => [artboard.pageId, artboard]));
    const bounds = unionWorkspaceRects(current.components.flatMap((component) => {
      if (!selected.has(component.id)) return [];
      const componentPageId = component.pageId ?? firstPageId;
      const artboard = boardByPage.get(componentPageId);
      if (!artboard) return [];
      const frame = resolveComponent(component, deviceForWorkspaceArtboard(current, artboard));
      if (frame.hidden) return [];
      return [{ x: artboard.x + frame.x, y: artboard.y + frame.y, width: frame.width, height: frame.height }];
    }));
    if (!bounds) return;
    const padding = Math.max(24, Math.min(80, Math.max(bounds.width, bounds.height) * 0.12));
    setWorkspaceCamera(fitWorkspaceRect(
      { x: bounds.x - padding, y: bounds.y - padding, width: bounds.width + padding * 2, height: bounds.height + padding * 2 },
      { width: viewport.clientWidth, height: viewport.clientHeight },
      { top: 112, right: 96, bottom: 104, left: 96 },
      2.5
    ));
  }

  function focusWorkspaceArtboard(artboard: WorkspaceArtboardPlacement) {
    activateWorkspaceArtboard(artboard);
    fitWorkspaceArtboard(artboard);
  }

  function focusWorkspaceArtboardByPageId(targetPageId: string) {
    const target = workspacePlacement?.artboards.find((artboard) => artboard.pageId === targetPageId);
    if (target) focusWorkspaceArtboard(target);
  }

  function updateBreakpoint(width: number, height: number, previewSelection?: ViewportSelection, reflow = false) {
    const safeWidth = Math.min(10000, Math.max(320, Math.round(width)));
    const safeHeight = Math.min(30000, Math.max(320, Math.round(height)));
    commitWithCanvasGrowth((current) => {
      const previousWidth = breakpointFor(current, device).width;
      const reflowed = reflow && previousWidth !== safeWidth
        ? pagesForDocument(current).reduce(
          (next, page) => reflowPageForViewport(next, page.id, device, previousWidth, safeWidth),
          current
        )
        : current;
      const breakpoints = {
        desktop: { ...(reflowed.breakpoints?.desktop ?? { width: reflowed.viewport.width, height: reflowed.viewport.height }) },
        tablet: { ...(reflowed.breakpoints?.tablet ?? { width: 768, height: 1100 }) },
        mobile: { ...(reflowed.breakpoints?.mobile ?? { width: 390, height: 844 }) }
      };
      breakpoints[device] = {
        ...breakpoints[device],
        width: safeWidth,
        height: safeHeight,
        preview: previewSelection ? {
          presetId: previewSelection.presetId,
          orientation: previewSelection.orientation,
          viewportHeight: previewSelection.customHeight
        } : breakpoints[device].preview
      };
      return { ...reflowed, breakpoints, viewport: device === 'desktop' ? { ...reflowed.viewport, width: safeWidth, height: safeHeight } : reflowed.viewport };
    });
  }

  function selectViewportPreset(presetId: string) {
    const preset = viewportPresets.find((candidate) => candidate.id === presetId);
    if (!preset) return;
    const dimensions = viewportDimensions(preset, 'default');
    const selection: ViewportSelection = { presetId: preset.id, orientation: 'default', customHeight: dimensions.height };
    setViewportSelections((current) => ({
      ...current,
      [device]: selection
    }));
    updateActiveWorkspaceViewport(dimensions.width, dimensions.height);
    updateBreakpoint(dimensions.width, breakpoint.height, selection, true);
    window.setTimeout(() => fitCanvasToWidth(dimensions.width), 0);
  }

  function rotateViewport() {
    if (viewportPreset) {
      const orientation: WebDesignViewportOrientation = viewportSelection.orientation === 'default' ? 'rotated' : 'default';
      const dimensions = viewportDimensions(viewportPreset, orientation);
      const selection: ViewportSelection = { ...viewportSelection, orientation, customHeight: dimensions.height };
      setViewportSelections((current) => ({
        ...current,
        [device]: selection
      }));
      updateActiveWorkspaceViewport(dimensions.width, dimensions.height);
      updateBreakpoint(dimensions.width, breakpoint.height, selection, true);
      window.setTimeout(() => fitCanvasToWidth(dimensions.width), 0);
      return;
    }
    const nextWidth = previewViewportHeight;
    const nextViewportHeight = breakpoint.width;
    const selection: ViewportSelection = { presetId: undefined, orientation: 'default', customHeight: nextViewportHeight };
    setViewportSelections((current) => ({
      ...current,
      [device]: selection
    }));
    updateActiveWorkspaceViewport(nextWidth, nextViewportHeight);
    updateBreakpoint(nextWidth, breakpoint.height, selection, true);
    window.setTimeout(() => fitCanvasToWidth(nextWidth), 0);
  }

  function updateCustomViewportWidth(width: number) {
    const selection: ViewportSelection = { ...viewportSelection, presetId: undefined };
    setViewportSelections((current) => ({
      ...current,
      [device]: selection
    }));
    updateActiveWorkspaceViewport(width, previewViewportHeight);
    updateBreakpoint(width, breakpoint.height, selection, true);
  }

  function updateCustomViewportHeight(height: number) {
    const safeHeight = Math.min(30000, Math.max(320, Math.round(height)));
    const selection: ViewportSelection = { presetId: undefined, orientation: 'default', customHeight: safeHeight };
    setViewportSelections((current) => ({
      ...current,
      [device]: selection
    }));
    updateActiveWorkspaceViewport(breakpoint.width, safeHeight);
    updateBreakpoint(breakpoint.width, breakpoint.height, selection);
  }

  function switchDevice(next: WebDesignDevice) {
    setDevice(next);
    const current = documentRef.current;
    const responsive = current ? breakpointFor(current, next) : undefined;
    if (responsive) updateActiveWorkspaceViewport(responsive.width, workspaceViewportHeight(current!, next));
    setSelectedId(undefined);
    setSelectedIds([]);
  }

  function withGeneratedResponsiveLayouts(active: WebDesignDocument, targetPageId: string) {
    const desktopWidth = breakpointFor(active, 'desktop').width;
    const tabletWidth = breakpointFor(active, 'tablet').width;
    const mobileWidth = breakpointFor(active, 'mobile').width;
    const withTablet = deriveResponsivePageFromDevice(active, targetPageId, 'desktop', 'tablet', desktopWidth, tabletWidth);
    return deriveResponsivePageFromDevice(withTablet, targetPageId, 'desktop', 'mobile', desktopWidth, mobileWidth);
  }

  function generateResponsiveLayouts() {
    if (!documentRef.current) return;
    commitWithCanvasGrowth((active) => withGeneratedResponsiveLayouts(active, pageId), ['tablet', 'mobile']);
    showToast('已补齐平板和手机布局，已有人工调整保持不变');
  }

  function fitCanvasToWidth(targetWidth: number) {
    const viewport = canvasScroll.current;
    if (!viewport) return;
    const artboard = workspacePlacement?.artboards.find((candidate) => candidate.artboardId === activeArtboardId);
    setWorkspaceCamera(fitWorkspaceWidth(
      { x: artboard?.x ?? 0, y: artboard?.y ?? 0, width: targetWidth, height: renderedCanvasHeight },
      { width: viewport.clientWidth, height: viewport.clientHeight }
    ));
  }

  function fitCanvasWidth() {
    fitCanvasToWidth(breakpoint.width);
  }

  function setCanvasZoom(nextZoom: number, anchor?: { x: number; y: number }) {
    const viewport = canvasScroll.current;
    if (!viewport) {
      setWorkspaceCamera((current) => ({ ...current, zoom: nextZoom }));
      return;
    }
    setWorkspaceCamera((current) => zoomWorkspaceCameraAt(current, nextZoom, anchor ?? {
      x: viewport.clientWidth / 2,
      y: viewport.clientHeight / 2
    }));
  }

  function toggleFullPreview() {
    if (preview) {
      setPreview(false);
      setPreviewOverlayPageId(undefined);
      setWorkspaceCamera((current) => ({ ...current, zoom: previewZoom.current }));
      return;
    }
    const editorCamera = editingSlot ? workspaceCameraBeforeSlot.current ?? workspaceCamera : workspaceCamera;
    previewZoom.current = interactionMode ? interactionZoom.current : editorCamera.zoom;
    if (interactionMode) setInteractionMode(false);
    resetSlotEditorCamera();
    setEditingSlot(undefined);
    setWorkspaceCamera(editorCamera);
    setSelectedId(undefined);
    setSelectedIds([]);
    setPreview(true);
    window.setTimeout(() => {
      const scroller = canvasScroll.current;
      if (!scroller) return;
      setWorkspaceCamera((current) => ({ ...current, zoom: Math.max(.1, Math.min(8, scroller.clientWidth / breakpoint.width)) }));
      scroller.scrollTo({ left: 0, top: 0 });
    }, 0);
  }

  function toggleInteractionMode() {
    if (interactionMode) {
      setInteractionMode(false);
      setWorkspaceCamera((current) => ({ ...current, zoom: interactionZoom.current }));
      return;
    }
    interactionZoom.current = zoom;
    setSelectedId(undefined);
    setSelectedIds([]);
    setInteractionMode(true);
    window.setTimeout(() => fitCanvasWidth(), 0);
  }

  function selectComponent(componentId: string, additive = false) {
    const current = documentRef.current;
    const component = current?.components.find((candidate) => candidate.id === componentId);
    const container = current && component ? contentContainerAncestor(current, component) : undefined;
    const slotId = current && component && container ? slotIdForDescendant(current, component, container.id) : undefined;
    if (container && slotId && (editingSlot?.componentId !== container.id || editingSlot.slotId !== slotId)) {
      enterSlotEditor({ componentId: container.id, slotId });
    }
    if (!additive) {
      setSelectedId(componentId);
      setSelectedIds([componentId]);
      return;
    }
    const next = selectedIds.includes(componentId) ? selectedIds.filter((id) => id !== componentId) : [...selectedIds, componentId];
    setSelectedIds(next);
    setSelectedId(next.includes(componentId) ? componentId : next[0]);
  }

  function selectableNodesForCurrentEditor(current: WebDesignDocument): EditorSelectableNode[] {
    if (editingSlot) {
      const container = current.components.find((component) => component.id === editingSlot.componentId);
      if (!container) return [];
      const containerFrame = resolveComponent(container, device);
      return visibleComponentsInSlot(current, editingSlot.componentId, editingSlot.slotId).map((component) => {
        const frame = resolveComponent(component, device);
        return {
          id: component.id,
          name: component.name,
          type: component.library?.component ?? component.type,
          parentId: component.parentId === container.id ? undefined : component.parentId,
          zIndex: component.zIndex,
          locked: component.locked,
          visible: !frame.hidden,
          rect: { x: frame.x - containerFrame.x, y: frame.y - containerFrame.y, width: frame.width, height: frame.height }
        };
      });
    }
    return componentsForPage(current, pageId)
      .filter((component) => !contentContainerAncestor(current, component))
      .map((component) => {
        const frame = resolveComponent(component, device);
        return {
          id: component.id,
          name: component.name,
          type: component.library?.component ?? component.type,
          parentId: component.parentId,
          zIndex: component.zIndex,
          locked: component.locked,
          visible: !frame.hidden,
          rect: { x: frame.x, y: frame.y, width: frame.width, height: frame.height }
        };
      });
  }

  function selectionOverlayItemsFor(
    components: readonly WebDesignComponent[],
    targetDevice: WebDesignDevice,
    origin: { x: number; y: number } = { x: 0, y: 0 }
  ): SelectionOverlayItem[] {
    const byId = new Map(components.map((component) => [component.id, component]));
    return selectedIds.flatMap((id) => {
      const component = byId.get(id);
      if (!component) return [];
      const frame = resolveComponent(component, targetDevice);
      if (frame.hidden) return [];
      return [{
        id: component.id,
        name: component.name,
        locked: Boolean(component.locked),
        primary: component.id === selectedId,
        rect: { x: frame.x - origin.x, y: frame.y - origin.y, width: frame.width, height: frame.height }
      }];
    });
  }

  function selectSelectionChild() {
    if (sceneEditingActive && selectedSceneNode) {
      const children = isSceneContainer(selectedSceneNode)
        ? selectedSceneNode.children
        : isSceneSlotContainer(selectedSceneNode)
          ? Object.values(selectedSceneNode.slots).flat()
          : [];
      const child = children.at(-1);
      if (child) {
        setSelectedId(child.id);
        setSelectedIds([child.id]);
      } else {
        showToast('当前 Scene 图层没有可进入的子层');
      }
      return;
    }
    const current = documentRef.current;
    if (!current || !selectedId) return;
    const selectedComponent = current.components.find((component) => component.id === selectedId);
    const editableSlot = selectedComponent ? editableSlotsForUiComponent(selectedComponent)[0] : undefined;
    if (selectedComponent && editableSlot) {
      void editComponentSlot(selectedComponent, editableSlot.id);
      return;
    }
    const child = deepestSelectionChild(selectableNodesForCurrentEditor(current), selectedId);
    if (child) selectComponent(child.id);
    else showToast('当前图层没有可进入的子层');
  }

  function selectSelectionParent() {
    if (sceneEditingActive && selectedId && sceneDocument) {
      const entry = indexSceneDocument(sceneDocument).get(selectedId);
      if (!entry || sceneDocument.pages.some((page) => page.id === entry.parentId)) {
        showToast('当前已经是画板最外层');
        return;
      }
      setSelectedId(entry.parentId);
      setSelectedIds([entry.parentId]);
      return;
    }
    const current = documentRef.current;
    if (!current || !selectedId) return;
    const component = current.components.find((candidate) => candidate.id === selectedId);
    if (!component?.parentId) {
      showToast('当前已经是最外层');
      return;
    }
    if (editingSlot && component.parentId === editingSlot.componentId) {
      exitSlotEditor();
      return;
    }
    selectComponent(component.parentId);
  }

  function sceneSelectionRootIds(): string[] {
    const scene = sceneDocumentRef.current;
    if (!scene) return [];
    const index = indexSceneDocument(scene);
    const selectedSet = new Set(selectedIds);
    return selectedIds.filter((id) => {
      let parentId = index.get(id)?.parentId;
      while (parentId && index.has(parentId)) {
        if (selectedSet.has(parentId)) return false;
        parentId = index.get(parentId)?.parentId;
      }
      return index.get(id)?.pageId === pageId;
    });
  }

  function cloneSceneSubtree(source: SceneNode, offsetX = 20, offsetY = 20): SceneNode {
    const clone = structuredClone(source);
    const idMap = new Map<string, string>();
    const collect = (node: SceneNode) => {
      idMap.set(node.id, `${node.type}:${crypto.randomUUID()}`);
      if (isSceneContainer(node)) node.children.forEach(collect);
      if (isSceneSlotContainer(node)) Object.values(node.slots).flat().forEach(collect);
    };
    const rewrite = (node: SceneNode, root: boolean) => {
      node.id = idMap.get(node.id)!;
      node.name = root ? `${node.name} 副本` : node.name;
      node.frame = { ...node.frame, ...(root ? { x: node.frame.x + offsetX, y: node.frame.y + offsetY } : {}) };
      node.annotations = [];
      node.createdBy = 'human';
      node.updatedBy = 'human';
      if (node.type === 'component-instance' && idMap.has(node.mainComponentId)) node.mainComponentId = idMap.get(node.mainComponentId)!;
      if (isSceneContainer(node)) node.children.forEach((child) => rewrite(child, false));
      if (isSceneSlotContainer(node)) Object.values(node.slots).flat().forEach((child) => rewrite(child, false));
    };
    collect(clone);
    rewrite(clone, true);
    return clone;
  }

  async function insertSceneCopies(nodes: readonly SceneNode[], targetPageId = pageId, preserveParent = false) {
    let scene = sceneDocumentRef.current;
    if (!scene || nodes.length === 0) return;
    const insertedRootIds: string[] = [];
    for (const source of nodes) {
      scene = sceneDocumentRef.current;
      if (!scene) return;
      const sourceEntry = indexSceneDocument(scene).get(source.id);
      const sourceParent = sourceEntry ? indexSceneDocument(scene).get(sourceEntry.parentId)?.node : undefined;
      const sourceSlot = sourceParent && isSceneSlotContainer(sourceParent)
        ? Object.entries(sourceParent.slots).find(([, children]) => children.some((child) => child.id === source.id))?.[0]
        : undefined;
      const target = preserveParent && sourceEntry
        ? { parentId: sourceEntry.parentId, index: Number.MAX_SAFE_INTEGER, slot: sourceSlot }
        : (() => {
          const { root } = scenePageRoot(scene!, targetPageId);
          return { parentId: root.id, index: root.children.length, slot: undefined };
        })();
      const parentEntry = indexSceneDocument(scene).get(target.parentId)?.node;
      const parentPage = scene.pages.find((candidate) => candidate.id === target.parentId);
      const childCount = parentPage?.children.length
        ?? (parentEntry && isSceneContainer(parentEntry)
          ? parentEntry.children.length
          : parentEntry && isSceneSlotContainer(parentEntry) && target.slot
            ? parentEntry.slots[target.slot]?.length ?? 0
            : 0);
      const copy = cloneSceneSubtree(source);
      await commitSceneCommand({ type: 'insert-node', parentId: target.parentId, index: Math.min(target.index, childCount), slot: target.slot, node: copy }, '用户复制 Scene 图层。');
      insertedRootIds.push(copy.id);
    }
    setSelectedId(insertedRootIds[0]);
    setSelectedIds(insertedRootIds);
    showToast(`已复制 ${insertedRootIds.length} 个 Scene 图层`);
  }

  function copySceneSelection() {
    const scene = sceneDocumentRef.current;
    if (!scene) return;
    const index = indexSceneDocument(scene);
    const nodes = sceneSelectionRootIds().flatMap((id) => {
      const node = index.get(id)?.node;
      return node ? [structuredClone(node)] : [];
    });
    setSceneClipboard(nodes);
    showToast(`已复制 ${nodes.length} 个 Scene 图层`);
  }

  function duplicateSceneSelection() {
    const scene = sceneDocumentRef.current;
    if (!scene) return;
    const index = indexSceneDocument(scene);
    const nodes = sceneSelectionRootIds().flatMap((id) => {
      const node = index.get(id)?.node;
      return node ? [structuredClone(node)] : [];
    });
    void insertSceneCopies(nodes, pageId, true).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
  }

  function pasteSceneClipboard() {
    void insertSceneCopies(sceneClipboard, pageId, false).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
  }

  async function wrapSceneSelection(kind: 'group' | 'frame' | 'auto-horizontal' | 'auto-vertical') {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length < 2) {
      showToast('请选择同一容器中的至少两个图层');
      return;
    }
    const wrapperId = `${kind.startsWith('auto') ? 'frame' : kind}:${crypto.randomUUID()}`;
    const command: SceneEditorCommand = kind === 'group'
      ? { type: 'group', nodeIds, wrapperId, name: `分组 · ${nodeIds.length} 项` }
      : kind === 'frame'
        ? { type: 'frame', nodeIds, wrapperId, name: `Frame · ${nodeIds.length} 项`, padding: 16 }
        : {
          type: 'auto-layout-frame',
          nodeIds,
          wrapperId,
          name: kind === 'auto-horizontal' ? '横向 Auto Layout' : '纵向 Auto Layout',
          direction: kind === 'auto-horizontal' ? 'horizontal' : 'vertical',
          padding: 16,
          gap: 16,
          sizingX: 'hug',
          sizingY: 'hug'
        };
    try {
      await commitSceneCommand(command, `在画板中创建 ${command.type}。`);
      setSelectedId(wrapperId);
      setSelectedIds([wrapperId]);
      showToast(command.type === 'group' ? '已创建 Group' : command.type === 'frame' ? '已创建 Frame' : '已创建 Auto Layout');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function ungroupSceneSelection() {
    if (!selectedSceneNode || (selectedSceneNode.type !== 'group' && selectedSceneNode.type !== 'frame') || selectedSceneNode.layout.mode !== 'free') return;
    const childIds = selectedSceneNode.children.map((child) => child.id);
    try {
      await commitSceneCommand({ type: 'ungroup', wrapperId: selectedSceneNode.id }, '用户取消 Scene 分组。');
      setSelectedId(childIds[0]);
      setSelectedIds(childIds);
      showToast('已取消分组');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function nudgeSceneSelection(deltaX: number, deltaY: number) {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length === 0) return;
    try {
      await commitSceneCommand({ type: 'move', nodeIds, deltaX, deltaY }, '用户使用键盘微调 Scene 图层。');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function alignSceneSelection(alignment: 'left' | 'horizontal-center' | 'right' | 'top' | 'vertical-center' | 'bottom') {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length < 2) return;
    try {
      await commitSceneCommand({ type: 'align', nodeIds, alignment }, '用户对齐 Scene 图层。');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function distributeSceneSelection(axis: 'horizontal' | 'vertical') {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length < 3) return;
    try {
      await commitSceneCommand({ type: 'distribute', nodeIds, axis }, '用户等间距分布 Scene 图层。');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function reorderSceneSelection(placement: 'front' | 'forward' | 'backward' | 'back') {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length === 0) return;
    try {
      await commitSceneCommand({ type: 'reorder', nodeIds, placement }, '用户调整 Scene 图层顺序。');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function deleteSceneSelection() {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length === 0) return;
    try {
      await commitSceneCommand({ type: 'delete-nodes', nodeIds }, '用户从画板删除 Scene 图层。');
      setSelectedId(undefined);
      setSelectedIds([]);
      showToast(`已删除 ${nodeIds.length} 个图层`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function updateSceneNodeById(nodeId: string, patches: Array<{ path: string[]; value: unknown }>, reason = '用户在属性栏调整 Scene 图层。') {
    try {
      await commitSceneCommand({ type: 'update-node', nodeId, patches }, reason);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function updateSceneNode(patches: Array<{ path: string[]; value: unknown }>, reason = '用户在属性栏调整 Scene 图层。') {
    if (!selectedSceneNode) return;
    await updateSceneNodeById(selectedSceneNode.id, patches, reason);
  }

  async function applySelectedSceneLibraryVariant(variantId: string) {
    if (selectedSceneNode?.type !== 'library-instance' || !selectedSceneLibrary || !selectedSceneLibraryDefinition) return;
    const variant = selectedSceneLibraryVariants.find((candidate) => candidate.id === variantId);
    if (!variant) return;
    const variantKeys = new Set(selectedSceneLibraryVariants.flatMap((candidate) => Object.keys(candidate.props)));
    const customProperties = Object.fromEntries(Object.entries(selectedSceneNode.properties).filter(([key]) => !variantKeys.has(key)));
    const patches: Array<{ path: string[]; value: unknown }> = [
      { path: ['variant'], value: variant.id },
      { path: ['properties'], value: { ...(selectedSceneLibraryDefinition.props ?? {}), ...customProperties, ...variant.props } }
    ];
    if (variant.content !== undefined) patches.push({ path: ['content'], value: variant.content });
    if (variant.width !== undefined) patches.push({ path: ['frame', 'width'], value: variant.width });
    if (variant.height !== undefined) patches.push({ path: ['frame', 'height'], value: variant.height });
    await updateSceneNode(patches, `用户切换 ${selectedSceneLibrary.displayName} ${selectedSceneLibraryDefinition.label} 的官方款式。`);
  }

  function focusSceneContent(nodeId: string, slot?: string) {
    if (!selectedSceneEntry) return;
    setSceneContentFocus({ pageId: selectedSceneEntry.pageId, nodeId, ...(slot ? { slot } : {}) });
    showToast(slot ? '已进入内容区；现在拖入的组件会直接放到这里' : '已进入容器；现在拖入的组件会直接放到这里');
  }

  async function updateSelectedSceneResponsiveOverride(changes: Omit<SceneResponsiveNodeOverride, 'nodeId'>) {
    if (!selectedSceneNode || !sceneResponsiveRuleSpec) return;
    const current = selectedSceneResponsiveOverride ?? {};
    const next = { ...structuredClone(current), ...structuredClone(changes) };
    await commitSceneCommand({
      type: 'set-responsive-override',
      ...sceneResponsiveRuleSpec,
      nodeId: selectedSceneNode.id,
      override: next
    }, `用户调整 ${device} Scene 响应式布局。`);
  }

  async function replaceSelectedSceneResponsiveOverride(next: Omit<SceneResponsiveNodeOverride, 'nodeId'>) {
    if (!selectedSceneNode || !sceneResponsiveRuleSpec) return;
    if (next.visible === undefined && next.layout === undefined && next.childOrder === undefined) {
      await clearSelectedSceneResponsiveOverride();
      return;
    }
    await commitSceneCommand({
      type: 'set-responsive-override',
      ...sceneResponsiveRuleSpec,
      nodeId: selectedSceneNode.id,
      override: next
    }, `用户调整 ${device} Scene 响应式布局。`);
  }

  async function clearSelectedSceneResponsiveOverride() {
    if (!selectedSceneNode || !sceneResponsiveRuleSpec || !selectedSceneResponsiveOverride) return;
    await commitSceneCommand({
      type: 'clear-responsive-override', ruleId: sceneResponsiveRuleSpec.ruleId, nodeId: selectedSceneNode.id
    }, `用户恢复 ${device} Scene 响应式继承。`);
  }

  function groupSelected() {
    if (sceneEditingActive) {
      void wrapSceneSelection('group');
      return;
    }
    const current = documentRef.current;
    if (!current || selectedIds.length < 2) return;
    const roots = selectedRootIds(current, selectedIds);
    if (roots.length < 2) return;
    const components = roots.map((id) => current.components.find((component) => component.id === id)).filter(Boolean) as WebDesignComponent[];
    const padding = 20;
    const boundsFor = (target: WebDesignDevice) => {
      const frames = components.map((component) => resolveComponent(component, target));
      const x = Math.min(...frames.map((frame) => frame.x)) - padding;
      const y = Math.min(...frames.map((frame) => frame.y)) - padding;
      const right = Math.max(...frames.map((frame) => frame.x + frame.width)) + padding;
      const bottom = Math.max(...frames.map((frame) => frame.y + frame.height)) + padding;
      return { x, y, width: right - x, height: bottom - y };
    };
    const desktopBounds = boundsFor('desktop');
    const tabletBounds = boundsFor('tablet');
    const mobileBounds = boundsFor('mobile');
    const parentIds = new Set(components.map((component) => component.parentId));
    const slotIds = new Set(components.map((component) => component.slot));
    const group = componentDefaults('section', desktopBounds.x, desktopBounds.y);
    group.id = `group-${crypto.randomUUID().slice(0, 8)}`;
    group.name = `新建分组 · ${roots.length} 项`;
    group.pageId = pageId;
    group.width = desktopBounds.width;
    group.height = desktopBounds.height;
    group.parentId = parentIds.size === 1 ? components[0].parentId : undefined;
    group.slot = slotIds.size === 1 ? components[0].slot : undefined;
    group.zIndex = Math.max(0, Math.min(...components.map((component) => component.zIndex)) - 1);
    group.layout = { mode: 'free', gap: 16, padding, align: 'start' };
    group.responsive = { tablet: tabletBounds, mobile: mobileBounds };
    commitWithCanvasGrowth((active) => ({
      ...active,
      components: [...active.components.map((component) => roots.includes(component.id) ? { ...component, parentId: group.id } : component), group]
    }));
    setSelectedId(group.id);
    setSelectedIds([group.id]);
    showToast('已创建分组，拖动分组外框即可整体移动');
  }

  function ungroupSelected() {
    if (sceneEditingActive) {
      void ungroupSceneSelection();
      return;
    }
    const current = documentRef.current;
    if (!current || !selected) return;
    const children = current.components.filter((component) => component.parentId === selected.id);
    if (children.length === 0) return;
    const childIds = children.map((component) => component.id);
    commit((active) => ({
      ...active,
      components: active.components
        .filter((component) => component.id !== selected.id)
        .map((component) => component.parentId === selected.id ? { ...component, parentId: selected.parentId, slot: selected.slot } : component),
      requests: active.requests.filter((request) => request.componentId !== selected.id)
    }));
    setSelectedId(childIds[0]);
    setSelectedIds(childIds);
    showToast('已取消分组，内部组件保持在原位置');
  }

  function updateSelectedLayout(changes: Partial<NonNullable<WebDesignComponent['layout']>>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride({
      ...component,
      layout: { mode: 'free', gap: 16, padding: 16, align: 'start', justify: 'start', wrap: false, ...component.layout, ...changes }
    }, 'frame', true));
  }

  function applySelectedAutoLayout() {
    if (!selected) return;
    commitWithCanvasGrowth((current) => {
      const changedIds = new Set(current.components.filter((component) => component.parentId === selected.id)
        .flatMap((component) => [component.id, ...descendantIds(current, component.id)]));
      const laidOut = autoLayoutContainer(current, selected.id, device);
      return { ...laidOut, components: laidOut.components.map((component) => changedIds.has(component.id) ? setSymbolOverride(component, 'frame', true) : component) };
    });
  }

  function saveSelectionAsSymbol() {
    const current = documentRef.current;
    if (!current || selectedIds.length === 0) return;
    const defaultName = selectedIds.length > 1 ? `${selected?.name ?? '组合'} · ${selectedIds.length} 层` : selected?.name ?? '我的组件';
    const name = window.prompt('给这个组合起个名字', defaultName)?.trim();
    if (!name) return;
    const symbol = createSymbolFromSelection(current, selectedIds, name);
    commit((active) => ({ ...active, symbols: [...(active.symbols ?? []), symbol] }));
    setPersonalSymbols((symbols) => [...symbols.filter((candidate) => candidate.id !== symbol.id), structuredClone(symbol)]);
    chooseLibraryTab('my');
    showToast(`已保存到“我的”：${symbol.name}`);
  }

  function insertSymbol(symbol: WebDesignSymbol) {
    const current = documentRef.current;
    if (!current) return;
    const instance = instantiateSymbol(current, symbol, pageId);
    commitWithCanvasGrowth(
      (active) => ({
        ...active,
        symbols: (active.symbols ?? []).some((candidate) => candidate.id === symbol.id)
          ? (active.symbols ?? []).map((candidate) => candidate.id === symbol.id ? structuredClone(symbol) : candidate)
          : [...(active.symbols ?? []), structuredClone(symbol)],
        components: [...active.components, ...instance.components]
      }),
      ['desktop', 'tablet', 'mobile']
    );
    setSelectedId(instance.rootIds[0]);
    setSelectedIds(instance.rootIds);
    const instanceTop = Math.min(...instance.components.map((component) => resolveComponent(component, device).y));
    if (instanceTop > breakpointFor(current, device).height * .65) {
      window.setTimeout(() => {
        const scroller = window.document.querySelector('.canvas-scroll');
        scroller?.scrollTo({ top: scroller.scrollHeight, behavior: 'smooth' });
      }, 0);
    }
    showToast(`已插入 ${symbol.name}`);
  }

  function renamePersonalSymbol(symbol: WebDesignSymbol) {
    const name = window.prompt('重命名我的组件', symbol.name)?.trim();
    if (!name || name === symbol.name) return;
    setPersonalSymbols((symbols) => symbols.map((candidate) => candidate.id === symbol.id ? { ...candidate, name } : candidate));
    commit((current) => ({ ...current, symbols: current.symbols?.map((candidate) => candidate.id === symbol.id ? { ...candidate, name } : candidate) }));
  }

  function removePersonalSymbol(symbolId: string) {
    if (!window.confirm('从“我的”中移除这个组件？已放入画布的内容不会受影响。')) return;
    setPersonalSymbols((symbols) => symbols.filter((symbol) => symbol.id !== symbolId));
    showToast('已从“我的”移除，画布中的实例保持不变');
  }

  function saveSceneSelectionAsSnippet() {
    const scene = sceneDocumentRef.current;
    if (!scene || selectedIds.length === 0) return;
    const defaultName = selectedIds.length === 1
      ? indexSceneDocument(scene).get(selectedIds[0])?.node.name ?? '我的组件'
      : `设计组合 · ${selectedIds.length} 层`;
    const name = window.prompt('给这个可复用设计组合起个名字', defaultName)?.trim();
    if (!name) return;
    try {
      const snippet = createSceneSnippet(scene, selectedIds, name);
      setSceneSnippets((items) => [...items.filter((item) => item.id !== snippet.id), snippet]);
      chooseLibraryTab('my');
      showToast(`已保存到“我的”：${snippet.name}`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function insertSceneSnippet(snippet: SceneSnippet) {
    const scene = sceneDocumentRef.current;
    if (!scene) return;
    try {
      const artboard = workspacePlacement?.artboards.find((candidate) => candidate.artboardId === activeArtboardId)
        ?? workspacePlacement?.artboards.find((candidate) => candidate.pageId === pageId);
      const targetPageId = artboard?.pageId ?? pageId;
      const viewportWidth = artboard?.viewportWidth ?? breakpoint.width;
      const target = resolveSceneInsertionTarget({
        document: scene,
        pageId: targetPageId,
        viewportWidth,
        point: { x: 72, y: 72 },
        preferred: sceneContentFocus?.pageId === targetPageId ? sceneContentFocus : undefined
      });
      const node = instantiateSceneSnippet(snippet, target.x, target.y);
      await commitSceneCommand({ type: 'insert-node', parentId: target.nodeId, slot: target.slot, index: target.index, node }, `用户插入“我的”设计组合 ${snippet.name}。`);
      setSelectedId(node.id);
      setSelectedIds([node.id]);
      showToast(`已插入 ${snippet.name}`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function renameSceneSnippet(snippet: SceneSnippet) {
    const name = window.prompt('重命名设计组合', snippet.name)?.trim();
    if (!name || name === snippet.name) return;
    setSceneSnippets((items) => items.map((item) => item.id === snippet.id ? { ...item, name, updatedAt: new Date().toISOString() } : item));
  }

  function removeSceneSnippet(snippetId: string) {
    if (!window.confirm('从“我的”中移除这个设计组合？已经插入画布的内容不会受影响。')) return;
    setSceneSnippets((items) => items.filter((item) => item.id !== snippetId));
    showToast('已从“我的”移除，画布中的内容保持不变');
  }

  async function applySceneVariablesDraft() {
    if (!sceneDocumentRef.current) return;
    try {
      const collections = JSON.parse(sceneVariablesDraft) as SceneVariableCollection[];
      if (!Array.isArray(collections)) throw new Error('变量数据必须是数组。');
      await commitSceneCommand({ type: 'set-variable-collections', collections }, '用户更新 Scene 变量与模式。');
      showToast('Scene 变量已保存');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function seedSceneVariables() {
    const modeId = 'mode:default';
    setSceneVariablesDraft(JSON.stringify([{
      id: 'variables:visual-system',
      name: '视觉系统',
      modes: [{ id: modeId, name: '默认' }],
      variables: [
        { id: 'variable:color-primary', name: '主色', type: 'color', valuesByMode: { [modeId]: '#0A84FF' } },
        { id: 'variable:color-surface', name: '表面', type: 'color', valuesByMode: { [modeId]: '#FFFFFF' } },
        { id: 'variable:color-text', name: '正文', type: 'color', valuesByMode: { [modeId]: '#1D1D1F' } },
        { id: 'variable:spacing-base', name: '基础间距', type: 'number', valuesByMode: { [modeId]: 8 } },
        { id: 'variable:radius-card', name: '卡片圆角', type: 'number', valuesByMode: { [modeId]: 20 } },
        { id: 'variable:font-family', name: '字体', type: 'string', valuesByMode: { [modeId]: '-apple-system, BlinkMacSystemFont, sans-serif' } }
      ]
    } satisfies SceneVariableCollection], null, 2));
  }

  function toggleSelectedSymbolOverride(override: WebSymbolOverride) {
    if (!selected) return;
    const enabled = !(selected.symbolOverrides ?? []).includes(override);
    updateComponent(selected.id, (component) => setSymbolOverride(component, override, enabled));
  }

  function synchronizeSelectedSymbol() {
    if (!selected?.symbolId) return;
    commit((current) => syncSymbolInstances(current, selected.symbolId!));
    showToast('已同步全部组件实例');
  }

  function updateSelectedSymbolDefinition() {
    if (!selected?.symbolInstanceId) return;
    commit((current) => updateSymbolFromInstance(current, selected.id));
    showToast('已更新组件定义并同步其他实例');
  }

  function updateSelectedLibraryProp(key: string, value: WebDesignJsonValue) {
    if (!selected?.library) return;
    updateSelected({ library: { ...selected.library, props: { ...selected.library.props, [key]: value } } });
  }

  function applySelectedLibraryVariant(variantId: string) {
    if (!selected?.library) return;
    if (selected.library.props.editorDetachedContent !== true) {
      updateComponent(selected.id, (component) => applyUiLibraryVariant(component, variantId));
      return;
    }
    if (!window.confirm('切换官方款式会替换当前已经拆分的内部设计，是否继续？')) return;
    commit((current) => {
      const slotRoots = current.components.filter((component) => component.parentId === selected.id && component.slot);
      const removed = new Set(slotRoots.flatMap((component) => [component.id, ...descendantIds(current, component.id)]));
      return {
        ...current,
        components: current.components
          .filter((component) => !removed.has(component.id))
          .map((component) => {
            if (component.id !== selected.id || !component.library) return component;
            const props = { ...component.library.props };
            delete props.editorDetachedContent;
            return applyUiLibraryVariant({ ...component, library: { ...component.library, props } }, variantId);
          })
      };
    });
    setSelectedId(selected.id);
    setSelectedIds([selected.id]);
    showToast('已切换官方款式；再次进入“内部内容”即可拆分编辑');
  }

  function detachSelectedSymbol() {
    if (!selected?.symbolInstanceId) return;
    commit((current) => detachSymbolInstance(current, selected.id));
    showToast('当前实例已脱离组件库');
  }

  function updateTokens(updater: (tokens: WebDesignTokens) => WebDesignTokens) {
    commit((current) => ({ ...current, tokens: updater(structuredClone(tokensForDocument(current))) }));
  }

  async function applyDesignTheme(preset: WebDesignThemePreset) {
    const scene = sceneDocumentRef.current;
    if (!scene) {
      showToast('请先让 AI 建立 Scene，再应用视觉变量');
      return;
    }
    const modeId = 'theme:default';
    const collection: SceneVariableCollection = {
      id: 'theme:visual-system',
      name: `${preset.name} 视觉系统`,
      modes: [{ id: modeId, name: 'Default' }],
      variables: [
        ...Object.entries({ canvas: preset.canvasBackground, ...preset.tokens.colors }).map(([key, value]) => ({ id: `theme:color:${key}`, name: `Color / ${key}`, type: 'color' as const, valuesByMode: { [modeId]: value } })),
        ...Object.entries(preset.tokens.radii).map(([key, value]) => ({ id: `theme:radius:${key}`, name: `Radius / ${key}`, type: 'number' as const, valuesByMode: { [modeId]: value } })),
        { id: 'theme:font:family', name: 'Typography / font family', type: 'string', valuesByMode: { [modeId]: preset.tokens.typography.fontFamily } },
        { id: 'theme:font:base-size', name: 'Typography / base size', type: 'number', valuesByMode: { [modeId]: preset.tokens.typography.baseFontSize } }
      ]
    };
    try {
      await commitSceneCommand({
        type: 'set-variable-collections',
        collections: [...scene.variableCollections.filter((item) => item.id !== collection.id), collection]
      }, `用户从 ${preset.name} 建立 Scene 视觉变量。`);
      setThemePickerOpen(false);
      showToast(`已建立 ${preset.name} Scene 视觉变量`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function updateTokenColor(key: keyof WebDesignTokens['colors'], value: string) {
    updateTokens((current) => ({ ...current, colors: { ...current.colors, [key]: value } }));
  }

  function applyColorToken(property: 'background' | 'color', token: keyof WebDesignTokens['colors']) {
    updateSelectedStyle({ [property]: `var(--color-${token})` });
  }

  function applyRadiusToken(token: keyof WebDesignTokens['radii']) {
    updateSelectedStyle({ borderRadius: tokens?.radii[token] ?? 0 });
  }

  function switchPage(nextPageId: string) {
    setPreviewOverlayPageId(undefined);
    const artboard = workspacePlacement?.artboards.find((candidate) => candidate.pageId === nextPageId);
    if (artboard) activateWorkspaceArtboard(artboard);
    else setPageId(nextPageId);
    resetSlotEditorCamera();
    setEditingSlot(undefined);
    setSelectedId(undefined);
    setSelectedIds([]);
  }

  function addPage() {
    void addWorkspaceSurface('page');
  }

  async function duplicateScenePage() {
    const scene = sceneDocumentRef.current;
    const sourcePage = scene?.pages.find((page) => page.id === pageId);
    if (!scene || !sourcePage) return;
    const newPageId = `page:${crypto.randomUUID()}`;
    const name = `${sourcePage.name} 副本`;
    try {
      await commitSceneCommand({ type: 'duplicate-page', pageId: sourcePage.id, newPageId, name }, '用户复制完整 Scene 画板。');
      const sourceArtboard = workspacePlacement?.artboards.find((candidate) => candidate.pageId === sourcePage.id);
      const x = workspacePlacement?.artboards.length
        ? Math.max(...workspacePlacement.artboards.map((artboard) => artboard.x + artboard.viewportWidth)) + WORKSPACE_ARTBOARD_GAP
        : 0;
      const artboard: WorkspaceArtboardPlacement = {
        artboardId: `artboard-${crypto.randomUUID().slice(0, 8)}`,
        pageId: newPageId,
        surfaceKind: sourceArtboard?.surfaceKind ?? 'page',
        viewportWidth: sourceArtboard?.viewportWidth ?? breakpoint.width,
        viewportHeight: sourceArtboard?.viewportHeight ?? previewViewportHeight,
        x,
        y: sourceArtboard?.y ?? 0
      };
      setWorkspacePlacement((current) => current ? { ...current, artboards: [...current.artboards, artboard] } : current);
      activateWorkspaceArtboard(artboard);
      setSelectedId(undefined);
      setSelectedIds([]);
      showToast(`已复制画板“${sourcePage.name}”，响应式规则保持一致`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function duplicatePage() {
    if (sceneDocumentRef.current) {
      void duplicateScenePage();
      return;
    }
    const current = documentRef.current;
    if (!current || !currentPage) return;
    const id = `page-${crypto.randomUUID().slice(0, 8)}`;
    const page = { id, name: `${currentPage.name} 副本`, slug: `/page-${pagesForDocument(current).length + 1}`, surfaceKind: currentPage.surfaceKind ?? 'page' };
    const sourceIds = componentsForPage(current, currentPage.id).map((component) => component.id);
    const cloned = cloneComponentSubtrees(current, sourceIds, id, 0, current);
    commit((active) => ({ ...active, pages: [...pagesForDocument(active), page], components: [...active.components, ...cloned.components] }));
    const sourceArtboard = workspacePlacement?.artboards.find((candidate) => candidate.pageId === currentPage.id);
    if (workspacePlacement) {
      const right = workspacePlacement.artboards.length === 0
        ? 0
        : Math.max(...workspacePlacement.artboards.map((artboard) => artboard.x + artboard.viewportWidth)) + WORKSPACE_ARTBOARD_GAP;
      const duplicateArtboard: WorkspaceArtboardPlacement = {
        artboardId: `artboard-${crypto.randomUUID().slice(0, 8)}`,
        pageId: id,
        surfaceKind: sourceArtboard?.surfaceKind ?? 'page',
        viewportWidth: sourceArtboard?.viewportWidth ?? breakpoint.width,
        viewportHeight: sourceArtboard?.viewportHeight ?? previewViewportHeight,
        x: right,
        y: sourceArtboard?.y ?? 0
      };
      setWorkspacePlacement({ ...workspacePlacement, artboards: [...workspacePlacement.artboards, duplicateArtboard] });
      setActiveArtboardId(duplicateArtboard.artboardId);
    }
    setPageId(id);
  }

  function deleteCurrentPage() {
    if (sceneDocumentRef.current) {
      const scene = sceneDocumentRef.current;
      const page = scene.pages.find((candidate) => candidate.id === pageId);
      if (!page || scene.pages.length <= 1) return;
      if (!window.confirm(`确定删除画板“${page.name}”及其全部 Scene 图层吗？此操作可撤销。`)) return;
      const nextPage = scene.pages.find((candidate) => candidate.id !== page.id);
      void commitSceneCommand({ type: 'delete-page', pageId: page.id }, '用户删除完整 Scene 画板。').then(() => {
        const remaining = workspacePlacement?.artboards.filter((artboard) => artboard.pageId !== page.id) ?? [];
        setWorkspacePlacement((current) => current ? { ...current, artboards: current.artboards.filter((artboard) => artboard.pageId !== page.id) } : current);
        const nextArtboard = remaining.find((artboard) => artboard.pageId === nextPage?.id) ?? remaining[0];
        if (nextArtboard) activateWorkspaceArtboard(nextArtboard);
        else if (nextPage) setPageId(nextPage.id);
        setSelectedId(undefined);
        setSelectedIds([]);
        showToast(`已删除画板“${page.name}”`);
      }).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
      return;
    }
    const current = documentRef.current;
    if (!current || !currentPage || pagesForDocument(current).length <= 1) return;
    if (!window.confirm(`确定删除页面“${currentPage.name}”及其全部组件吗？`)) return;
    const removedIds = new Set(componentsForPage(current, currentPage.id).map((component) => component.id));
    const remainingPages = pagesForDocument(current).filter((page) => page.id !== currentPage.id);
    commit((active) => ({
      ...active,
      pages: remainingPages,
      components: active.components.filter((component) => !removedIds.has(component.id)).map((component) => component.interaction?.type === 'page' && component.interaction.target === currentPage.id
        ? { ...component, interaction: undefined }
        : component),
      symbols: active.symbols?.map((symbol) => ({
        ...symbol,
        components: symbol.components.map((component) => component.interaction?.type === 'page' && component.interaction.target === currentPage.id
          ? { ...component, interaction: undefined }
          : component)
      })),
      requests: active.requests.filter((request) => !request.componentId || !removedIds.has(request.componentId))
    }));
    const remainingArtboards = workspacePlacement?.artboards.filter((artboard) => artboard.pageId !== currentPage.id) ?? [];
    if (workspacePlacement) setWorkspacePlacement({ ...workspacePlacement, artboards: remainingArtboards });
    const nextArtboard = remainingArtboards.find((artboard) => artboard.pageId === remainingPages[0].id) ?? remainingArtboards[0];
    if (nextArtboard) activateWorkspaceArtboard(nextArtboard);
    else setPageId(remainingPages[0].id);
  }

  function updateCurrentPage(changes: Partial<{ name: string; slug: string; surfaceKind: WorkspaceSurfaceKind }>) {
    if (!currentPage) return;
    if (sceneDocumentRef.current) {
      if (changes.name?.trim() && changes.name.trim() !== currentPage.name) {
        void commitSceneCommand({ type: 'rename-page', pageId: currentPage.id, name: changes.name.trim() }, '用户重命名 Scene 画板。')
          .catch((error) => showToast(error instanceof Error ? error.message : String(error)));
      }
      if (changes.surfaceKind) {
        setWorkspacePlacement((current) => current ? {
          ...current,
          artboards: current.artboards.map((artboard) => artboard.pageId === currentPage.id ? { ...artboard, surfaceKind: changes.surfaceKind! } : artboard)
        } : current);
      }
      return;
    }
    commit((current) => ({
      ...current,
      pages: pagesForDocument(current).map((page) => page.id === currentPage.id ? { ...page, ...changes } : page)
    }));
    if (changes.surfaceKind) {
      setWorkspacePlacement((current) => current ? {
        ...current,
        artboards: current.artboards.map((artboard) => artboard.pageId === currentPage.id ? { ...artboard, surfaceKind: changes.surfaceKind! } : artboard)
      } : current);
    }
  }

  function useAsset(asset: WebDesignAsset) {
    const current = documentRef.current;
    if (!current) return;
    if (selected?.type === 'image') {
      updateComponent(selected.id, (component) => ({ ...component, content: asset.dataUrl, name: asset.name }));
      return;
    }
    let component = componentDefaults('image', 70, 70);
    component.pageId = pageId;
    component.name = asset.name;
    component.content = asset.dataUrl;
    component.zIndex = Math.max(1, ...componentsForPage(current, pageId).map((item) => item.zIndex)) + 1;
    if (device !== 'desktop') component = updateComponentFrame(component, device, { x: 32, y: 80, width: 326, height: 220 });
    commit((active) => ({ ...active, components: [...active.components, component] }));
    setSelectedId(component.id);
    setSelectedIds([component.id]);
  }

  async function importAssets(files: FileList | null) {
    if (!files?.length) return;
    for (const file of Array.from(files)) {
      if (!file.type.startsWith('image/')) {
        showToast(`${file.name} 不是图片文件`);
        continue;
      }
      if (file.size > 8_000_000) {
        showToast(`${file.name} 超过 8MB`);
        continue;
      }
      const dataUrl = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error ?? new Error('读取图片失败'));
        reader.readAsDataURL(file);
      });
      const asset: WebDesignAsset = {
        id: `asset-${crypto.randomUUID().slice(0, 8)}`,
        name: file.name,
        mimeType: file.type,
        dataUrl,
        createdAt: new Date().toISOString()
      };
      commit((current) => ({ ...current, assets: [...(current.assets ?? []), asset] }));
      useAsset(asset);
    }
    if (assetInput.current) assetInput.current.value = '';
  }

  function downloadTextFile(filename: string, content: string, mimeType: string) {
    const blob = new Blob([content], { type: `${mimeType};charset=utf-8` });
    const url = URL.createObjectURL(blob);
    const link = window.document.createElement('a');
    link.href = url;
    link.download = filename;
    link.style.display = 'none';
    window.document.body.appendChild(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    showToast(`已导出 ${filename}`);
  }

  function exportCurrentPage() {
    const current = documentRef.current;
    if (!current || !currentPage) return;
    const filename = currentPage.slug === '/' ? 'index.html' : `${currentPage.slug.replace(/^\/+|\/+$/g, '') || currentPage.id}.html`;
    downloadTextFile(filename, exportPageHtml(current, currentPage.id, device), 'text/html');
  }

  function exportReact() {
    const current = documentRef.current;
    if (!current) return;
    const file = exportReactComponent(current, device);
    downloadTextFile(file.filename, file.content, 'text/javascript');
  }

  function exportVue() {
    const current = documentRef.current;
    if (!current) return;
    const file = exportVueComponent(current, device);
    downloadTextFile(file.filename, file.content, 'text/plain');
  }

  function activatePreviewInteraction(component: WebDesignComponent) {
    if (!component.interaction) return;
    if (component.interaction.type === 'page') {
      const target = pages.find((page) => page.id === component.interaction!.target);
      if (!target) return;
      if ((target.surfaceKind ?? 'page') === 'page') switchPage(target.id);
      else setPreviewOverlayPageId(target.id);
      return;
    }
    window.open(component.interaction.target, '_blank', 'noopener,noreferrer');
  }

  function activateScenePrototype(link: ScenePrototypeLink) {
    const target = pages.find((page) => page.id === link.targetPageId);
    if (!target) {
      showToast('原型目标画板已经不存在');
      return;
    }
    if (link.action === 'navigate') switchPage(target.id);
    else setPreviewOverlayPageId(target.id);
  }

  function addLegacyAnnotation() {
    if (!selected || !annotationText.trim()) return;
    const annotation = { id: `note-${crypto.randomUUID().slice(0, 8)}`, text: annotationText.trim(), status: 'open' as const, createdAt: new Date().toISOString() };
    updateComponent(selected.id, (component) => ({ ...component, annotations: [...component.annotations, annotation] }));
    setAnnotationText('');
  }

  async function prepareSceneAnnotation(nodeId: string, annotationId: string) {
    const scene = sceneDocumentRef.current;
    if (!repository || !scene) return;
    setSceneAnnotationPreparingId(annotationId);
    try {
      const context = await repository.prepareSceneAnnotationTask(scene.documentId, {
        nodeId,
        annotationId,
        viewportWidth: breakpoint.width
      });
      setSceneAiContext(context);
      showToast('视觉批注已准备，AI 可按稳定节点和截图继续修改');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    } finally {
      setSceneAnnotationPreparingId(undefined);
    }
  }

  async function addSceneAnnotation(body = annotationText, prepareForAi = false) {
    const node = selectedSceneNode ?? activeScenePage?.children[0];
    const trimmed = body.trim();
    if (!node || !trimmed) return;
    const annotationId = `annotation:${crypto.randomUUID()}`;
    try {
      await commitSceneCommand({ type: 'add-annotation', nodeId: node.id, annotationId, body: trimmed }, '用户为 Scene 图层添加视觉批注。');
      setAnnotationText('');
      if (prepareForAi) await prepareSceneAnnotation(node.id, annotationId);
      else showToast('已添加批注，AI 会把它视为待处理设计任务');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function changeSceneAnnotationStatus(annotationId: string, status: 'open' | 'resolved') {
    if (!selectedSceneNode) return;
    try {
      await commitSceneCommand({
        type: status === 'open' ? 'reopen-annotation' : 'resolve-annotation',
        nodeId: selectedSceneNode.id,
        annotationId
      }, status === 'open' ? '用户重新打开 Scene 批注。' : '用户确认 Scene 批注已完成。');
      if (sceneAiContext?.task.annotationId === annotationId) setSceneAiContext(undefined);
      showToast(status === 'open' ? '批注已重新打开' : '批注已标记完成');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function submitSceneAiInstruction() {
    const instruction = aiInstruction.trim();
    if (!instruction) return;
    setAiInstruction('');
    await addSceneAnnotation(instruction, true);
  }

  async function addAiRequest(instruction = aiInstruction) {
    const current = documentRef.current;
    if (!current || !instruction.trim()) return;
    const target = selected ?? editingContainer;
    const request = {
      id: `request-${crypto.randomUUID().slice(0, 8)}`,
      componentId: target?.id,
      instruction: `[${device}][${editingSlotDefinition ? `内容区域:${editingSlotDefinition.label}` : target ? `组件:${target.name}` : `页面:${currentPage?.name ?? pageId}`}] ${instruction.trim()}`,
      status: 'pending' as const,
      createdAt: new Date().toISOString()
    };
    commit((active) => ({ ...active, requests: [...active.requests, request] }));
    setAiInstruction('');
    await save(true, true);
    showToast(target ? '已提交组件修改任务' : '已提交整页设计任务');
  }

  const storageBadge = <span className={`service-pill ${repository?.mode === 'server' ? 'online' : ''}`}>{repository?.mode === 'server' ? '本地服务' : '浏览器存储'}</span>;
  const newDesignModal = newDesignOpen && activeProject && <div className="studio-modal-backdrop" onPointerDown={() => setNewDesignOpen(false)}>
    <section className="studio-modal project-create-modal" onPointerDown={(event) => event.stopPropagation()}>
      <header><div><span className="eyebrow">{activeProject.name}</span><h2>新建设计工作区</h2><p>先创建空的设计范围，再让 AI 逐页规划、分步骤生成和视觉验收；不会自动塞入演示模板。</p></div><button onClick={() => setNewDesignOpen(false)}>×</button></header>
      <div className="project-form-body">
        <label>设计名称<input autoFocus maxLength={240} value={newDesignName} onChange={(event) => setNewDesignName(event.target.value)} placeholder="例如：官网改版 2026" /></label>
        <div className="ai-first-create-note"><span>✦</span><div><strong>AI 分步设计</strong><small>先定页面清单和视觉方向，再一次完成一个有界步骤。复杂页面可以多轮完善。</small></div></div>
      </div>
      <footer className="project-modal-actions"><button className="quiet-button" onClick={() => setNewDesignOpen(false)}>取消</button><button className="primary-button" disabled={!newDesignName.trim()} onClick={() => void createDesignFromSheet()}>创建并打开</button></footer>
    </section>
  </div>;

  if (!ready) return <div className="loading-screen"><div className="loading-dot" />正在准备 Web Design Studio…</div>;

  if (screen === 'project' && activeProject) return <div className="web-project-shell">
    <header className="web-project-toolbar"><div className="brand"><span className="brand-mark">W</span><span>{activeProject.name}</span>{storageBadge}</div><button className="primary-button" onClick={() => void createNew()}>＋ 新建设计</button></header>
    <main className="web-project-home">
      <section className="web-project-intro"><div><span className="eyebrow">网站项目</span><h1>{activeProject.name}</h1><p>{activeProject.description || `项目内共有 ${activeProjectDocuments.length} 份网站设计。`}</p></div><button className="web-project-new-card" onClick={() => void createNew()}><span>＋</span><strong>新建设计工作区</strong><small>由 AI 逐页规划、分步生成，人负责审阅和批注</small></button></section>
      <section className="web-project-section"><div className="web-project-section-heading"><h2>项目设计</h2><span>{activeProjectDocuments.length} 份</span></div>
        {activeProjectDocuments.length ? <div className="web-design-grid">{activeProjectDocuments.map((item) => <article className="web-design-card" key={item.documentId}>
          <button className="web-design-card-open" onClick={() => void openProjectDocument(item.documentId)}><span className="web-design-thumbnail"><i /><i /><i /></span><span className="web-project-card-copy"><strong>{item.title}</strong><small>{item.pageCount ?? 1} 个页面 · {item.componentCount} 个组件 · v{item.revision}</small></span><time>{formatProjectDate(item.updatedAt)}</time><b>›</b></button>
          <button className="web-design-delete" aria-label={`删除设计 ${item.title}`} onClick={() => void deleteProjectDocument(item)}>×</button>
        </article>)}</div> : <div className="web-project-empty"><span>▧</span><strong>这个项目还没有网站设计</strong><p>先创建一份设计，为它单独命名，再进入画布设计页面。</p><button className="primary-button" onClick={() => void createNew()}>＋ 新建网站设计</button></div>}
      </section>
    </main>{newDesignModal}{toast && <div className="toast">{toast}</div>}
  </div>;

  if (!document || !activeProject) return <div className="loading-screen"><div className="loading-dot" />正在打开网站项目…</div>;

  const layerComponents = flattenComponentTree(document, pageId);
  const sceneLayerNodes = sceneDocument
    ? [...indexSceneDocument(sceneDocument).values()]
      .filter((entry) => entry.pageId === pageId)
      .map((entry) => ({ node: entry.node, depth: Math.max(0, entry.path.length - 2) }))
    : [];
  const directChildCount = selected ? document.components.filter((component) => component.parentId === selected.id).length : 0;
  const canUngroup = Boolean(selected?.id.startsWith('group-') && directChildCount > 0);
  const canUngroupScene = Boolean(selectedSceneNode
    && (selectedSceneNode.type === 'group' || selectedSceneNode.type === 'frame')
    && selectedSceneNode.layout.mode === 'free');
  const selectedSceneLibrary = selectedSceneNode?.type === 'library-instance' ? uiLibraryByName(selectedSceneNode.library as WebDesignLibraryName) : undefined;
  const selectedSceneLibraryDefinition = selectedSceneNode?.type === 'library-instance'
    ? selectedSceneLibrary?.components.find((item) => item.id === selectedSceneNode.component)
    : undefined;
  const selectedSceneLibraryVariants = selectedSceneNode?.type === 'library-instance' && selectedSceneLibrary
    ? selectedSceneLibrary.variants[selectedSceneNode.component] ?? [{ id: 'default', label: '默认款式', props: {} }]
    : [];
  const selectedSceneRegistryElement = selectedSceneNode?.type === 'library-instance'
    ? libraryPreviewSelection(selectedSceneNode.properties.registryElement)
    : undefined;
  const selectedSceneEditableSlots = selectedSceneNode?.type === 'library-instance'
    ? editableSlotsForSceneLibraryNode(selectedSceneNode)
    : [];
  const selectedSymbol = selected?.symbolId ? document.symbols?.find((symbol) => symbol.id === selected.symbolId) : undefined;
  const selectedLibrary = uiLibraryByName(selected?.library?.name);
  const selectedLibraryDefinition = selected?.library ? selectedLibrary?.components.find((item) => item.id === selected.library?.component) : undefined;
  const selectedLibraryVariants = selected?.library ? variantsForBoundComponent(selected) : [];
  const selectedRegistryElement = libraryPreviewSelection(selected?.library?.props.registryElement);
  const selectedEditableSlots = selected ? editableSlotsForUiComponent(selected) : [];
  const inspectorCapabilities = selected ? resolveInspectorCapabilities(selected.type, {
    library: Boolean(selected.library),
    directChildCount,
    editableSlotCount: selectedEditableSlots.length
  }) : undefined;
  const selectedInspectableLibraryProps = selected?.library ? inspectableLibraryProps(selected.library.props) : [];
  const aiTarget = selected ?? editingContainer;
  const normalizedPaletteQuery = paletteQuery.trim().toLowerCase();
  const filteredPalette = palette.filter((item) => !normalizedPaletteQuery
    || `${item.label} ${item.id} ${item.keywords.join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const filteredPersonalSymbols = personalSymbols.filter((symbol) => !normalizedPaletteQuery
    || `${symbol.name} ${symbol.components.map((component) => component.name).join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const filteredSceneSnippets = sceneSnippets.filter((snippet) => !normalizedPaletteQuery
    || `${snippet.name} ${snippet.nodes.map((node) => node.name).join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const activeUiLibrary = libraryTab !== 'components' && libraryTab !== 'my' && libraryTab !== 'layers' ? uiLibraryByName(libraryTab) : undefined;
  const filteredUiLibraryComponents = activeUiLibrary?.components.filter((item) => !normalizedPaletteQuery
    || `${item.id} ${item.label} ${item.keywords.join(' ')}`.toLowerCase().includes(normalizedPaletteQuery)) ?? [];
  const variantPickerLibrary = variantPickerTarget ? uiLibraryByName(variantPickerTarget.library) : undefined;
  const variantPickerDefinition = variantPickerTarget ? variantPickerLibrary?.components.find((item) => item.id === variantPickerTarget.componentId) : undefined;
  const variantPickerVariants = variantPickerDefinition && variantPickerLibrary ? variantPickerLibrary.variants[variantPickerDefinition.id] ?? [{ id: 'default', label: '默认款式', props: {} }] : [];
  const variantPickerPresentation = variantPickerDefinition && variantPickerLibrary
    ? officialRuntimePresentation(variantPickerLibrary.id, String(variantPickerDefinition.props?.componentSlug ?? variantPickerDefinition.id))
    : undefined;
  const sceneAiTarget = selectedSceneNode ?? activeScenePage?.children[0];
  const sceneAnnotationTasks = sceneDocument
    ? [...indexSceneDocument(sceneDocument).values()].flatMap((entry) => entry.node.annotations
      .filter((annotation) => annotation.status === 'open')
      .map((annotation) => ({ annotation, node: entry.node, pageId: entry.pageId })))
    : [];
  const aiQuickPrompts = selectedSceneNode
    ? ['让这个组件更精致、更有层次', '优化尺寸、间距和对齐', '给我 3 个更好看的视觉方案']
    : ['设计一个像 Apple 官网一样克制高级的页面', '统一整页的字号、间距、圆角和色彩', '检查并修复页面中不协调的视觉细节'];

  function renderGenerationReviewPanel() {
    if (generationLoading && !generationPlan) return <div className="generation-review-empty"><span className="loading-dot" /><strong>正在读取 AI 设计进度…</strong></div>;
    if (!generationPlan) return <div className="generation-review-empty">
      <div className="empty-icon">✦</div>
      <strong>AI 还没有开始分步设计</strong>
      <p>AI 会先规划网站和当前页面，再一次只提交一个可审阅的视觉步骤。页面不需要一轮完成。</p>
      <small>开始后，这里会显示真实截图、视觉差异、质量结论和接受/退回操作。</small>
      <button className="secondary-button" onClick={() => void refreshGenerationState(true).catch((error) => showToast(error instanceof Error ? error.message : String(error)))}>刷新进度</button>
    </div>;
    const plan = generationPlan;
    const activePage = plan.pages.find((page) => page.pageId === plan.activePage?.pageId);
    const candidate = generationReview?.candidate;
    const artifacts = [...new Map([...(candidate?.artifacts ?? []), ...(generationReview?.attempt.artifacts ?? [])]
      .map((artifact) => [artifact.artifactId, artifact])).values()];
    const imageArtifacts = artifacts.filter((artifact) => ['page-snapshot', 'region-crop', 'visual-diff'].includes(artifact.kind));
    const statusLabel: Record<string, string> = {
      draft: '规划中', ready: '待开始', running: '设计中', paused: '已暂停', completed: '已完成', failed: '需要处理',
      planned: '已规划', generating: '生成中', validating: '视觉验收中', 'awaiting-review': '等待你审阅', accepted: '已接受',
      rejected: '已退回', retryable: '等待重做', blocked: '被阻塞', stale: '需要更新', 'rolled-back': '已回滚'
    };
    return <div className="generation-review-panel">
      <section className="generation-plan-summary">
        <header><div><span>AI 设计计划</span><strong>{plan.objective}</strong></div><em className={`generation-status ${plan.status}`}>{statusLabel[plan.status] ?? plan.status}</em></header>
        <p>{plan.audience.join(' · ')}</p>
        <small>Plan r{plan.revision} · {plan.mode === 'auto-current-page' ? '当前页面自动推进' : '逐步审阅模式'}</small>
      </section>
      <div className="generation-plan-pages">
        {plan.pages.map((plannedPage) => <article key={plannedPage.pageId} className={plannedPage.pageId === plan.activePage?.pageId ? 'active' : ''}>
          <span>{plannedPage.order + 1}</span><div><strong>{plannedPage.name}</strong><small>{plannedPage.purpose}</small></div><em>{statusLabel[plannedPage.status] ?? plannedPage.status}</em>
        </article>)}
      </div>
      {activePage?.design && <details className="generation-design-intent" open>
        <summary>当前页面视觉方向</summary>
        <dl><div><dt>美术方向</dt><dd>{activePage.design.artDirection}</dd></div><div><dt>构图</dt><dd>{activePage.design.compositionIntent}</dd></div><div><dt>排版</dt><dd>{activePage.design.typographyIntent}</dd></div><div><dt>图片策略</dt><dd>{activePage.design.imageStrategy}</dd></div></dl>
        <ul>{activePage.design.designAcceptanceCriteria.map((criterion) => <li key={criterion}>{criterion}</li>)}</ul>
      </details>}
      {plan.activeStep && <section className={`generation-active-step ${plan.activeStep.status}`}>
        <header><div><span>当前只做这一小步</span><strong>{plan.activeStep.title}</strong></div><em>{statusLabel[plan.activeStep.status] ?? plan.activeStep.status}</em></header>
        <p>{plan.activeStep.kind} · {plan.activeStep.target.nodeIds.join('、')}</p>
        {plan.activeStep.target.viewportWidths.length > 0 && <small>验收宽度：{plan.activeStep.target.viewportWidths.join(' / ')} px</small>}
      </section>}
      {generationReview && <section className="generation-candidate-review">
        <header><div><span>候选方案</span><strong>{generationReview.step.title}</strong></div><em>Scene r{generationReview.attempt.baseRevision} → r{generationReview.attempt.baseRevision + 1}</em></header>
        {imageArtifacts.length > 0 && <div className="generation-candidate-images">{imageArtifacts.map((artifact) => <figure key={artifact.artifactId}>
          <img src={repository?.generationArtifactImageUrl(documentRef.current?.documentId ?? '', artifact.artifactId)} alt={`${artifact.kind} ${artifact.artifactId}`} onError={(event) => { event.currentTarget.closest('figure')?.classList.add('image-unavailable'); }} />
          <figcaption><strong>{artifact.kind === 'visual-diff' ? '视觉差异' : artifact.kind === 'region-crop' ? '局部截图' : '页面截图'}</strong><span>{artifact.viewportWidth ? `${artifact.viewportWidth}px` : `r${artifact.revision}`}</span></figcaption>
        </figure>)}</div>}
        {candidate && <><p className="generation-quality-summary">{candidate.qualitySummary}</p>
          {candidate.issueIds.length > 0 && <div className="generation-issues"><strong>仍需注意</strong>{candidate.issueIds.map((issue) => <span key={issue}>{issue}</span>)}</div>}
          {candidate.protectionConflicts.length > 0 && <div className="generation-protection-warning"><strong>会触及人工调整</strong><span>{candidate.protectionConflicts.length} 个受保护字段，需要你明确确认。</span></div>}
        </>}
        {generationReview.attempt.error && <div className="generation-attempt-error"><strong>{generationReview.attempt.error.code}</strong><span>{generationReview.attempt.error.message}</span></div>}
      </section>}
      {candidate && plan.activeStep?.status === 'awaiting-review' && <section className="generation-review-actions">
        <button className="ai-button" disabled={Boolean(generationAction)} onClick={() => void runGenerationReviewAction('accept')}>{generationAction === 'accept' ? '正在提交…' : '接受这一小步'}</button>
        <textarea rows={3} maxLength={4000} value={generationRejectionReason} onChange={(event) => setGenerationRejectionReason(event.target.value)} placeholder="指出具体视觉问题，例如层级、留白、构图或图片不符合方向…" />
        <button className="secondary-button danger" disabled={Boolean(generationAction) || !generationRejectionReason.trim()} onClick={() => void runGenerationReviewAction('reject')}>{generationAction === 'reject' ? '正在退回…' : '退回并让 AI 重做'}</button>
      </section>}
      <div className="generation-plan-controls">
        {plan.status === 'paused'
          ? <button disabled={Boolean(generationAction)} onClick={() => void runGenerationReviewAction('resume')}>继续 AI 设计</button>
          : plan.status === 'running' && <button disabled={Boolean(generationAction)} onClick={() => void runGenerationReviewAction('pause')}>暂停 AI 设计</button>}
        <button disabled={generationLoading} onClick={() => void refreshGenerationState(true).catch((error) => showToast(error instanceof Error ? error.message : String(error)))}>刷新</button>
      </div>
    </div>;
  }

  function renderWorkspaceArtboard(artboard: WorkspaceArtboardPlacement) {
    const currentDocument = documentRef.current;
    if (!currentDocument) return null;
    const targetDevice = deviceForWorkspaceArtboard(currentDocument, artboard);
    const targetCanvasHeight = sceneDocument
      ? sceneArtboardContentHeight(sceneDocument, artboard.pageId, artboard.viewportWidth, artboard.viewportHeight)
      : artboard.viewportHeight;
    const targetPage = pages.find((page) => page.id === artboard.pageId);
    const active = artboard.artboardId === activeArtboardId;
    const surfaceLabel = WORKSPACE_SURFACE_LABELS[artboard.surfaceKind];
    const viewport = canvasScroll.current;
    const viewportSize = viewport ? { width: viewport.clientWidth, height: viewport.clientHeight } : undefined;
    const renderTier = workspaceViewportReady(viewportSize)
      ? workspaceArtboardRenderTier(
          workspaceCamera,
          workspaceArtboardContentBounds(currentDocument, artboard, sceneDocument),
          viewportSize,
          active
        )
      : 'runtime';
    if (renderTier === 'anchor') return <div
      key={artboard.artboardId}
      className="workspace-artboard-anchor"
      style={{ left: artboard.x, top: artboard.y, width: artboard.viewportWidth, height: targetCanvasHeight }}
      data-artboard-id={artboard.artboardId}
      data-render-tier="anchor"
      aria-hidden="true"
    />;
    const contentVisible = renderTier === 'content' || renderTier === 'runtime';
    return <div
      key={artboard.artboardId}
      className={`workspace-artboard surface-${artboard.surfaceKind} ${active ? 'active' : ''}`}
      style={{ left: artboard.x, top: artboard.y, width: artboard.viewportWidth, height: targetCanvasHeight }}
      data-artboard-id={artboard.artboardId}
      data-surface-kind={artboard.surfaceKind}
      data-render-tier={renderTier}
    >
      <button className="workspace-artboard-header" onPointerDown={(event) => beginWorkspaceArtboardMove(event, artboard)} onClick={() => activateWorkspaceArtboard(artboard)}>
        <span className="workspace-artboard-status" />
        <strong>{targetPage?.name ?? artboard.pageId}</strong>
        <span className="workspace-surface-kind">{surfaceLabel}</span>
        <em>{artboard.viewportWidth} × {artboard.viewportHeight}</em>
        <small>{active ? '当前画板' : '点击选择'}</small>
      </button>
      <div className={`design-canvas workspace-projected-canvas device-${targetDevice}`} style={{
        width: artboard.viewportWidth,
        height: targetCanvasHeight,
        background: currentDocument.viewport.background,
        fontFamily: tokens?.typography.fontFamily,
        fontSize: tokens?.typography.baseFontSize,
        '--color-primary': tokens?.colors.primary,
        '--color-accent': tokens?.colors.accent,
        '--color-surface': tokens?.colors.surface,
        '--color-text': tokens?.colors.text,
        '--color-muted': tokens?.colors.muted,
        '--radius-small': `${tokens?.radii.small ?? 8}px`,
        '--radius-medium': `${tokens?.radii.medium ?? 16}px`,
        '--radius-large': `${tokens?.radii.large ?? 28}px`
      } as CSSProperties}
        data-page-id={artboard.pageId}
        data-artboard-id={artboard.artboardId}
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => { if (sceneDocument) void onSceneCanvasDrop(event, artboard); }}>
        {artboard.viewportHeight < targetCanvasHeight && <div className="viewport-fold-line" style={{ top: artboard.viewportHeight }}><span>首屏结束 · {artboard.viewportWidth} × {artboard.viewportHeight}</span></div>}
        {contentVisible && sceneDocument && <SceneArtboardCanvas
          scene={sceneDocument}
          pageId={artboard.pageId}
          viewportWidth={artboard.viewportWidth}
          viewportHeight={artboard.viewportHeight}
          active={active && !interactionMode}
          interactive={interactionMode}
          selectionOnly={workspaceShell.activeTool === 'comment'}
          selectedIds={active ? selectedIds : []}
          primaryId={active ? selectedId : undefined}
          onSelectionChange={(ids, primary) => {
            setSelectedIds(ids);
            setSelectedId(primary);
            if (primary && workspaceShell.activeTool === 'comment') {
              setInspectorTab('ai');
              if (!workspaceShell.rightPanelOpen) dispatchWorkspaceShell({ type: 'toggle-right-panel' });
              showToast('已定位 Scene 图层，请在右侧添加视觉批注');
            }
          }}
          onCommit={commitSceneCommand}
          onError={showToast}
          onPrototypeActivate={(link) => activateScenePrototype(link)}
          contentFocus={sceneContentFocus?.pageId === artboard.pageId ? sceneContentFocus : undefined}
        />}
        {contentVisible && !sceneDocument && <div className={`scene-v2-load-state ${sceneLoadState}`} style={{ minHeight: artboard.viewportHeight }}>
          {sceneLoadState === 'loading'
            ? <><span className="loading-dot" /><strong>正在读取 AI 设计场景…</strong></>
            : <><strong>这个设计还没有 Scene 画布</strong><span>请让 AI 先规划当前页面并生成第一个视觉步骤。</span></>}
        </div>}
        {!active && <button className="workspace-artboard-activation" onClick={() => activateWorkspaceArtboard(artboard)}><span>选择此画板</span></button>}
      </div>
    </div>;
  }

  function renderPreviewSurfaceOverlay() {
    if (!previewOverlayPage || !documentRef.current || !sceneDocument) return null;
    const currentDocument = documentRef.current;
    const surfaceKind = previewOverlayPage.surfaceKind ?? previewOverlayArtboard?.surfaceKind ?? 'modal';
    const defaultSize = surfaceKind === 'page' || surfaceKind === 'state' ? { width: 960, height: 720 } : WORKSPACE_SURFACE_SIZES[surfaceKind];
    const width = previewOverlayArtboard?.viewportWidth ?? defaultSize.width;
    const height = previewOverlayArtboard?.viewportHeight ?? defaultSize.height;
    const overlayHeight = sceneArtboardContentHeight(sceneDocument, previewOverlayPage.id, width, height);
    const frameHeight = Math.min(overlayHeight, Math.max(320, window.innerHeight - 96));
    return <div className={`preview-surface-backdrop surface-${surfaceKind}`} onPointerDown={() => setPreviewOverlayPageId(undefined)}>
      <div className="preview-surface-frame" style={{ width, height: frameHeight }} onPointerDown={(event) => event.stopPropagation()}>
        <button className="preview-surface-close" aria-label="关闭叠层" onClick={() => setPreviewOverlayPageId(undefined)}>×</button>
        <div className="preview-surface-canvas design-canvas device-desktop" style={{
          width,
          height: overlayHeight,
          background: currentDocument.viewport.background,
          fontFamily: tokens?.typography.fontFamily,
          fontSize: tokens?.typography.baseFontSize,
          '--color-primary': tokens?.colors.primary,
          '--color-accent': tokens?.colors.accent,
          '--color-surface': tokens?.colors.surface,
          '--color-text': tokens?.colors.text,
          '--color-muted': tokens?.colors.muted,
          '--radius-small': `${tokens?.radii.small ?? 8}px`,
          '--radius-medium': `${tokens?.radii.medium ?? 16}px`,
          '--radius-large': `${tokens?.radii.large ?? 28}px`
        } as CSSProperties}>
          <SceneArtboardCanvas
            scene={sceneDocument}
            pageId={previewOverlayPage.id}
            viewportWidth={width}
            viewportHeight={height}
            active={false}
            interactive
            selectedIds={[]}
            onSelectionChange={() => undefined}
            onCommit={commitSceneCommand}
            onError={showToast}
            onPrototypeActivate={(link) => activateScenePrototype(link)}
          />
        </div>
      </div>
    </div>;
  }

  return (
    <div className={`studio-shell ${preview ? 'preview-active' : ''}`}>
      <header className="topbar">
        <div className="brand editor-brand"><button className="web-home-button" onClick={goToActiveProject} aria-label="返回当前项目首页">‹</button><span className="brand-mark">W</span></div>
        <button className={`project-library-trigger ${projectLibraryOpen ? 'active' : ''}`} onClick={() => setProjectLibraryOpen((open) => !open)} aria-label="打开当前项目的设计列表">
          <span>⌘</span><strong>{activeProject.name}</strong><small>{activeProjectDocuments.length}</small><b>⌄</b>
        </button>
        <div className="editor-document-title"><span>项目：{activeProject.name}<i>/</i></span><strong>{document.title}</strong><small>{saving ? '正在保存…' : dirty ? '未保存修改' : `已保存 · v${persistedRevision}`}</small></div>
        <button className="quiet-button compact-new-design" aria-label="在当前项目中新建设计" onClick={() => void createNew()}>＋ 新建设计</button>
        <button className="quiet-button style-trigger" onClick={() => setThemePickerOpen(true)}>设计风格</button>
        <div className="history-tools">
          <button title="撤销 ⌘Z" disabled={sceneEditingActive ? !sceneHistory?.undoCount : past.length === 0} onClick={undo}>↶</button>
          <button title="重做 ⇧⌘Z" disabled={sceneEditingActive ? !sceneHistory?.redoCount : future.length === 0} onClick={redo}>↷</button>
        </div>
        <div className="topbar-spacer" />
        <span className={`service-pill ${repository?.mode === 'server' ? 'online' : ''}`}>{repository?.mode === 'server' ? '本地服务' : '浏览器存储'}</span>
        <button className="quiet-button" onClick={() => void refresh()}>刷新</button>
        <button className={`quiet-button ${preview ? 'active' : ''}`} onClick={toggleFullPreview}>{preview ? '退出预览' : '全屏预览'}</button>
        <button className="ai-design-trigger" onClick={() => activateWorkspaceArea('ai')}>✦ AI 设计</button>
        <button className="primary-button" disabled={!dirty || saving} onClick={() => void save()}>{saving ? '保存中…' : dirty ? '保存' : '已保存'}</button>
      </header>

      {projectLibraryOpen && !preview && <div className="project-library-popover">
        <header><div><span className="eyebrow">当前网站项目</span><strong>{activeProject.name}</strong></div><button onClick={() => setProjectLibraryOpen(false)} aria-label="关闭项目设计列表">×</button></header>
        <div className="project-library-list">
          {activeProjectDocuments.map((item) => <div key={item.documentId} className={`project-library-item ${item.documentId === document.documentId ? 'active' : ''}`}>
            <button onClick={() => void openProjectDocument(item.documentId)}><span className="project-library-thumb"><i /><i /><i /></span><span><strong>{item.title}</strong><small>{item.pageCount ?? 1} 个页面 · {item.componentCount} 个组件 · v{item.revision}</small></span></button>
            <button className="project-library-delete" onClick={() => void deleteProjectDocument(item)} aria-label={`删除设计 ${item.title}`}>×</button>
          </div>)}
        </div>
        <footer><button onClick={() => void createNew()}>＋ 在当前项目中新建设计</button><button onClick={goToActiveProject}>查看项目首页</button></footer>
      </div>}

      <main className={`workspace workspace-v3-shell ${preview ? 'preview-mode' : ''}`} style={workspaceShellGridStyle(workspaceShell) as CSSProperties}>
        {!preview && <WorkspaceNavigationBar activeArea={workspaceShell.activeArea} leftPanelOpen={workspaceShell.leftPanelOpen} onSelect={activateWorkspaceArea} onToggleLeft={() => dispatchWorkspaceShell({ type: 'toggle-left-panel' })} />}
        {!preview && workspaceShell.leftPanelOpen && <aside className="palette-panel">
          {workspaceShell.activeArea === 'assets' && <div className="library-tabs workspace-library-tabs">
            <button className={libraryTab === 'antd' ? 'active' : ''} onClick={() => chooseLibraryTab('antd')}>AntD</button>
            <button className={libraryTab === 'chakra' ? 'active' : ''} onClick={() => chooseLibraryTab('chakra')}>Chakra</button>
            <button className={libraryTab === 'shadcn' ? 'active' : ''} onClick={() => chooseLibraryTab('shadcn')}>shadcn</button>
            <button className={libraryTab === 'magicui' ? 'active' : ''} onClick={() => chooseLibraryTab('magicui')}>Magic</button>
            <button className={libraryTab === 'spell' ? 'active' : ''} onClick={() => chooseLibraryTab('spell')}>Spell</button>
            <button className={libraryTab === 'inspira' ? 'active' : ''} onClick={() => chooseLibraryTab('inspira')}>Inspira</button>
            <button className={libraryTab === 'daisyui' ? 'active' : ''} onClick={() => chooseLibraryTab('daisyui')}>daisyUI</button>
          </div>}
          <div className="palette-panel-content">
            {['assets', 'tools', 'my'].includes(workspaceShell.activeArea) && <input className="component-search" value={paletteQuery} onChange={(event) => setPaletteQuery(event.target.value)} placeholder={workspaceShell.activeArea === 'tools' ? '搜索视觉原语…' : activeUiLibrary ? `搜索 ${activeUiLibrary.displayName} 组件…` : '搜索我的组件…'} />}

            {workspaceShell.activeArea === 'tools' && <>
              <div className="panel-intro"><strong>视觉原语</strong><span>用矩形、圆形和直线组合背景、光效、装饰与容器；产品控件使用成熟 UI 库</span></div>
              <div className="palette-grid shapes-grid">{filteredPalette.map((item) => <div key={item.id} className="palette-item" draggable onDragStart={(event) => onPaletteDrag(event, item.id)}><span className="palette-icon">{item.icon}</span><span>{item.label}</span></div>)}</div>
            </>}

            {workspaceShell.activeArea === 'assets' && activeUiLibrary && <>
              <div className={`ui-library-heading library-${activeUiLibrary.id}`}><div className="ui-library-logo-mark">{activeUiLibrary.brandMark}</div><div><strong>{activeUiLibrary.displayName}</strong><span>{activeUiLibrary.license ? `开源组件 · ${activeUiLibrary.license} · ${activeUiLibrary.version}` : activeUiLibrary.id === 'shadcn' ? `本地源码组件 · ${activeUiLibrary.version}` : `官方运行时 · v${activeUiLibrary.version}`}</span></div></div>
              <div className="panel-intro"><strong>{activeUiLibrary.displayName} 组件总览</strong><span>先打开组件，再点击或拖动你真正需要的单个官方示例</span></div>
              {activeUiLibrary.categories.map((category) => {
                const items = filteredUiLibraryComponents.filter((item) => item.category === category);
                return items.length > 0 && <div key={category} className="ui-library-category"><div className="ui-library-category-title">{category}</div><div className="ui-library-component-list">
                  {items.map((item) => <button key={item.id} onClick={() => setVariantPickerTarget({ library: activeUiLibrary.id, componentId: item.id })}><span className="ui-library-list-icon">{item.icon}</span><strong>{item.id}</strong><small>{item.label}</small><em>{item.status === 'deprecated' ? `已废弃 · ${activeUiLibrary.variants[item.id]?.length ?? 1} 款` : item.introduced ? `v${item.introduced} · ${activeUiLibrary.variants[item.id]?.length ?? 1} 款` : `${activeUiLibrary.variants[item.id]?.length ?? 1} 款`}</em><b>›</b></button>)}
                </div></div>;
              })}
            </>}

            {workspaceShell.activeArea === 'my' && <>
              <div className="panel-intro my-library-intro"><strong>我的设计组合</strong><span>保存真实 Scene 子树，下次插入后仍可继续拆分、移动、批注和让 AI 修改。</span></div>
              {sceneDocument && selectedIds.length > 0 && <button className="my-library-save" onClick={saveSceneSelectionAsSnippet}><span>＋</span><div><strong>保存当前选中</strong><small>{selectedIds.length === 1 ? selectedSceneNode?.name : `${selectedIds.length} 个 Scene 图层`}</small></div></button>}
              {filteredSceneSnippets.length > 0 ? <div className="my-library-grid">{filteredSceneSnippets.map((snippet) => <article key={snippet.id} className="my-library-card"><button className="my-library-insert" onClick={() => void insertSceneSnippet(snippet)}><span className="my-library-preview"><i /><i /><i /></span><span><strong>{snippet.name}</strong><small>{snippet.nodes.length} 个根层 · {Math.round(snippet.width)} × {Math.round(snippet.height)}</small></span></button><footer><button onClick={() => renameSceneSnippet(snippet)}>重命名</button><button className="danger" onClick={() => removeSceneSnippet(snippet.id)}>移除</button></footer></article>)}</div> : <div className="my-library-empty"><span>◇</span><strong>还没有保存的设计组合</strong><p>{sceneDocument ? '在画布中选择一个 Scene 图层，或按住 Shift 选择同一容器里的多个图层，再保存为自己的组合。' : '请先让 AI 创建第一个 Scene 页面，再保存可复用的视觉组合。'}</p></div>}
            </>}

            {workspaceShell.activeArea === 'layers' && <>
              <div className="panel-title layer-title"><span>Scene 图层</span><small>{sceneDocument ? sceneLayerNodes.length : 0}</small></div>
              <div className={`layer-group-actions ${selectedIds.length > 1 || canUngroup || canUngroupScene ? 'ready' : ''}`}>
                <div><strong>{selectedIds.length > 1 ? `已选择 ${selectedIds.length} 个图层` : canUngroup || canUngroupScene ? '当前是一个容器' : '创建可整体移动的分组'}</strong><small>{selectedIds.length > 1 ? 'Group、Frame 或 Auto Layout 都会成为真实 Scene 容器' : canUngroup || canUngroupScene ? '可以取消容器并保持内部图层视觉位置' : '按住 Shift 点击画布或图层进行多选'}</small></div>
                {selectedIds.length > 1
                  ? <button onClick={groupSelected}>创建分组 <kbd>⌘G</kbd></button>
                  : canUngroup || canUngroupScene
                    ? <button onClick={ungroupSelected}>取消分组 <kbd>⇧⌘G</kbd></button>
                    : undefined}
              </div>
              <div className="layers-list expanded">
                {sceneDocument ? sceneLayerNodes.map(({ node, depth }) => <div key={node.id} className={`layer-row ${selectedIdSet.has(node.id) ? 'selected' : ''} ${!node.visible ? 'hidden' : ''}`} style={{ paddingLeft: 4 + depth * 14 }} onClick={(event) => {
                  const additive = event.shiftKey || event.metaKey || event.ctrlKey;
                  if (!additive) {
                    setSelectedId(node.id);
                    setSelectedIds([node.id]);
                    return;
                  }
                  const next = selectedIds.includes(node.id) ? selectedIds.filter((id) => id !== node.id) : [...selectedIds, node.id];
                  setSelectedIds(next);
                  setSelectedId(next.includes(node.id) ? node.id : next.at(-1));
                }}>
                  <button title={node.visible ? '隐藏' : '显示'} onClick={(event) => { event.stopPropagation(); void updateSceneNodeById(node.id, [{ path: ['visible'], value: !node.visible }]); }}>{node.visible ? '●' : '○'}</button>
                  <span className="layer-type">{node.type === 'text' ? 'T' : node.type === 'media' ? '▧' : node.type === 'group' ? '◇' : node.type === 'frame' ? '▣' : '◆'}</span>
                  <span className="layer-name">{depth > 0 ? '└ ' : ''}{node.name}</span>
                  <button title={node.locked ? '解锁' : '锁定'} onClick={(event) => { event.stopPropagation(); void updateSceneNodeById(node.id, [{ path: ['locked'], value: !node.locked }]); }}>{node.locked ? '🔒' : '⌁'}</button>
                </div>) : <div className="scene-sidebar-empty"><strong>等待 AI 建立 Scene</strong><span>先规划页面和视觉方向，再生成第一个有界步骤。</span></div>}
              </div>

              <div className="panel-title section-title">设计面设置</div>
              <label className="field-label">当前画板<select value={currentPage?.id} onChange={(event) => switchPage(event.target.value)}>{pages.map((page) => <option key={page.id} value={page.id}>{page.name} · {WORKSPACE_SURFACE_LABELS[page.surfaceKind ?? 'page']}</option>)}</select></label>
              {sceneDocument ? <div className="page-actions"><button onClick={addPage}>＋ 新建设计面</button><button onClick={duplicatePage}>复制画板</button><button disabled={pages.length <= 1} onClick={deleteCurrentPage}>删除</button></div> : <p className="helper-text">页面清单由 AI 先写入 Plan；开始当前页后才建立可编辑 Scene 画板。</p>}
              {currentPage && <><label className="field-label">画板类型<select value={currentPage.surfaceKind ?? 'page'} onChange={(event) => updateCurrentPage({ surfaceKind: event.target.value as WorkspaceSurfaceKind })}>{(Object.keys(WORKSPACE_SURFACE_LABELS) as WorkspaceSurfaceKind[]).map((kind) => <option key={kind} value={kind}>{WORKSPACE_SURFACE_LABELS[kind]}</option>)}</select></label>{sceneDocument
                ? <><label className="field-label">设计面名称<input key={`${currentPage.id}:${currentPage.name}`} defaultValue={currentPage.name} onBlur={(event) => updateCurrentPage({ name: event.currentTarget.value })} /></label><label className="field-label page-slug">稳定页面 ID<input value={currentPage.id} readOnly /></label></>
                : <><label className="field-label">设计面名称<input value={currentPage.name} readOnly /></label><label className="field-label page-slug">等待 Scene 页面 ID<input value="由 AI Plan 创建" readOnly /></label></>}</>}
              <div className="panel-title section-title">视口与页面</div>
              {sceneDocument && activeWorkspaceArtboard ? <>
                <div className="size-row"><NumberField label="画板宽" value={activeWorkspaceArtboard.viewportWidth} min={240} max={10000} onChange={(width) => updateActiveWorkspaceViewport(Math.max(240, width), activeWorkspaceArtboard.viewportHeight)} /><NumberField label="首屏高" value={activeWorkspaceArtboard.viewportHeight} min={240} max={50000} onChange={(height) => updateActiveWorkspaceViewport(activeWorkspaceArtboard.viewportWidth, Math.max(240, height))} /></div>
                <p className="helper-text viewport-helper">画板尺寸只描述这个页面或弹层的设计表面；实际内容高度由 Scene 节点自动增长。</p>
              </> : <div className="scene-sidebar-empty"><strong>等待 Scene 画板</strong><span>画板尺寸会在 AI 开始当前页时建立，内容边界随后由真实 Scene 节点自动增长。</span></div>}

              <div className="panel-title section-title layer-title"><span>图片资源</span><small>{document.assets?.length ?? 0}</small></div>
              <input ref={assetInput} className="asset-input" type="file" accept="image/*" multiple onChange={(event) => void importAssets(event.target.files).catch((error) => showToast(String(error)))} />
              <button className="secondary-button" onClick={() => assetInput.current?.click()}>导入图片</button>
              <div className="asset-grid">{(document.assets ?? []).map((asset) => <button key={asset.id} title={`使用 ${asset.name}`} onClick={() => useAsset(asset)}><img src={asset.dataUrl} alt={asset.name} /><span>{asset.name}</span></button>)}</div>
            </>}

            {workspaceShell.activeArea === 'variables' && <div className="workspace-sidebar-section variables-sidebar">
              <div className="panel-intro"><strong>Scene Variables</strong><span>这里编辑的就是 Scene v2 Variable Collections 与 Modes，不再维护另一份旧 token 数据。</span></div>
              {sceneDocument ? <>
                <div className="panel-title layer-title"><span>变量集合</span><small>{sceneDocument.variableCollections.length}</small></div>
                <div className="scene-variable-summary">{sceneDocument.variableCollections.map((collection) => <article key={collection.id}><strong>{collection.name}</strong><span>{collection.modes.length} 个模式 · {collection.variables.length} 个变量</span></article>)}</div>
                <label className="field-label">完整变量数据<textarea className="scene-variable-editor" rows={16} spellCheck={false} value={sceneVariablesDraft} onChange={(event) => setSceneVariablesDraft(event.target.value)} /></label>
                <button className="secondary-button" onClick={() => void applySceneVariablesDraft()}>应用 Scene 变量</button>
                <button className="quiet-button variables-theme-button" onClick={() => setThemePickerOpen(true)}>从视觉风格建立变量</button>
              </> : <div className="scene-sidebar-empty"><strong>还没有 Scene 变量</strong><span>AI 开始第一个页面后，可在这里管理颜色、排版、圆角和响应式模式。</span></div>}
            </div>}

            {workspaceShell.activeArea === 'ai' && <div className="workspace-sidebar-section ai-tasks-sidebar">
              <div className="panel-intro"><strong>AI 视觉设计</strong><span>计划、候选截图、视觉 Diff 与人工批注都绑定 Scene 稳定节点；AI 一次只推进一个有界步骤。</span></div>
              <div className="scene-ai-sidebar-progress">{renderGenerationReviewPanel()}</div>
              <div className="panel-title layer-title"><span>待处理视觉批注</span><small>{sceneAnnotationTasks.length}</small></div>
              <div className="ai-sidebar-request-list">{sceneAnnotationTasks.map(({ node, annotation }) => <article key={annotation.id}><strong>{node.name}</strong><p>{annotation.body}</p><small>Scene r{sceneDocument?.revision} · {new Date(annotation.createdAt).toLocaleString()}</small><button disabled={sceneAnnotationPreparingId === annotation.id} onClick={() => void prepareSceneAnnotation(node.id, annotation.id)}>准备视觉上下文</button></article>)}</div>
              {sceneAnnotationTasks.length === 0 && <div className="ai-sidebar-empty"><span>✓</span><strong>没有待处理视觉批注</strong><p>{sceneDocument ? '选择图层后写下具体的构图、层级、留白、字体或图片问题。' : '先让 AI 规划一个页面并开始第一个视觉步骤。'}</p></div>}
              <div className="panel-title section-title">给 AI 一个小任务</div>
              <div className="ai-quick-prompts sidebar-prompts">{aiQuickPrompts.map((prompt) => <button key={prompt} onClick={() => setAiInstruction(prompt)}>{prompt}</button>)}</div>
              <textarea className="composer" rows={5} value={aiInstruction} onChange={(event) => setAiInstruction(event.target.value)} placeholder={selectedSceneNode ? `描述“${selectedSceneNode.name}”需要改好的视觉问题…` : sceneAiTarget ? `描述“${activeScenePage?.name ?? '当前页面'}”需要改好的整体视觉问题…` : 'AI 建立 Scene 后，可在这里对页面或具体图层发起视觉任务。'} />
              <button className="ai-button" disabled={!aiInstruction.trim() || !sceneAiTarget || Boolean(sceneAnnotationPreparingId)} onClick={() => void submitSceneAiInstruction()}>{sceneAnnotationPreparingId ? '正在生成截图…' : '提交视觉任务'}</button>
            </div>}
          </div>
          <WorkspacePanelResizeHandle side="left" width={workspaceShell.leftPanelWidth} onResize={(width) => dispatchWorkspaceShell({ type: 'resize-left-panel', width })} />
        </aside>}

        <section ref={canvasStage} className="canvas-stage">
          {!preview && <div className="canvas-toolbar device-toolbar">
            {editingSlot && editingContainer && editingSlotDefinition ? <>
              <button className="slot-editor-back" onClick={exitSlotEditor}>‹ 返回页面</button>
              <span className="slot-editor-path"><b>{editingContainer.library?.component}</b><i>/</i>{editingSlotDefinition.label}</span>
              <span className="toolbar-divider" />
              <button title="缩小内部画布" onClick={() => setCanvasZoom(zoom / 1.2)}>−</button>
              <span className="zoom-value">{Math.round(zoom * 100)}%</span>
              <button title="放大内部画布" onClick={() => setCanvasZoom(zoom * 1.2)}>＋</button>
              <span className="toolbar-divider" />
              <button className="fit-button" disabled={selectedIds.length === 0} title="将内部选中的一个或多个组件放到可见区域中心" onClick={fitWorkspaceSelection}>适应选择</button>
              <button className="fit-button" title="完整显示当前可编辑内容区域" onClick={fitSlotEditorContent}>适应内容</button>
              <button className="fit-button" title="恢复内部画布为 100%" onClick={() => setCanvasZoom(1)}>100%</button>
              <span className="toolbar-divider" />
              <button className="fit-button" onClick={() => insertSlotTemplate('form')}>＋ 表单模板</button>
              <button className="fit-button" onClick={() => insertSlotTemplate('details')}>＋ 详情模板</button>
            </> : <>
              <div className="device-switcher">
                {deviceOptions.map((item) => <button key={item.device} className={device === item.device ? 'active' : ''} title={item.label} onClick={() => switchDevice(item.device)}>{item.icon}<span>{item.label}</span></button>)}
              </div>
              <span className="toolbar-divider" />
              <select className="viewport-preset-select" aria-label="预览分辨率" value={viewportSelection.presetId ?? 'custom'} onChange={(event) => selectViewportPreset(event.target.value)}>
                {!viewportSelection.presetId && <option value="custom">自定义 · {breakpoint.width} × {previewViewportHeight}</option>}
                <optgroup label="常用 CSS 视口">{viewportPresets.filter((preset) => !preset.group).map((preset) => <option key={preset.id} value={preset.id}>{preset.label} · {preset.width} × {preset.height}</option>)}</optgroup>
                {viewportPresets.some((preset) => preset.group === 'large-display') && <optgroup label="超宽与原生高分辨率">{viewportPresets.filter((preset) => preset.group === 'large-display').map((preset) => <option key={preset.id} value={preset.id}>{preset.label} · {preset.width} × {preset.height}</option>)}</optgroup>}
              </select>
              <button className="rotate-viewport-button" title="旋转视口" onClick={rotateViewport}>↻</button>
              <button className="fit-button responsive-generate-button" disabled={!sceneDocument} title="切换平板或手机后，对选中 Scene 图层设置该断点的布局覆盖；始终复用同一节点" onClick={() => { activateWorkspaceArea('layers'); showToast('切换到平板或手机，选中图层后在右侧编辑该断点布局'); }}>响应式编辑</button>
              <span className="toolbar-divider" />
              <div className="new-surface-control">
                <select aria-label="新画板类型" value={newSurfaceKind} onChange={(event) => setNewSurfaceKind(event.target.value as WorkspaceSurfaceKind)}>
                  {(Object.keys(WORKSPACE_SURFACE_LABELS) as WorkspaceSurfaceKind[]).map((kind) => <option key={kind} value={kind}>{WORKSPACE_SURFACE_LABELS[kind]}</option>)}
                </select>
                <button className="fit-button artboard-action-button" disabled={!sceneDocument} title="创建一个独立的页面、弹层或界面状态画板" onClick={() => void addWorkspaceSurface()}>＋ 画板</button>
              </div>
              <button className="fit-button artboard-action-button" disabled={(workspacePlacement?.artboards.length ?? 0) <= 1} title="从工作区移除当前画板，不删除其设计内容" onClick={removeActiveWorkspaceArtboard}>移出工作区</button>
              <button className="fit-button artboard-action-button" title="在工作区中显示全部页面与界面状态" onClick={fitAllWorkspaceArtboards}>显示全部</button>
              <button className={`fit-button prototype-flow-toggle ${prototypeLinksVisible ? 'active' : ''}`} title="显示或隐藏组件到目标画板的原型关系" onClick={() => setPrototypeLinksVisible((visible) => !visible)}>流程线 {prototypeConnections.length}</button>
              <span className="toolbar-divider" />
              <button title="缩小" onClick={() => setCanvasZoom(zoom / 1.2)}>−</button><span className="zoom-value">{Math.round(zoom * 100)}%</span><button title="放大" onClick={() => setCanvasZoom(zoom * 1.2)}>＋</button>
              <span className="toolbar-divider" />
              <button className="fit-button" disabled={selectedIds.length === 0} title="将当前选中的一个或多个组件放到可见区域中心" onClick={fitWorkspaceSelection}>适应选择</button>
              <button className="fit-button" title="完整显示当前画板及其实际内容" onClick={fitActiveWorkspaceArtboard}>适应画板</button>
              <button className="fit-button" title="恢复 100%" onClick={() => setCanvasZoom(1)}>100%</button>
              <span className="toolbar-divider" /><button className={`fit-button interaction-mode-button ${interactionMode ? 'active' : ''}`} title="操作输入框、选择器、抽屉、标签页等真实组件" onClick={toggleInteractionMode}>{interactionMode ? '退出交互' : '交互'}</button>
            </>}
          </div>}
          {preview && <button className="exit-fullscreen-preview" onClick={toggleFullPreview}>退出预览 <span>Esc</span></button>}
          {interactionMode && !preview && <div className="interaction-mode-banner"><span>●</span> 交互模式：可以输入、选择、展开和打开弹层；退出后继续拖动编辑</div>}
          {preview && routePages.length > 1 && <nav className="route-preview-bar">{routePages.map((page) => <button key={page.id} className={page.id === pageId ? 'active' : ''} onClick={() => switchPage(page.id)}><span>{page.name}</span><small>{page.slug}</small></button>)}</nav>}
          {selected && !preview && <div className="selection-toolbar">
            <button title="左对齐" onClick={() => alignSelected('left')}>⇤</button><button title="水平居中" onClick={() => alignSelected('center')}>↔</button><button title="右对齐" onClick={() => alignSelected('right')}>⇥</button>
            <button title="顶部对齐" onClick={() => alignSelected('top')}>↥</button><button title="垂直居中" onClick={() => alignSelected('middle')}>↕</button><button title="底部对齐" onClick={() => alignSelected('bottom')}>↧</button>
            <span /><button title="置于顶层" onClick={() => reorderSelected('front')}>⤒</button><button title="上移一层" onClick={() => reorderSelected('forward')}>↑</button><button title="下移一层" onClick={() => reorderSelected('backward')}>↓</button><button title="置于底层" onClick={() => reorderSelected('back')}>⤓</button>
            <span /><button title="复制 ⌘C" onClick={copySelected}>⧉</button><button title="粘贴 ⌘V" disabled={!clipboard} onClick={pasteClipboard}>▣</button>
            {selectedIds.length > 1 && <><span /><button className="wide-tool" title="创建可整体移动的分组 ⌘G" onClick={groupSelected}>创建分组</button></>}
            {canUngroup && <button className="wide-tool" title="取消当前分组 ⇧⌘G" onClick={ungroupSelected}>取消分组</button>}
          </div>}
          {selectedSceneNode && !preview && <div className="selection-toolbar scene-selection-toolbar">
            <span className="scene-selection-kind">{selectedSceneNode.type}</span>
            <button title="复制 Scene 图层 ⌘D" onClick={duplicateSceneSelection}>⧉</button>
            <button title="复制到剪贴板 ⌘C" onClick={copySceneSelection}>C</button>
            <button title="从剪贴板粘贴 ⌘V" disabled={sceneClipboard.length === 0} onClick={pasteSceneClipboard}>V</button>
            {selectedIds.length > 1 && <>
              <span />
              <button title="左对齐" onClick={() => void alignSceneSelection('left')}>⇤</button>
              <button title="水平居中对齐" onClick={() => void alignSceneSelection('horizontal-center')}>↔</button>
              <button title="右对齐" onClick={() => void alignSceneSelection('right')}>⇥</button>
              <button title="顶部对齐" onClick={() => void alignSceneSelection('top')}>↥</button>
              <button title="垂直居中对齐" onClick={() => void alignSceneSelection('vertical-center')}>↕</button>
              <button title="底部对齐" onClick={() => void alignSceneSelection('bottom')}>↧</button>
            </>}
            {selectedIds.length > 2 && <>
              <button className="wide-tool" title="水平等间距分布" onClick={() => void distributeSceneSelection('horizontal')}>水平分布</button>
              <button className="wide-tool" title="垂直等间距分布" onClick={() => void distributeSceneSelection('vertical')}>垂直分布</button>
            </>}
            <span />
            <button title="置于顶层" onClick={() => void reorderSceneSelection('front')}>⤒</button>
            <button title="上移一层" onClick={() => void reorderSceneSelection('forward')}>↑</button>
            <button title="下移一层" onClick={() => void reorderSceneSelection('backward')}>↓</button>
            <button title="置于底层" onClick={() => void reorderSceneSelection('back')}>⤓</button>
            {selectedIds.length > 1 && <>
              <span />
              <button className="wide-tool" title="把选中图层组成可整体移动的 Group" onClick={() => void wrapSceneSelection('group')}>Group</button>
              <button className="wide-tool" title="用带内边距的 Frame 包住选中图层" onClick={() => void wrapSceneSelection('frame')}>Frame</button>
              <button title="横向 Auto Layout" onClick={() => void wrapSceneSelection('auto-horizontal')}>⇥</button>
              <button title="纵向 Auto Layout" onClick={() => void wrapSceneSelection('auto-vertical')}>⇣</button>
            </>}
            {(selectedSceneNode.type === 'group' || selectedSceneNode.type === 'frame') && selectedSceneNode.layout.mode === 'free'
              && <button className="wide-tool" title="取消当前容器" onClick={() => void ungroupSceneSelection()}>取消容器</button>}
            <button title="删除选中图层" onClick={() => void deleteSceneSelection()}>⌫</button>
          </div>}
          {selectionCandidatePopover && !preview && <div className="selection-candidate-popover" style={{
            left: Math.max(8, Math.min(window.innerWidth - 248, selectionCandidatePopover.clientX + 12)),
            top: Math.max(8, Math.min(window.innerHeight - 300, selectionCandidatePopover.clientY + 12))
          }} onPointerDown={(event) => event.stopPropagation()}>
            <header><strong>选择这个位置的图层</strong><span>{selectionCandidatePopover.candidates.length} 个重叠对象</span></header>
            <div>{selectionCandidatePopover.candidates.map((candidate, index) => <button key={candidate.id} onClick={() => {
              selectComponent(candidate.id);
              setSelectionCandidatePopover(undefined);
            }}><span>{index + 1}</span><div><strong>{candidate.name}</strong><small>{candidate.type}{candidate.depth > 0 ? ` · 第 ${candidate.depth + 1} 层` : ' · 外层'}</small></div>{candidate.locked && <em>已锁定</em>}</button>)}</div>
            <footer>Command / Ctrl 点击可再次查看候选</footer>
          </div>}
          <div
            ref={canvasScroll}
            className={`canvas-scroll ${preview ? 'preview-canvas-scroll' : 'workspace-camera-viewport'} ${editingSlot ? 'slot-editor-scroll' : ''} ${canvasPanReady || workspaceShell.activeTool === 'hand' ? 'pan-ready' : ''} ${canvasPanning ? 'panning' : ''}`}
            style={!preview ? {
              '--workspace-grid-size': `${16 * zoom}px`,
              '--workspace-grid-x': `${workspaceCamera.x}px`,
              '--workspace-grid-y': `${workspaceCamera.y}px`
            } as CSSProperties : undefined}
            onPointerDown={beginCanvasPan}
          >
          {editingSlot && editingContainer && editingSlotDefinition && editingSlotCanvasSize ? <div className="slot-editor-camera-world" style={{ transform: `translate3d(${workspaceCamera.x}px,${workspaceCamera.y}px,0) scale(${zoom})` }}><div className="slot-editor-frame">
              <div className="slot-editor-heading"><div><span>可编辑内容区域</span><strong>{editingSlotDefinition.label}</strong><small>{editingSlotDefinition.description}</small></div><em>{Math.round(editingSlotCanvasSize.width)} × {Math.round(editingSlotCanvasSize.height)}</em></div>
              <div className="slot-editor-canvas-shell">
                <div className="slot-design-canvas design-canvas" style={{ width: editingSlotCanvasSize.width, height: editingSlotCanvasSize.height }} onDragOver={(event) => event.preventDefault()} onDrop={onCanvasDrop} onPointerDown={beginCanvasMarquee}>
                  {editingSlotComponents.length === 0 && <div className="slot-empty-state"><span>＋</span><strong>从左侧拖入组件</strong><p>也可以先插入表单或详情模板，再逐项调整。</p><div><button onPointerDown={(event) => event.stopPropagation()} onClick={() => insertSlotTemplate('form')}>插入表单</button><button onPointerDown={(event) => event.stopPropagation()} onClick={() => insertSlotTemplate('details')}>插入详情</button></div></div>}
                  {editingVisibleComponents.sort((left, right) => left.zIndex - right.zIndex).map((component) => {
                    const frame = resolveComponent(component, device);
                    const containerFrame = resolveComponent(editingContainer, device);
                    const resolved = { ...frame, x: frame.x - containerFrame.x, y: frame.y - containerFrame.y };
                    if (resolved.hidden) return null;
                    const editableSlot = editableSlotsForUiComponent(component)[0];
                    return <WorkspaceCanvasComponent key={component.id} component={component} resolved={resolved} selected={selectedIdSet.has(component.id)} primary={component.id === selectedId} interactive={false} forcedState={component.id === selectedId && inspectorVisualState !== 'default' ? inspectorVisualState : undefined} tokens={tokens} slotContent={runtimeSlotContentMap(document, component, device, false, tokens, activatePreviewInteraction)} onPointerDown={(event) => beginInteraction(event, component, 'move')} onResizePointerDown={(event) => beginInteraction(event, component, 'resize')} onPreviewActivate={() => activatePreviewInteraction(component)} onEditContents={editableSlot ? () => void editComponentSlot(component, editableSlot.id) : undefined} />;
                  })}
                  <SelectionOverlay items={selectionOverlayItemsFor(editingVisibleComponents, device, resolveComponent(editingContainer, device))} marqueeRect={marqueeRect} onResizePointerDown={(componentId, _handle, event) => {
                    const component = document.components.find((candidate) => candidate.id === componentId);
                    if (component) beginInteraction(event, component, 'resize');
                  }} />
                </div>
              </div>
            </div></div> : preview ? <div className="preview-canvas-board"><div className="canvas-scale" style={{
              width: scaledCanvasWidth,
              height: scaledCanvasHeight
            }}>
              <div className={`design-canvas device-${device}`} style={{
                width: breakpoint.width,
                height: renderedCanvasHeight,
                background: document.viewport.background,
                transform: `scale(${zoom})`,
                fontFamily: tokens?.typography.fontFamily,
                fontSize: tokens?.typography.baseFontSize,
                '--color-primary': tokens?.colors.primary,
                '--color-accent': tokens?.colors.accent,
                '--color-surface': tokens?.colors.surface,
                '--color-text': tokens?.colors.text,
                '--color-muted': tokens?.colors.muted,
                '--radius-small': `${tokens?.radii.small ?? 8}px`,
                '--radius-medium': `${tokens?.radii.medium ?? 16}px`,
                '--radius-large': `${tokens?.radii.large ?? 28}px`
              } as CSSProperties} onDragOver={(event) => event.preventDefault()} onDrop={onCanvasDrop} onPointerDown={() => { if (!interactionMode && workspaceShell.activeTool !== 'hand' && workspaceShell.activeTool !== 'comment') { setSelectedId(undefined); setSelectedIds([]); } }}>
                {sceneDocument ? <SceneArtboardCanvas
                  scene={sceneDocument}
                  pageId={pageId}
                  viewportWidth={breakpoint.width}
                  viewportHeight={previewViewportHeight}
                  active={false}
                  interactive
                  selectedIds={[]}
                  onSelectionChange={() => undefined}
                  onCommit={commitSceneCommand}
                  onError={showToast}
                  onPrototypeActivate={(link) => activateScenePrototype(link)}
                /> : <div className="scene-v2-load-state missing"><strong>还没有可预览的 Scene 页面</strong><span>先让 AI 完成一个视觉步骤，再进入全屏预览。</span></div>}
              </div>
            </div>{renderPreviewSurfaceOverlay()}</div> : <div className="workspace-camera-world" style={{ transform: `translate3d(${workspaceCamera.x}px,${workspaceCamera.y}px,0) scale(${zoom})` }}>
              {prototypeLinksVisible && prototypeConnections.length > 0 && <svg className="prototype-flow-layer" aria-hidden="true">
                <defs><marker id="prototype-flow-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" /></marker></defs>
                {prototypeConnections.map((connection) => <g key={connection.id} className={connection.componentId === selectedId ? 'active' : ''}>
                  <path className="prototype-flow-halo" d={prototypeFlowPath(connection)} />
                  <path className="prototype-flow-path" d={prototypeFlowPath(connection)} markerEnd="url(#prototype-flow-arrow)" />
                  <circle cx={connection.start.x} cy={connection.start.y} r="6" />
                  <text x={connection.label.x} y={connection.label.y - 10}>{WORKSPACE_SURFACE_LABELS[connection.targetSurfaceKind]}</text>
                </g>)}
              </svg>}
              {workspacePlacement?.artboards.map(renderWorkspaceArtboard)}
            </div>}
          </div>
          {!preview && <WorkspaceBottomToolbar activeTool={workspaceShell.activeTool} leftPanelOpen={workspaceShell.leftPanelOpen} rightPanelOpen={workspaceShell.rightPanelOpen} canvasMaximized={workspaceShell.canvasMaximized} onSelectTool={activateWorkspaceTool} onToggleLeft={() => dispatchWorkspaceShell({ type: 'toggle-left-panel' })} onToggleRight={() => dispatchWorkspaceShell({ type: 'toggle-right-panel' })} onToggleMaximize={() => dispatchWorkspaceShell({ type: 'toggle-canvas-maximized' })} />}
        </section>

        {!preview && workspaceShell.rightPanelOpen && <aside className="inspector-panel">
          <WorkspacePanelResizeHandle side="right" width={workspaceShell.rightPanelWidth} onResize={(width) => dispatchWorkspaceShell({ type: 'resize-right-panel', width })} />
          <div className="inspector-review-switch">
            <button className={inspectorTab === 'review' ? 'active' : ''} onClick={() => setInspectorTab(inspectorTab === 'review' ? 'design' : 'review')}><span>✦</span><strong>AI 设计进度</strong>{generationPlan?.activeStep?.status === 'awaiting-review' && <em>待审阅</em>}</button>
          </div>
          {inspectorTab === 'review' ? renderGenerationReviewPanel() : selectedSceneNode ? <>
            <div className="inspector-heading"><div><span className="eyebrow">Scene v2 · {selectedIds.length > 1 ? `${selectedIds.length} 项` : selectedSceneNode.type}</span><strong>{selectedSceneNode.name}</strong></div><span className="scene-revision-badge">r{sceneDocument?.revision}</span></div>
            <div className="inspector-actions">
              <button onClick={duplicateSceneSelection}>复制 ⌘D</button>
              <button onClick={saveSceneSelectionAsSnippet}>保存到“我的”</button>
              <button className={selectedSceneNode.locked ? 'active' : ''} onClick={() => void updateSceneNode([{ path: ['locked'], value: !selectedSceneNode.locked }])}>{selectedSceneNode.locked ? '解锁' : '锁定'}</button>
              <button className={!selectedSceneNode.visible ? 'active' : ''} onClick={() => void updateSceneNode([{ path: ['visible'], value: !selectedSceneNode.visible }])}>{selectedSceneNode.visible ? '隐藏' : '显示'}</button>
              {(selectedSceneNode.type === 'group' || selectedSceneNode.type === 'frame') && selectedSceneNode.layout.mode === 'free' && <button onClick={() => void ungroupSceneSelection()}>取消容器</button>}
            </div>
            <div className="inspector-mode-tabs scene-inspector-tabs" role="tablist" aria-label="Scene 属性栏模式">
              <button role="tab" aria-selected={inspectorTab === 'design'} className={inspectorTab === 'design' ? 'active' : ''} onClick={() => setInspectorTab('design')}>设计</button>
              <button role="tab" aria-selected={inspectorTab === 'prototype'} className={inspectorTab === 'prototype' ? 'active' : ''} onClick={() => setInspectorTab('prototype')}>原型</button>
              <button role="tab" aria-selected={inspectorTab === 'ai'} className={inspectorTab === 'ai' ? 'active' : ''} onClick={() => setInspectorTab('ai')}>批注与 AI</button>
            </div>
            {inspectorTab === 'design' && <>
            <div className="panel-title section-title">图层</div>
            <label className="field-label">名称<input key={`${selectedSceneNode.id}:${selectedSceneNode.name}`} defaultValue={selectedSceneNode.name} maxLength={240} onBlur={(event) => {
              const name = event.currentTarget.value.trim();
              if (name && name !== selectedSceneNode.name) void updateSceneNode([{ path: ['name'], value: name }]);
            }} /></label>
            {selectedSceneNode.type === 'text' && <label className="field-label">文字内容<textarea key={`${selectedSceneNode.id}:${selectedSceneNode.content}`} rows={4} defaultValue={selectedSceneNode.content} onBlur={(event) => {
              if (event.currentTarget.value !== selectedSceneNode.content) void updateSceneNode([{ path: ['content'], value: event.currentTarget.value }]);
            }} /></label>}
            {selectedSceneNode.type === 'library-instance' && <section className="scene-library-inspector">
              <header><div><span>官方组件</span><strong>{selectedSceneLibrary?.displayName ?? selectedSceneNode.library} · {selectedSceneLibraryDefinition?.label ?? selectedSceneNode.component}</strong></div><em>{selectedSceneLibrary?.version ?? 'runtime'}</em></header>
              <div className="scene-library-binding-grid"><label className="field-label">组件库<input value={selectedSceneNode.library} readOnly /></label><label className="field-label">组件<input value={selectedSceneNode.component} readOnly /></label></div>
              <label className="field-label">官方款式<select value={selectedSceneNode.variant ?? selectedSceneLibraryVariants[0]?.id ?? 'default'} disabled={selectedSceneNode.locked || selectedSceneLibraryVariants.length === 0} onChange={(event) => void applySelectedSceneLibraryVariant(event.target.value)}>{selectedSceneLibraryVariants.map((variant) => <option key={variant.id} value={variant.id}>{variant.label}</option>)}</select></label>
              <label className="field-label">展示内容<textarea key={`${selectedSceneNode.id}:${selectedSceneNode.content ?? ''}`} rows={3} defaultValue={selectedSceneNode.content ?? ''} disabled={selectedSceneNode.locked} onBlur={(event) => {
                if (event.currentTarget.value !== (selectedSceneNode.content ?? '')) void updateSceneNode([{ path: ['content'], value: event.currentTarget.value }], '用户修改 Scene 官方组件内容。');
              }} /></label>
              <div className="scene-library-runtime-actions"><button disabled={selectedSceneNode.locked || !selectedSceneLibrary} onClick={() => selectedSceneLibrary && setVariantPickerTarget({ library: selectedSceneLibrary.id, componentId: selectedSceneNode.component, replaceComponentId: selectedSceneNode.id })}>{selectedSceneRegistryElement ? '重新选择官方元素' : '浏览官方示例与元素'}</button>{selectedSceneRegistryElement && <span>{selectedSceneRegistryElement.label}</span>}</div>
              <JsonObjectEditor label="组件属性 / 示例数据" value={selectedSceneNode.properties} disabled={selectedSceneNode.locked} onChange={(value) => void updateSceneNode([{ path: ['properties'], value }], '用户修改 Scene 官方组件属性和示例数据。')} />
              {selectedSceneEditableSlots.length > 0 && <div className="scene-content-slots"><div className="panel-title section-title">内部内容区</div><p className="helper-text">进入内容区后，左侧拖入或点击插入的官方组件会成为当前组件的真实 Scene 子层，不会生成另一套编辑器数据。</p>{selectedSceneEditableSlots.map((slot) => {
                const activeSlot = Boolean(sceneContentFocus && sceneContentFocus.pageId === selectedSceneEntry?.pageId && sceneContentFocus.nodeId === selectedSceneNode.id && sceneContentFocus.slot === slot.id);
                return <button key={slot.id} className={activeSlot ? 'active' : ''} onClick={() => activeSlot ? setSceneContentFocus(undefined) : focusSceneContent(selectedSceneNode.id, slot.id)}><span><strong>{slot.label}</strong><small>{slot.description}</small></span><em>{selectedSceneNode.slots[slot.id]?.length ?? 0} 层</em><b>{activeSlot ? '退出' : '进入编辑'}</b></button>;
              })}</div>}
            </section>}
            {isSceneContainer(selectedSceneNode) && selectedSceneNode.type !== 'component-set' && <section className="scene-container-focus-card"><div><strong>容器内部编辑</strong><span>把后续组件直接放入这个 {selectedSceneNode.type === 'group' ? 'Group' : 'Frame'}</span></div><button className={sceneContentFocus?.nodeId === selectedSceneNode.id && !sceneContentFocus.slot ? 'active' : ''} onClick={() => sceneContentFocus?.nodeId === selectedSceneNode.id && !sceneContentFocus.slot ? setSceneContentFocus(undefined) : focusSceneContent(selectedSceneNode.id)}>{sceneContentFocus?.nodeId === selectedSceneNode.id && !sceneContentFocus.slot ? '退出内部编辑' : '进入内部编辑'}</button></section>}
            <div className="size-row four">
              <SceneNumberField label="X" value={selectedSceneNode.frame.x} disabled={!selectedScenePositionEditable || selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['frame', 'x'], value }])} />
              <SceneNumberField label="Y" value={selectedSceneNode.frame.y} disabled={!selectedScenePositionEditable || selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['frame', 'y'], value }])} />
              <SceneNumberField label="W" value={selectedSceneNode.frame.width} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['frame', 'width'], value }])} />
              <SceneNumberField label="H" value={selectedSceneNode.frame.height} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['frame', 'height'], value }])} />
            </div>
            {!selectedScenePositionEditable && <p className="helper-text">这个图层由父级 Auto Layout / Grid 排布，X、Y 位置由布局计算。</p>}
            <div className="panel-title section-title">布局</div>
            <label className="field-label">布局方式<select value={selectedSceneNode.layout.mode} disabled={selectedSceneNode.locked} onChange={(event) => void updateSceneNode([{ path: ['layout', 'mode'], value: event.target.value }])}><option value="free">自由布局</option><option value="auto">Auto Layout</option><option value="grid">Grid</option></select></label>
            {selectedSceneNode.layout.mode === 'auto' && <label className="field-label">方向<select value={selectedSceneNode.layout.direction ?? 'vertical'} disabled={selectedSceneNode.locked} onChange={(event) => void updateSceneNode([{ path: ['layout', 'direction'], value: event.target.value }])}><option value="horizontal">横向</option><option value="vertical">纵向</option></select></label>}
            <div className="size-row">
              <label className="field-label">水平尺寸<select value={selectedSceneNode.layout.sizingX} disabled={selectedSceneNode.locked} onChange={(event) => void updateSceneNode([{ path: ['layout', 'sizingX'], value: event.target.value }])}><option value="fixed">固定</option><option value="hug">适应内容</option><option value="fill">填满</option></select></label>
              <label className="field-label">垂直尺寸<select value={selectedSceneNode.layout.sizingY} disabled={selectedSceneNode.locked} onChange={(event) => void updateSceneNode([{ path: ['layout', 'sizingY'], value: event.target.value }])}><option value="fixed">固定</option><option value="hug">适应内容</option><option value="fill">填满</option></select></label>
            </div>
            <div className="size-row four">
              <SceneNumberField label="上" value={selectedSceneNode.layout.padding.top} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'padding', 'top'], value }])} />
              <SceneNumberField label="右" value={selectedSceneNode.layout.padding.right} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'padding', 'right'], value }])} />
              <SceneNumberField label="下" value={selectedSceneNode.layout.padding.bottom} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'padding', 'bottom'], value }])} />
              <SceneNumberField label="左" value={selectedSceneNode.layout.padding.left} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'padding', 'left'], value }])} />
            </div>
            <div className="size-row"><SceneNumberField label="行间距" value={selectedSceneNode.layout.gap.row} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'gap', 'row'], value }])} /><SceneNumberField label="列间距" value={selectedSceneNode.layout.gap.column} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'gap', 'column'], value }])} /></div>
            {sceneResponsiveRuleSpec && <section className="scene-responsive-editor">
              <header><div><strong>{device === 'mobile' ? '手机' : '平板'}布局覆盖</strong><span>仍使用同一棵 Scene，不复制组件</span></div><button disabled={!selectedSceneResponsiveOverride} onClick={() => void clearSelectedSceneResponsiveOverride()}>恢复继承</button></header>
              <label className="field-label">此断点可见性<select value={selectedSceneResponsiveOverride?.visible === undefined ? 'inherit' : selectedSceneResponsiveOverride.visible ? 'visible' : 'hidden'} disabled={selectedSceneNode.locked} onChange={(event) => {
                const value = event.target.value;
                if (value === 'inherit') {
                  const next: Omit<SceneResponsiveNodeOverride, 'nodeId'> = structuredClone(selectedSceneResponsiveOverride ?? {});
                  delete next.visible;
                  void replaceSelectedSceneResponsiveOverride(next);
                } else void updateSelectedSceneResponsiveOverride({ visible: value === 'visible' });
              }}><option value="inherit">继承基础设计</option><option value="visible">强制显示</option><option value="hidden">在此断点隐藏</option></select></label>
              <div className="size-row">
                <label className="field-label">水平尺寸<select value={selectedSceneResponsiveOverride?.layout?.sizingX ?? 'inherit'} disabled={selectedSceneNode.locked} onChange={(event) => {
                  const layout = structuredClone(selectedSceneResponsiveOverride?.layout ?? {});
                  if (event.target.value === 'inherit') delete layout.sizingX;
                  else layout.sizingX = event.target.value as 'fixed' | 'hug' | 'fill';
                  const next = { ...structuredClone(selectedSceneResponsiveOverride ?? {}), ...(Object.keys(layout).length ? { layout } : {}) };
                  if (!Object.keys(layout).length) delete next.layout;
                  void replaceSelectedSceneResponsiveOverride(next);
                }}><option value="inherit">继承</option><option value="fixed">固定</option><option value="hug">适应内容</option><option value="fill">填满可用宽度</option></select></label>
                <label className="field-label">垂直尺寸<select value={selectedSceneResponsiveOverride?.layout?.sizingY ?? 'inherit'} disabled={selectedSceneNode.locked} onChange={(event) => {
                  const layout = structuredClone(selectedSceneResponsiveOverride?.layout ?? {});
                  if (event.target.value === 'inherit') delete layout.sizingY;
                  else layout.sizingY = event.target.value as 'fixed' | 'hug' | 'fill';
                  const next = { ...structuredClone(selectedSceneResponsiveOverride ?? {}), ...(Object.keys(layout).length ? { layout } : {}) };
                  if (!Object.keys(layout).length) delete next.layout;
                  void replaceSelectedSceneResponsiveOverride(next);
                }}><option value="inherit">继承</option><option value="fixed">固定</option><option value="hug">适应内容</option><option value="fill">填满可用高度</option></select></label>
              </div>
              {isSceneContainer(selectedSceneNode) && <>
                <label className="field-label">内容方向<select value={selectedSceneResponsiveOverride?.layout?.direction ?? 'inherit'} disabled={selectedSceneNode.locked} onChange={(event) => {
                  const layout = structuredClone(selectedSceneResponsiveOverride?.layout ?? {});
                  if (event.target.value === 'inherit') delete layout.direction;
                  else layout.direction = event.target.value as 'horizontal' | 'vertical';
                  const next = { ...structuredClone(selectedSceneResponsiveOverride ?? {}), ...(Object.keys(layout).length ? { layout } : {}) };
                  if (!Object.keys(layout).length) delete next.layout;
                  void replaceSelectedSceneResponsiveOverride(next);
                }}><option value="inherit">继承</option><option value="horizontal">横向</option><option value="vertical">纵向</option></select></label>
                {selectedSceneNode.children.length > 1 && <div className="scene-responsive-order"><strong>此断点子层顺序</strong><span>只改变排列顺序，不复制或删除图层</span>{(selectedSceneResponsiveOverride?.childOrder ?? selectedSceneNode.children.map((child) => child.id)).map((childId, index, order) => {
                  const child = selectedSceneNode.children.find((candidate) => candidate.id === childId);
                  return <div key={childId}><span>{child?.name ?? childId}</span><button disabled={index === 0 || selectedSceneNode.locked} onClick={() => {
                    const next = [...order];
                    [next[index - 1], next[index]] = [next[index], next[index - 1]];
                    void updateSelectedSceneResponsiveOverride({ childOrder: next });
                  }}>↑</button><button disabled={index === order.length - 1 || selectedSceneNode.locked} onClick={() => {
                    const next = [...order];
                    [next[index], next[index + 1]] = [next[index + 1], next[index]];
                    void updateSelectedSceneResponsiveOverride({ childOrder: next });
                  }}>↓</button></div>;
                })}</div>}
              </>}
            </section>}
            <section className="scene-ai-policy-card"><header><strong>AI 编辑策略</strong><span>{selectedSceneNode.aiPolicy.editable ? '允许 AI 修改' : '仅人工修改'}</span></header>{selectedSceneNode.aiPolicy.intent && <p>{selectedSceneNode.aiPolicy.intent}</p>}<small>{selectedSceneNode.aiPolicy.lockedFields.length ? `保护字段：${selectedSceneNode.aiPolicy.lockedFields.join('、')}` : '没有单独保护的字段'}</small></section>
            </>}
            {inspectorTab === 'prototype' && <>
              <div className="panel-title section-title">画板连接</div>
              {selectedPrototypeTarget && <div className="prototype-relationship-card">
                <div className="prototype-relationship-node"><span>来源</span><strong>{selectedSceneNode.name}</strong><small>{pages.find((page) => page.id === selectedSceneEntry?.pageId)?.name ?? selectedSceneEntry?.pageId}</small></div>
                <div className="prototype-relationship-action"><i>→</i><span>{selectedSceneNode.prototypeLink?.action === 'navigate' ? '跳转' : `打开${WORKSPACE_SURFACE_LABELS[selectedPrototypeTarget.surfaceKind ?? 'page']}`}</span></div>
                <div className="prototype-relationship-node target"><span>目标</span><strong>{selectedPrototypeTarget.name}</strong><small>{WORKSPACE_SURFACE_LABELS[selectedPrototypeTarget.surfaceKind ?? 'page']}</small></div>
                <div className="prototype-relationship-buttons"><button onClick={() => focusWorkspaceArtboardByPageId(selectedPrototypeTarget.id)}>定位目标画板</button><button onClick={() => void updateSceneNode([{ path: ['prototypeLink'], value: null }], '用户移除 Scene 原型关系。')}>移除关系</button></div>
              </div>}
              <label className="field-label">点击行为<select value={selectedSceneNode.prototypeLink ? 'page' : 'none'} disabled={selectedSceneNode.locked} onChange={(event) => {
                if (event.target.value === 'none') {
                  void updateSceneNode([{ path: ['prototypeLink'], value: null }], '用户移除 Scene 原型关系。');
                  return;
                }
                const target = pages.find((candidate) => candidate.id !== selectedSceneEntry?.pageId);
                if (!target) {
                  showToast('请先创建另一个独立画板');
                  return;
                }
                const action = (target.surfaceKind ?? 'page') === 'page' ? 'navigate' : 'overlay';
                void updateSceneNode([{ path: ['prototypeLink'], value: { trigger: 'click', action, targetPageId: target.id } }], '用户创建 Scene 原型关系。');
              }}><option value="none">无连接</option><option value="page">连接到独立画板</option></select></label>
              {selectedSceneNode.prototypeLink && <label className="field-label interaction-target">目标画板<select value={selectedSceneNode.prototypeLink.targetPageId} disabled={selectedSceneNode.locked} onChange={(event) => {
                const target = pages.find((candidate) => candidate.id === event.target.value);
                if (!target) return;
                const action = (target.surfaceKind ?? 'page') === 'page' ? 'navigate' : 'overlay';
                void updateSceneNode([{ path: ['prototypeLink'], value: { trigger: 'click', action, targetPageId: target.id } }], '用户修改 Scene 原型目标。');
              }}>{pages.filter((candidate) => candidate.id !== selectedSceneEntry?.pageId).map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.name} · {WORKSPACE_SURFACE_LABELS[candidate.surfaceKind ?? 'page']}</option>)}</select></label>}
              <p className="helper-text inspector-prototype-help">页面、弹窗、抽屉、菜单与状态都作为独立画板设计。普通页面执行跳转；其他画板在来源页面上叠加预览。工作区会显示这条关系的箭头。</p>
            </>}
            {inspectorTab === 'ai' && <div className="scene-annotation-panel">
              <section className="scene-ai-target-card">
                <header><strong>AI 视觉目标</strong><span>r{sceneDocument?.revision}</span></header>
                <p>{activeScenePage?.name ?? pageId} / {selectedSceneNode.name}</p>
                <small>节点 {selectedSceneNode.id}</small>
              </section>
              <div className="panel-title section-title">图层批注</div>
              <div className="notes-list scene-notes-list">
                {selectedSceneNode.annotations.length === 0 && <span className="empty-hint">还没有批注。写下视觉问题后，AI 会按这个稳定节点逐步修改。</span>}
                {selectedSceneNode.annotations.map((note) => <article key={note.id} className={`note-card scene-note-card ${note.status}`}>
                  <span>{note.body}</span>
                  <small>{note.status === 'open' ? '待 AI 处理' : '已完成'} · {note.author} · {note.id}</small>
                  <div>
                    {note.status === 'open' ? <>
                      <button disabled={sceneAnnotationPreparingId === note.id} onClick={() => void prepareSceneAnnotation(selectedSceneNode.id, note.id)}>{sceneAnnotationPreparingId === note.id ? '正在生成截图…' : '准备给 AI'}</button>
                      <button onClick={() => void changeSceneAnnotationStatus(note.id, 'resolved')}>标记完成</button>
                    </> : <button onClick={() => void changeSceneAnnotationStatus(note.id, 'open')}>重新打开</button>}
                  </div>
                </article>)}
              </div>
              <textarea className="composer" rows={3} maxLength={4000} placeholder="例如：标题与按钮的视觉层级不够明确，请加强对比但保留当前布局。" value={annotationText} onChange={(event) => setAnnotationText(event.target.value)} />
              <button className="secondary-button" disabled={!annotationText.trim()} onClick={() => void addSceneAnnotation()}>只添加批注</button>
              <div className="panel-title section-title">直接交给 AI</div>
              <p className="helper-text">提交时会自动创建批注，并截取当前节点的真实画面、Scene revision 与节点坐标，AI 不需要猜元素。</p>
              <textarea className="composer" rows={4} maxLength={4000} placeholder="描述你希望 AI 下一步改善的视觉效果…" value={aiInstruction} onChange={(event) => setAiInstruction(event.target.value)} />
              <button className="ai-button" disabled={!aiInstruction.trim() || Boolean(sceneAnnotationPreparingId)} onClick={() => void submitSceneAiInstruction()}>{sceneAnnotationPreparingId ? '正在准备视觉上下文…' : '提交视觉任务'}</button>
              {sceneAiContext && <section className="scene-ai-context-card">
                {sceneAiContext.imageDataUrl && <img src={sceneAiContext.imageDataUrl} alt={`批注目标 ${selectedSceneNode.name}`} />}
                <div><strong>视觉上下文已就绪</strong><span>{sceneAiContext.capture.width} × {sceneAiContext.capture.height} · {sceneAiContext.capture.groundingCount} 个稳定节点</span></div>
                <dl><div><dt>页面</dt><dd>{sceneAiContext.task.pageId}</dd></div><div><dt>节点</dt><dd>{sceneAiContext.task.targetNodeId}</dd></div><div><dt>Scene</dt><dd>r{sceneAiContext.task.baseRevision}</dd></div><div><dt>截图</dt><dd>{sceneAiContext.capture.artifact.artifactId}</dd></div></dl>
              </section>}
            </div>}
          </> : selected && inspectedFrame ? <>
            <div className="inspector-heading"><div><span className="eyebrow">已选择 {selectedIds.length > 1 ? `${selectedIds.length} 项` : ''} · {device}</span><strong>{selected.name}</strong></div><button className="danger-link" onClick={deleteSelected}>删除</button></div>
            <div className="inspector-actions"><button onClick={duplicateSelected}>复制 ⌘D</button><button className={selected.locked ? 'active' : ''} onClick={() => toggleLocked(selected)}>{selected.locked ? '解锁' : '锁定'}</button><button className={inspectedFrame.hidden ? 'active' : ''} onClick={() => toggleHidden(selected)}>{inspectedFrame.hidden ? '显示' : '隐藏'}</button></div>
            <div className="inspector-mode-tabs" role="tablist" aria-label="属性栏模式">
              <button role="tab" aria-selected={inspectorTab === 'design'} className={inspectorTab === 'design' ? 'active' : ''} onClick={() => setInspectorTab('design')}>设计</button>
              <button role="tab" aria-selected={inspectorTab === 'prototype'} className={inspectorTab === 'prototype' ? 'active' : ''} onClick={() => setInspectorTab('prototype')}>原型</button>
              <button role="tab" aria-selected={inspectorTab === 'ai'} className={inspectorTab === 'ai' ? 'active' : ''} onClick={() => setInspectorTab('ai')}>批注与 AI</button>
            </div>
            {inspectorTab === 'design' && <>
            <button className="secondary-button save-symbol-button" onClick={saveSelectionAsSymbol}>保存到“我的”</button>
            {selectedSymbol && selected.symbolInstanceId && <div className="symbol-instance-panel">
              <div><span>实例来源</span><strong>{selectedSymbol.name}</strong></div>
              <label><input type="checkbox" checked={(selected.symbolOverrides ?? []).includes('content')} onChange={() => toggleSelectedSymbolOverride('content')} />保留内容</label>
              <label><input type="checkbox" checked={(selected.symbolOverrides ?? []).includes('style')} onChange={() => toggleSelectedSymbolOverride('style')} />保留样式</label>
              <label><input type="checkbox" checked={(selected.symbolOverrides ?? []).includes('frame')} onChange={() => toggleSelectedSymbolOverride('frame')} />保留位置尺寸</label>
              <div className="symbol-instance-actions"><button onClick={updateSelectedSymbolDefinition}>用当前实例更新定义</button><button onClick={synchronizeSelectedSymbol}>同步全部实例</button><button className="danger" onClick={detachSelectedSymbol}>脱离组件库</button></div>
            </div>}
            <label className="field-label">组件名称<input value={selected.name} onChange={(event) => updateSelected({ name: event.target.value })} /></label>
            {inspectorCapabilities?.content && <label className="field-label">{inspectorCapabilities.media ? '资源地址' : '内容'}<textarea rows={3} value={selected.content} onChange={(event) => updateSelected({ content: event.target.value })} /></label>}
            {inspectorCapabilities?.library && selected.library && selectedLibrary && <div className={`ui-library-inspector library-${selected.library.name}`}>
              <div className="panel-title section-title">{selectedLibrary.displayName} 组件</div>
              <div className="antd-binding-summary"><span>组件</span><strong>{selected.library.component}</strong><small>{selected.library.name === 'shadcn' ? selected.library.version : `v${selected.library.version}`}</small></div>
              {selectedLibraryDefinition?.docsUrl && <a className="ui-library-doc-link" href={selectedLibraryDefinition.docsUrl} target="_blank" rel="noreferrer">查看当前官网文档 ↗</a>}
              {selectedLibraryDefinition?.status === 'deprecated' && <div className="ui-library-deprecation-note">官网已将该组件标记为废弃；新设计建议使用 Listy。</div>}
              {selectedEditableSlots.length > 0 && <div className="content-slots-panel">
                <div className="content-slots-heading"><div><strong>内部内容</strong><span>像页面一样继续设计</span></div><em>{selectedEditableSlots.length} 个区域</em></div>
                {selectedEditableSlots.map((slot) => {
                  const count = componentsInSlot(document, selected.id, slot.id).length;
                  const officialDemo = Boolean(selected.library?.props.registryDemo);
                  return <button key={slot.id} className={editingSlot?.componentId === selected.id && editingSlot.slotId === slot.id ? 'active' : ''} onClick={() => void editComponentSlot(selected, slot.id)}><span><strong>{slot.label}</strong><small>{slot.description}</small></span><em>{count > 0 ? `${count} 个组件` : officialDemo ? '尚未拆分' : '空白'}</em><b>{officialDemo && count === 0 ? '拆开并编辑 ›' : '进入编辑 ›'}</b></button>;
                })}
              </div>}
              {selectedRegistryElement ? <div className="selected-registry-element-summary"><div><span>已选择的独立元素</span><strong>{selectedRegistryElement.label}</strong><small>{selected.library.variant ? `来源款式：${selectedLibraryVariants.find((variant) => variant.id === selected.library?.variant)?.label ?? selected.library.variant}` : '保留官方真实运行时'}</small></div><button onClick={() => setVariantPickerTarget({ library: selected.library!.name, componentId: selected.library!.component, replaceComponentId: selected.id })}>重新选择</button></div>
                : <label className="field-label ui-library-variant-field">展现款式<select value={selected.library.variant ?? selectedLibraryVariants[0]?.id} onChange={(event) => applySelectedLibraryVariant(event.target.value)}>{selectedLibraryVariants.map((variant) => <option key={variant.id} value={variant.id}>{variant.label}</option>)}</select></label>}
              {selectedInspectableLibraryProps.filter(([, value]) => ['string', 'number', 'boolean'].includes(typeof value)).map(([key, value]) => typeof value === 'boolean'
                ? <label key={key} className="ui-library-boolean-prop"><input type="checkbox" checked={value} onChange={(event) => updateSelectedLibraryProp(key, event.target.checked)} /><span>{key}</span></label>
                : typeof value === 'number'
                  ? <NumberField key={key} label={key} value={value} onChange={(next) => updateSelectedLibraryProp(key, next)} />
                  : <label key={key} className="field-label">{key}<input value={String(value)} onChange={(event) => updateSelectedLibraryProp(key, event.target.value)} /></label>)}
              {selectedInspectableLibraryProps.some(([, value]) => value !== null && typeof value === 'object') && <div className="ui-library-data-editors"><div className="panel-title section-title">示例数据</div>{selectedInspectableLibraryProps.filter(([, value]) => value !== null && typeof value === 'object').map(([key, value]) => <JsonPropertyEditor key={key} label={key} value={value} onChange={(next) => updateSelectedLibraryProp(key, next)} />)}</div>}
            </div>}
            </>}
            {inspectorTab === 'prototype' && <>
            <div className="panel-title section-title">预览交互</div>
            {selectedPrototypeTarget && <div className="prototype-relationship-card">
              <div className="prototype-relationship-node"><span>来源</span><strong>{selected.name}</strong><small>{currentPage?.name ?? pageId}</small></div>
              <div className="prototype-relationship-action"><i>→</i><span>{(selectedPrototypeTarget.surfaceKind ?? 'page') === 'page' ? '跳转' : `打开${WORKSPACE_SURFACE_LABELS[selectedPrototypeTarget.surfaceKind ?? 'page']}`}</span></div>
              <div className="prototype-relationship-node target"><span>目标</span><strong>{selectedPrototypeTarget.name}</strong><small>{WORKSPACE_SURFACE_LABELS[selectedPrototypeTarget.surfaceKind ?? 'page']}</small></div>
              <div className="prototype-relationship-buttons"><button onClick={() => focusWorkspaceArtboardByPageId(selectedPrototypeTarget.id)}>定位目标画板</button><button onClick={() => updateSelected({ interaction: undefined })}>移除关系</button></div>
            </div>}
            <label className="field-label">点击行为<select value={selected.interaction?.type ?? 'none'} onChange={(event) => {
              const type = event.target.value;
              if (type === 'none') updateSelected({ interaction: undefined });
              else if (type === 'page') updateSelected({ interaction: { type: 'page', target: pages.find((page) => page.id !== pageId)?.id ?? pageId } });
              else updateSelected({ interaction: { type: 'url', target: 'https://example.com' } });
            }}><option value="none">无交互</option><option value="page">连接到设计面</option><option value="url">打开 URL</option></select></label>
            {selected.interaction?.type === 'page' && <label className="field-label interaction-target">目标画板<select value={selected.interaction.target} onChange={(event) => updateSelected({ interaction: { type: 'page', target: event.target.value } })}>{pages.map((page) => <option key={page.id} value={page.id}>{page.name} · {WORKSPACE_SURFACE_LABELS[page.surfaceKind ?? 'page']}</option>)}</select></label>}
            {selected.interaction?.type === 'url' && <label className="field-label interaction-target">目标 URL<input value={selected.interaction.target} onChange={(event) => updateSelected({ interaction: { type: 'url', target: event.target.value } })} placeholder="https://example.com" /></label>}
            <p className="helper-text inspector-prototype-help">连接普通页面时执行跳转；连接弹窗、抽屉、浮层或菜单时，会在来源页面上叠加预览目标画板。</p>
            </>}
            {inspectorTab === 'design' && <>
            <div className="size-row four">
              <NumberField label={editingSlot ? 'X · 内容' : 'X'} value={inspectedFrame.x} onChange={(x) => updateInspectedFrame({ x })} disabled={selected.locked} />
              <NumberField label={editingSlot ? 'Y · 内容' : 'Y'} value={inspectedFrame.y} onChange={(y) => updateInspectedFrame({ y })} disabled={selected.locked} />
              <NumberField label="W" value={inspectedFrame.width} onChange={(width) => updateInspectedFrame({ width })} disabled={selected.locked} />
              <NumberField label="H" value={inspectedFrame.height} onChange={(height) => updateInspectedFrame({ height })} disabled={selected.locked} />
            </div>
            <div className="panel-title section-title">响应式布局 · {device}</div>
            <label className="field-label responsive-constraint-field">水平约束<select value={selected.constraints?.[device]?.horizontal ?? 'auto'} onChange={(event) => updateSelectedHorizontalConstraint(event.target.value as WebHorizontalConstraint)}>{horizontalConstraintOptions.map((option) => <option key={option.id} value={option.id}>{option.label}</option>)}</select></label>
            <p className="helper-text responsive-constraint-help">{horizontalConstraintOptions.find((option) => option.id === (selected.constraints?.[device]?.horizontal ?? 'auto'))?.description}</p>
            <div className="size-constraints-grid"><NumberField label="最小宽" value={selected.constraints?.[device]?.minWidth ?? 16} min={16} onChange={(minWidth) => updateSelectedSizeConstraints({ minWidth: Math.max(16, minWidth) })} /><NumberField label="最大宽" value={selected.constraints?.[device]?.maxWidth ?? 100000} min={16} onChange={(maxWidth) => updateSelectedSizeConstraints({ maxWidth: Math.max(16, maxWidth) })} /><NumberField label="最小高" value={selected.constraints?.[device]?.minHeight ?? 16} min={16} onChange={(minHeight) => updateSelectedSizeConstraints({ minHeight: Math.max(16, minHeight) })} /><NumberField label="最大高" value={selected.constraints?.[device]?.maxHeight ?? 100000} min={16} onChange={(maxHeight) => updateSelectedSizeConstraints({ maxHeight: Math.max(16, maxHeight) })} /></div>
            <label className="ui-library-boolean-prop constraint-toggle"><input type="checkbox" checked={selected.constraints?.[device]?.lockAspectRatio === true} onChange={(event) => updateSelectedSizeConstraints({ lockAspectRatio: event.target.checked })} /><span>调整大小时保持当前宽高比</span></label>
            <div className="panel-title section-title">视觉设计 · {device}</div>
            {inspectorCapabilities?.visualStates && <div className="visual-state-switcher"><button className={inspectorVisualState === 'default' ? 'active' : ''} onClick={() => setInspectorVisualState('default')}>默认</button><button className={inspectorVisualState === 'hover' ? 'active' : ''} onClick={() => setInspectorVisualState('hover')}>悬停</button><button className={inspectorVisualState === 'active' ? 'active' : ''} onClick={() => setInspectorVisualState('active')}>按下</button><button className={inspectorVisualState === 'focus' ? 'active' : ''} onClick={() => setInspectorVisualState('focus')}>聚焦</button></div>}
            {inspectorCapabilities?.visualStates && inspectorVisualState !== 'default' && <div className="visual-state-help"><p className="helper-text">正在设计“{inspectorVisualState === 'hover' ? '悬停' : inspectorVisualState === 'active' ? '按下' : '聚焦'}”状态；画布会立即显示效果，预览时由真实交互触发。</p><button onClick={clearSelectedVisualState} disabled={!selected.states?.[inspectorVisualState]}>清除状态样式</button></div>}
            <section className="design-inspector-group">
              <header><strong>填充</strong><span>颜色、渐变与透明材质</span></header>
              <ColorValueField label="背景" value={inspectedStyle?.background ?? ''} onChange={(background) => updateSelectedStyle({ background })} allowComplex />
              <div className="style-preset-grid fill-presets">{fillPresets.map((fill, index) => <button key={fill} title={fill} aria-label={`填充预设 ${index + 1}`} style={{ background: fill }} onClick={() => updateSelectedStyle({ background: fill })} />)}</div>
              <div className="size-row"><NumberField label="内边距" value={inspectedStyle?.padding ?? 0} onChange={(padding) => updateSelectedStyle({ padding: Math.max(0, padding) })} /><NumberField label="透明度" value={inspectedStyle?.opacity ?? 1} step={0.05} min={0} max={1} onChange={(opacity) => updateSelectedStyle({ opacity: Math.min(1, Math.max(0, opacity)) })} /></div>
            </section>
            <section className="design-inspector-group">
              <header><strong>描边</strong><span>边框与圆角</span></header>
              <ColorValueField label="描边颜色" value={inspectedStyle?.borderColor ?? ''} onChange={(borderColor) => updateSelectedStyle({ borderColor })} />
              <div className="size-row"><NumberField label="粗细" value={inspectedStyle?.borderWidth ?? 0} onChange={(borderWidth) => updateSelectedStyle({ borderWidth: Math.max(0, borderWidth) })} /><NumberField label="圆角" value={inspectedStyle?.borderRadius ?? 0} onChange={(borderRadius) => updateSelectedStyle({ borderRadius: Math.max(0, borderRadius) })} /></div>
              <label className="field-label">线型<select value={inspectedStyle?.borderStyle ?? 'solid'} onChange={(event) => updateSelectedStyle({ borderStyle: event.target.value as NonNullable<WebComponentStyle['borderStyle']> })}><option value="solid">实线</option><option value="dashed">虚线</option><option value="dotted">点线</option><option value="double">双线</option><option value="none">无</option></select></label>
            </section>
            <section className="design-inspector-group">
              <header><strong>效果</strong><span>阴影、模糊与叠加</span></header>
              <label className="field-label">阴影<input value={inspectedStyle?.shadow ?? ''} onChange={(event) => updateSelectedStyle({ shadow: event.target.value })} placeholder="0 18px 48px rgba(0,0,0,.18)" /></label>
              <div className="style-preset-grid shadow-presets">{shadowPresets.map((shadow, index) => <button key={`${shadow}-${index}`} className={shadow ? '' : 'none'} title={shadow || '无阴影'} style={{ boxShadow: shadow || undefined }} onClick={() => updateSelectedStyle({ shadow })}>{shadow ? '' : '×'}</button>)}</div>
              <div className="size-row"><NumberField label="元素模糊" value={inspectedStyle?.blur ?? 0} onChange={(blur) => updateSelectedStyle({ blur: Math.max(0, blur) })} /><NumberField label="背景模糊" value={inspectedStyle?.backdropBlur ?? 0} onChange={(backdropBlur) => updateSelectedStyle({ backdropBlur: Math.max(0, backdropBlur) })} /></div>
              <div className="size-row"><NumberField label="旋转 °" value={inspectedStyle?.rotate ?? 0} onChange={(rotate) => updateSelectedStyle({ rotate })} /><NumberField label="缩放" value={inspectedStyle?.scale ?? 1} step={0.05} min={0.01} onChange={(scale) => updateSelectedStyle({ scale: Math.max(0.01, scale) })} /></div>
              <div className="size-row"><label className="field-label">溢出<select value={inspectedStyle?.overflow ?? 'visible'} onChange={(event) => updateSelectedStyle({ overflow: event.target.value as NonNullable<WebComponentStyle['overflow']> })}><option value="visible">显示</option><option value="hidden">裁切</option><option value="auto">自动滚动</option><option value="scroll">始终滚动</option></select></label><label className="field-label">混合模式<select value={inspectedStyle?.mixBlendMode ?? 'normal'} onChange={(event) => updateSelectedStyle({ mixBlendMode: event.target.value as NonNullable<WebComponentStyle['mixBlendMode']> })}><option value="normal">正常</option><option value="multiply">正片叠底</option><option value="screen">滤色</option><option value="overlay">叠加</option><option value="difference">差值</option></select></label></div>
            </section>
            {inspectorCapabilities?.typography && <section className="design-inspector-group">
              <header><strong>排版</strong><span>所有文字型组件与 UI 内容</span></header>
              <ColorValueField label="文字颜色" value={inspectedStyle?.color ?? ''} onChange={(color) => updateSelectedStyle({ color })} />
              <div className="size-row"><NumberField label="字号" value={inspectedStyle?.fontSize ?? 16} onChange={(fontSize) => updateSelectedStyle({ fontSize: Math.max(6, fontSize) })} /><NumberField label="字重" value={inspectedStyle?.fontWeight ?? 400} step={50} min={100} max={1000} onChange={(fontWeight) => updateSelectedStyle({ fontWeight: Math.min(1000, Math.max(100, fontWeight)) })} /></div>
              <div className="size-row"><NumberField label="行高" value={inspectedStyle?.lineHeight ?? 1.2} step={0.05} min={0.5} max={5} onChange={(lineHeight) => updateSelectedStyle({ lineHeight })} /><NumberField label="字间距" value={inspectedStyle?.letterSpacing ?? 0} step={0.1} onChange={(letterSpacing) => updateSelectedStyle({ letterSpacing })} /></div>
              <div className="size-row"><label className="field-label">对齐<select value={inspectedStyle?.textAlign ?? 'left'} onChange={(event) => updateSelectedStyle({ textAlign: event.target.value as NonNullable<WebComponentStyle['textAlign']> })}><option value="left">左对齐</option><option value="center">居中</option><option value="right">右对齐</option></select></label><label className="field-label">大小写<select value={inspectedStyle?.textTransform ?? 'none'} onChange={(event) => updateSelectedStyle({ textTransform: event.target.value as NonNullable<WebComponentStyle['textTransform']> })}><option value="none">保持</option><option value="uppercase">大写</option><option value="lowercase">小写</option><option value="capitalize">首字母大写</option></select></label></div>
            </section>}
            {inspectorCapabilities?.media && <section className="design-inspector-group"><header><strong>媒体</strong><span>裁切与焦点</span></header><div className="size-row"><label className="field-label">适应方式<select value={inspectedStyle?.objectFit ?? 'cover'} onChange={(event) => updateSelectedStyle({ objectFit: event.target.value as NonNullable<WebComponentStyle['objectFit']> })}><option value="cover">覆盖裁切</option><option value="contain">完整显示</option><option value="fill">拉伸填充</option><option value="none">原始大小</option><option value="scale-down">自动缩小</option></select></label><label className="field-label">焦点<input value={inspectedStyle?.objectPosition ?? '50% 50%'} onChange={(event) => updateSelectedStyle({ objectPosition: event.target.value })} /></label></div></section>}
            <section className="design-inspector-group advanced-css-group">
              <header><strong>高级样式</strong><span>开放式 CSS，不受面板枚举限制</span></header>
              <AdvancedCssEditor value={inspectorVisualState === 'default' ? inspectedFrame?.style.customCss ?? {} : selected.states?.[inspectorVisualState]?.customCss ?? {}} onChange={updateSelectedCustomCss} />
            </section>
            <div className="token-apply-row"><button onClick={() => applyColorToken('background', 'primary')}>主色背景</button><button onClick={() => applyColorToken('background', 'surface')}>表面背景</button><button onClick={() => applyColorToken('color', 'text')}>正文色</button><button onClick={() => applyRadiusToken('medium')}>中圆角</button></div>
            {inspectorCapabilities?.layout && <>
              <div className="panel-title section-title">容器布局 · {directChildCount} 个子组件</div>
              <label className="field-label">布局方式<select value={selected.layout?.mode ?? 'free'} onChange={(event) => updateSelectedLayout({ mode: event.target.value as NonNullable<WebDesignComponent['layout']>['mode'] })}><option value="free">自由布局</option><option value="flex-row">Flex 横向</option><option value="flex-column">Flex 纵向</option><option value="grid">Grid 网格</option></select></label>
              <div className="size-row"><NumberField label="间距" value={selected.layout?.gap ?? 16} onChange={(gap) => updateSelectedLayout({ gap })} /><NumberField label="内边距" value={selected.layout?.padding ?? 16} onChange={(padding) => updateSelectedLayout({ padding })} /></div>
              {selected.layout?.mode === 'grid' && <NumberField label="列数" value={selected.layout.columns ?? 2} onChange={(columns) => updateSelectedLayout({ columns: Math.max(1, Math.round(columns)) })} />}
              <div className="size-row"><label className="field-label layout-align-field">交叉轴<select value={selected.layout?.align ?? 'start'} onChange={(event) => updateSelectedLayout({ align: event.target.value as NonNullable<WebDesignComponent['layout']>['align'] })}><option value="start">起点</option><option value="center">居中</option><option value="end">终点</option><option value="stretch">拉伸</option></select></label><label className="field-label">主轴<select value={selected.layout?.justify ?? 'start'} onChange={(event) => updateSelectedLayout({ justify: event.target.value as NonNullable<WebDesignComponent['layout']>['justify'] })}><option value="start">起点</option><option value="center">居中</option><option value="end">终点</option><option value="space-between">两端分布</option><option value="space-around">环绕分布</option></select></label></div>
              {selected.layout?.mode === 'flex-row' && <label className="ui-library-boolean-prop"><input type="checkbox" checked={selected.layout?.wrap === true} onChange={(event) => updateSelectedLayout({ wrap: event.target.checked })} /><span>空间不足时自动换行</span></label>}
              <button className="secondary-button" disabled={directChildCount === 0 || selected.layout?.mode === 'free'} onClick={applySelectedAutoLayout}>应用自动布局</button>
            </>}
            </>}
            {inspectorTab === 'ai' && <>
            <div className="panel-title section-title">组件批注</div>
            <div className="notes-list">{selected.annotations.length === 0 && <span className="empty-hint">还没有批注</span>}{selected.annotations.map((note) => <div key={note.id} className={`note-card ${note.status}`}><span>{note.text}</span><small>{note.status === 'open' ? '待处理' : '已完成'}</small></div>)}</div>
            <textarea className="composer" rows={3} placeholder="例如：这里的按钮再醒目一些" value={annotationText} onChange={(event) => setAnnotationText(event.target.value)} /><button className="secondary-button" onClick={addLegacyAnnotation}>添加批注</button>
            <div className="panel-title section-title">与 AI 交互</div>
            <textarea className="composer" rows={4} placeholder={`告诉 AI 如何修改当前${device === 'desktop' ? '桌面' : device === 'tablet' ? '平板' : '手机'}组件…`} value={aiInstruction} onChange={(event) => setAiInstruction(event.target.value)} /><button className="ai-button" onClick={() => void addAiRequest()}>提交给 AI</button>
            </>}
          </> : <div className="empty-inspector"><div className="empty-icon">↖</div><strong>选择一个组件</strong><p>在画布或图层中选择组件，然后编辑、对齐、锁定、批注或提交 AI 请求。</p></div>}
        </aside>}
      </main>
      {variantPickerDefinition && variantPickerLibrary && <div className={`studio-side-surface-host ${variantPickerDrag?.dragging ? 'dragging-library-element' : ''}`}>
        <section className="studio-modal studio-side-surface variant-picker" data-library-portal-host>
          <header><div><span className="eyebrow">{variantPickerLibrary.displayName} · {variantPickerDefinition.category}</span><h2>{variantPickerDefinition.id} · {variantPickerDefinition.label}</h2><p>移动到想要的元素上，点击直接插入，或按住拖到画布中的准确位置。</p></div><button onClick={() => setVariantPickerTarget(undefined)}>×</button></header>
          <div className={`variant-preview-grid ${WIDE_VARIANT_PREVIEWS.has(variantPickerDefinition.id) || variantPickerPresentation?.previewSpan === 'wide' ? 'wide-component-previews' : ''} ${variantPickerVariants.length === 1 ? 'single-component-preview' : ''}`}>{variantPickerVariants.map((variant) => {
            const previewComponent = applyUiLibraryVariant(createComponentFromUiLibrary(variantPickerLibrary.id, variantPickerDefinition.id, 0, 0), variant.id);
            previewComponent.id = `library-preview-${variantPickerLibrary.id}-${variantPickerDefinition.id}-${variant.id}`;
            const differences = variantDifferenceLabels(variant);
            const interactiveVariant = variantIsInteractive(variant, variantPickerDefinition.id);
            const openOverlayPreview = OPEN_OVERLAY_PREVIEWS.has(variantPickerDefinition.id);
            const inlinePickerPreview = variantPickerLibrary.id === 'chakra' && ['DatePicker', 'ColorPicker'].includes(variantPickerDefinition.id);
            const previewHeight = variantPickerPresentation?.previewHeight ?? (inlinePickerPreview || interactiveVariant
              ? 440
              : Math.max(openOverlayPreview ? 320 : 118, Math.min(360, previewComponent.height + 24)));
            return <SelectableVariantCard key={variant.id} component={previewComponent} previewHeight={previewHeight} className={`variant-live-preview ${openOverlayPreview ? 'overlay-showcase' : ''} ${inlinePickerPreview ? 'inline-picker-showcase' : ''} ${variant.props.bordered === false || variant.props.variant === 'borderless' ? 'contrast-surface' : ''}`} interactive={interactiveVariant} variantLabel={variant.label} differences={differences} tokens={tokens} onPickItem={(selection) => chooseUiLibraryPreviewElement(variantPickerLibrary.id, variantPickerDefinition.id, variant.id, selection)} onPickPointerEvent={(event) => handleUiLibraryPreviewPointerEvent(variantPickerLibrary.id, variantPickerDefinition.id, variant.id, event)} />;
          })}</div>
        </section>
      </div>}
      {variantPickerDrag?.dragging && <div
        className="variant-picker-pointer-capture"
        onPointerMove={(event) => { event.preventDefault(); event.stopPropagation(); moveUiLibraryPreviewPointerDragAt(event.clientX, event.clientY); }}
        onPointerUp={(event) => { event.preventDefault(); event.stopPropagation(); finishUiLibraryPreviewPointerDragAt(event.clientX, event.clientY); }}
        onPointerCancel={(event) => { event.preventDefault(); event.stopPropagation(); finishUiLibraryPreviewPointerDragAt(event.clientX, event.clientY, true); }}
        onContextMenu={(event) => event.preventDefault()}
      />}
      {variantPickerDrag?.dragging && <div className="variant-picker-drag-ghost" style={{ left: variantPickerDrag.clientX, top: variantPickerDrag.clientY }}><strong>{variantPickerDrag.selection.label}</strong><small>{Math.round(variantPickerDrag.selection.width)} × {Math.round(variantPickerDrag.selection.height)}</small></div>}
      {themePickerOpen && <div className="studio-side-surface-host">
        <section className="studio-modal studio-side-surface theme-picker">
          <header><div><span className="eyebrow">Scene visual system</span><h2>建立视觉变量</h2><p>把颜色、字体和圆角写入真实 Scene Variable Collection，供 AI 与人工设计共同绑定使用。</p></div><button onClick={() => setThemePickerOpen(false)}>×</button></header>
          <div className="theme-preset-grid">{WEB_DESIGN_THEME_PRESETS.map((preset) => <button key={preset.id} onClick={() => void applyDesignTheme(preset)}><div className="theme-preview" style={{ background: preset.canvasBackground }}><i style={{ background: preset.preview[1] }} /><b style={{ background: preset.preview[2] }} /><span style={{ color: preset.tokens.colors.text }}>Aa</span></div><strong>{preset.name}</strong><small>{preset.description}</small><div className="theme-swatches">{preset.preview.map((color) => <i key={color} style={{ background: color }} />)}</div></button>)}</div>
        </section>
      </div>}
      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}

function NumberField({ label, value, onChange, disabled = false, step, min, max }: { label: string; value: number; onChange: (value: number) => void; disabled?: boolean; step?: number; min?: number; max?: number }) {
  return <label className="field-label">{label}<input type="number" disabled={disabled} step={step} min={min} max={max} value={Number.isFinite(value) ? value : 0} onChange={(event) => onChange(Number(event.target.value))} /></label>;
}

function SceneNumberField({ label, value, onCommit, disabled = false, min, max }: {
  label: string;
  value: number;
  onCommit: (value: number) => void;
  disabled?: boolean;
  min?: number;
  max?: number;
}) {
  return <label className="field-label">{label}<input
    key={`${value}`}
    type="number"
    defaultValue={Number.isFinite(value) ? value : 0}
    disabled={disabled}
    min={min}
    max={max}
    onBlur={(event) => {
      const next = Number(event.currentTarget.value);
      if (!Number.isFinite(next) || next === value) return;
      onCommit(Math.min(max ?? Number.POSITIVE_INFINITY, Math.max(min ?? Number.NEGATIVE_INFINITY, next)));
    }}
    onKeyDown={(event) => {
      if (event.key === 'Enter') event.currentTarget.blur();
    }}
  /></label>;
}

function ColorValueField({ label, value, onChange, allowComplex = false }: { label: string; value: string; onChange: (value: string) => void; allowComplex?: boolean }) {
  const colorValue = /^#[0-9a-f]{6}$/i.test(value) ? value : '#000000';
  return <label className="field-label color-value-field">{label}<span><input type="color" value={colorValue} onChange={(event) => onChange(event.target.value.toUpperCase())} /><input value={value} onChange={(event) => onChange(event.target.value)} placeholder={allowComplex ? '#FFFFFF 或 linear-gradient(...)' : '#1D1D1F'} /></span></label>;
}

function AdvancedCssEditor({ value, onChange }: { value: Record<string, string | number>; onChange: (value: Record<string, string | number>) => void }) {
  const serialized = Object.entries(value).map(([property, propertyValue]) => `${property}: ${propertyValue}`).join('\n');
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState('');
  useEffect(() => { setDraft(serialized); setError(''); }, [serialized]);
  function apply() {
    try {
      const next: Record<string, string | number> = {};
      for (const rawLine of draft.split('\n')) {
        const line = rawLine.trim().replace(/;$/, '');
        if (!line || line.startsWith('/*') || line.startsWith('//')) continue;
        const separator = line.indexOf(':');
        if (separator <= 0) throw new Error(`缺少冒号：${line}`);
        const property = line.slice(0, separator).trim();
        const propertyValue = line.slice(separator + 1).trim();
        if (!/^(?:--[a-zA-Z0-9_-]{1,80}|[a-zA-Z][a-zA-Z0-9-]{0,80})$/.test(property)) throw new Error(`属性名不正确：${property}`);
        if (!propertyValue) throw new Error(`缺少属性值：${property}`);
        next[property] = /^-?(?:\d+|\d*\.\d+)$/.test(propertyValue) ? Number(propertyValue) : propertyValue;
      }
      onChange(next);
      setError('');
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'CSS 格式不正确');
    }
  }
  const suggestions = [
    ['异形裁切', 'clip-path: polygon(0 0, 100% 0, 92% 100%, 8% 100%)'],
    ['玻璃高光', 'background-image: linear-gradient(135deg, rgba(255,255,255,.28), rgba(255,255,255,0))'],
    ['渐隐遮罩', 'mask-image: linear-gradient(to bottom, black 72%, transparent)'],
    ['立体视角', 'perspective: 1000px']
  ] as const;
  function appendSuggestion(declaration: string) {
    setDraft((current) => current.trim() ? `${current.trim()}\n${declaration}` : declaration);
  }
  return <div className="advanced-css-editor"><div className="advanced-css-suggestions">{suggestions.map(([label, declaration]) => <button key={label} onClick={() => appendSuggestion(declaration)}>{label}</button>)}</div><textarea rows={Math.min(10, Math.max(4, draft.split('\n').length))} value={draft} onChange={(event) => setDraft(event.target.value)} spellCheck={false} placeholder={'clip-path: circle(48%)\ntransform-origin: 50% 100%\n--brand-glow: rgba(99,102,241,.45)'} /><div className="advanced-css-actions"><small>{error || '每行一个 CSS 属性；AI 也可以直接写入这些结构化样式。'}</small><button onClick={apply}>应用样式</button></div></div>;
}

function JsonPropertyEditor({ label, value, onChange }: { label: string; value: WebDesignJsonValue; onChange: (value: WebDesignJsonValue) => void }) {
  const serialized = JSON.stringify(value, null, 2);
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState('');
  useEffect(() => { setDraft(serialized); setError(''); }, [serialized]);
  function apply() {
    try {
      const parsed = JSON.parse(draft) as WebDesignJsonValue;
      onChange(parsed);
      setError('');
    } catch {
      setError('JSON 格式不正确');
    }
  }
  return <div className="json-prop-editor"><div><strong>{label}</strong><button onClick={apply}>应用数据</button></div><textarea rows={Math.min(10, Math.max(4, draft.split('\n').length))} value={draft} onChange={(event) => setDraft(event.target.value)} spellCheck={false} />{error && <small>{error}</small>}</div>;
}

function JsonObjectEditor({ label, value, onChange, disabled = false }: { label: string; value: Record<string, unknown>; onChange: (value: Record<string, unknown>) => void; disabled?: boolean }) {
  const serialized = JSON.stringify(value, null, 2);
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState('');
  useEffect(() => { setDraft(serialized); setError(''); }, [serialized]);
  function apply() {
    try {
      const parsed = JSON.parse(draft) as unknown;
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('属性必须是 JSON 对象');
      onChange(parsed as Record<string, unknown>);
      setError('');
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'JSON 格式不正确');
    }
  }
  return <div className="json-prop-editor scene-json-object-editor"><div><strong>{label}</strong><button disabled={disabled} onClick={apply}>应用数据</button></div><textarea disabled={disabled} rows={Math.min(14, Math.max(5, draft.split('\n').length))} value={draft} onChange={(event) => setDraft(event.target.value)} spellCheck={false} />{error && <small>{error}</small>}</div>;
}

function runtimeSlotContentMap(
  document: WebDesignDocument,
  component: WebDesignComponent,
  device: WebDesignDevice,
  interactive: boolean,
  tokens: WebDesignTokens | undefined,
  onPreviewActivate: (component: WebDesignComponent) => void
): Record<string, ReactNode> {
  return Object.fromEntries(editableSlotsForUiComponent(component)
    .filter((slot) => componentsInSlot(document, component.id, slot.id).length > 0)
    .map((slot) => [slot.id,
      <RuntimeSlotContent key={slot.id} document={document} container={component} slot={slot} device={device} interactive={interactive} tokens={tokens} onPreviewActivate={onPreviewActivate} />
    ]));
}

function RuntimeSlotContent({ document, container, slot, device, interactive, tokens, onPreviewActivate }: {
  document: WebDesignDocument;
  container: WebDesignComponent;
  slot: UiEditableSlot;
  device: WebDesignDevice;
  interactive: boolean;
  tokens?: WebDesignTokens;
  onPreviewActivate: (component: WebDesignComponent) => void;
}) {
  const containerFrame = resolveComponent(container, device);
  const children = visibleComponentsInSlot(document, container.id, slot.id).sort((left, right) => left.zIndex - right.zIndex);
  if (children.length === 0) return null;
  const requiredHeight = Math.max(slot.height, ...children.map((child) => {
    const frame = resolveComponent(child, device);
    return frame.y - containerFrame.y + frame.height + 12;
  }));
  return <div className="runtime-slot-canvas" style={{ minHeight: requiredHeight }}>
    {children.map((child) => {
      const frame = resolveComponent(child, device);
      if (frame.hidden) return null;
      const localFrame = { ...frame, x: frame.x - containerFrame.x, y: frame.y - containerFrame.y };
      return <RuntimeSlotCanvasComponent key={child.id} component={child} frame={localFrame} interactive={interactive} tokens={tokens} slotContent={runtimeSlotContentMap(document, child, device, interactive, tokens, onPreviewActivate)} onPreviewActivate={() => onPreviewActivate(child)} />;
    })}
  </div>;
}

function RuntimeSlotCanvasComponent({ component, frame, interactive, tokens, slotContent, onPreviewActivate }: {
  component: WebDesignComponent;
  frame: ResolvedWebDesignComponent;
  interactive: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
  onPreviewActivate: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  const [pressed, setPressed] = useState(false);
  const [focused, setFocused] = useState(false);
  const runtimeState: WebComponentVisualState | undefined = pressed ? 'active' : focused ? 'focus' : hovered ? 'hover' : undefined;
  const effectiveStyle = mergeComponentStyles(frame.style, runtimeState ? component.states?.[runtimeState] : undefined);
  const style: CSSProperties = {
    position: 'absolute', left: frame.x, top: frame.y, width: frame.width, height: frame.height, zIndex: component.zIndex,
    ...(component.library ? componentEffectStyleToCss(effectiveStyle) : componentStyleToCss(effectiveStyle)),
    transition: component.states ? 'background .18s ease, color .18s ease, border-color .18s ease, box-shadow .18s ease, opacity .18s ease, transform .18s ease' : undefined
  };
  return <div className={`runtime-slot-component type-${component.type}`} style={style} tabIndex={interactive && component.states?.focus ? 0 : undefined} onPointerEnter={() => interactive && setHovered(true)} onPointerLeave={() => { setHovered(false); setPressed(false); }} onPointerDown={() => interactive && setPressed(true)} onPointerUp={() => setPressed(false)} onPointerCancel={() => setPressed(false)} onFocus={() => interactive && setFocused(true)} onBlur={() => setFocused(false)} onClick={(event) => {
    if (interactive && component.interaction) {
      event.stopPropagation();
      onPreviewActivate();
    }
  }}><WorkspaceCanvasComponentContent component={component} style={effectiveStyle} interactive={interactive} tokens={tokens} slotContent={slotContent} /></div>;
}

function formatProjectDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '最近更新';
  const today = new Date();
  const sameDay = date.getFullYear() === today.getFullYear()
    && date.getMonth() === today.getMonth()
    && date.getDate() === today.getDate();
  return sameDay
    ? `今天 ${date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`
    : date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}
