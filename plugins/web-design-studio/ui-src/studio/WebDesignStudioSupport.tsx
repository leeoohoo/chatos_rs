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
  matchArtboardSizePreset,
  matchViewportPreset,
  viewportDimensions,
  viewportPresetsForDevice,
  WEB_DESIGN_ARTBOARD_SIZE_PRESETS,
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
import { DEFAULT_WORKSPACE_SHELL, parseWorkspaceShellState, workspaceShellGridStyle, workspaceShellReducer, workspaceShellShortcut, type WorkspaceArea, type WorkspaceTool } from './workspace-shell-model';
import { initialWorkspaceArtboards, reconcileWorkspaceArtboards, updateWorkspaceArtboardById, workspaceViewportHeight } from './workspace-artboard-model';
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

export type BasicShapeId = 'rectangle' | 'ellipse' | 'line';

export const SCENE_RESPONSIVE_EDITOR_RULES = {
  tablet: { ruleId: 'editor:tablet', ruleName: '人工中等宽度布局', minWidth: 768, maxWidth: 1200 },
  mobile: { ruleId: 'editor:mobile', ruleName: '人工窄宽度布局', maxWidth: 768 }
} as const;

export const SLOT_EDITOR_HEADER_HEIGHT = 72;
export const SLOT_EDITOR_CANVAS_INSETS = { top: 27, right: 9, bottom: 9, left: 9 } as const;

export function slotEditorFrameBounds(canvasSize: { width: number; height: number }) {
  return {
    x: 0,
    y: 0,
    width: canvasSize.width + SLOT_EDITOR_CANVAS_INSETS.left + SLOT_EDITOR_CANVAS_INSETS.right,
    height: SLOT_EDITOR_HEADER_HEIGHT + SLOT_EDITOR_CANVAS_INSETS.top + canvasSize.height + SLOT_EDITOR_CANVAS_INSETS.bottom
  };
}

export const palette: Array<{ id: BasicShapeId; type: WebComponentType; label: string; icon: string; keywords: string[] }> = [
  { id: 'rectangle', type: 'section', label: '矩形', icon: '▭', keywords: ['rectangle', '矩形', '容器'] },
  { id: 'ellipse', type: 'section', label: '圆形', icon: '○', keywords: ['ellipse', 'circle', '圆形'] },
  { id: 'line', type: 'divider', label: '直线', icon: '—', keywords: ['line', 'divider', '直线'] }
];

export const fillPresets = [
  '#FFFFFF', '#F5F5F7', '#1D1D1F', '#007AFF', '#7C3AED', '#EC4899',
  'linear-gradient(135deg,#007AFF,#5AC8FA)',
  'linear-gradient(135deg,#7C3AED,#EC4899)',
  'radial-gradient(circle at 30% 20%,rgba(255,255,255,.75),transparent 28%),linear-gradient(145deg,#111827,#312E81)'
];

export const shadowPresets = [
  '',
  '0 8px 24px rgba(15,23,42,.10)',
  '0 18px 48px rgba(15,23,42,.18)',
  '0 28px 80px rgba(15,23,42,.28)',
  '0 0 60px rgba(99,102,241,.45)',
  'inset 0 0 0 1px rgba(255,255,255,.18)'
];

export function basicShapeDefaults(shapeId: BasicShapeId, x: number, y: number): WebDesignComponent {
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

export function contentContainerAncestor(document: WebDesignDocument, component: WebDesignComponent): WebDesignComponent | undefined {
  const byId = new Map(document.components.map((candidate) => [candidate.id, candidate]));
  let parent = component.parentId ? byId.get(component.parentId) : undefined;
  while (parent) {
    if (isUiContentContainer(parent)) return parent;
    parent = parent.parentId ? byId.get(parent.parentId) : undefined;
  }
  return undefined;
}

export function overlayContentContainerAncestor(document: WebDesignDocument, component: WebDesignComponent): WebDesignComponent | undefined {
  const byId = new Map(document.components.map((candidate) => [candidate.id, candidate]));
  let parent = component.parentId ? byId.get(component.parentId) : undefined;
  while (parent) {
    if (isOverlayUiContentContainer(parent)) return parent;
    parent = parent.parentId ? byId.get(parent.parentId) : undefined;
  }
  return undefined;
}

export function growCanvasForDevice(document: WebDesignDocument, pageId: string, device: WebDesignDevice, minimumHeight?: number): WebDesignDocument {
  const fitted = growUiContentContainersToFit(document, pageId, device);
  const excludedComponentIds = new Set(componentsForPage(fitted, pageId)
    .filter((component) => overlayContentContainerAncestor(fitted, component))
    .map((component) => component.id));
  return growPageToFitContent(fitted, pageId, device, { excludedComponentIds, minimumHeight });
}

export function growAllCanvases(document: WebDesignDocument): WebDesignDocument {
  return pagesForDocument(document).reduce((current, page) =>
    (['desktop', 'tablet', 'mobile'] as const).reduce((next, device) => growCanvasForDevice(next, page.id, device), current), document);
}

export function createSlotStarterComponents(
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

export function materializeExistingSlotContent(
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

export function visibleCssColor(value: string): boolean {
  if (!value || value === 'transparent') return false;
  const alpha = value.match(/rgba?\([^)]*[,/]\s*([\d.]+)\s*\)$/)?.[1];
  return alpha === undefined || Number(alpha) > 0;
}

export function cssPixels(value: string): number {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

export function svgDataUrl(element: Element, color: string): string {
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

export async function materializeOfficialDemoContent(
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

export const horizontalConstraintOptions: Array<{ id: WebHorizontalConstraint; label: string; description: string }> = [
  { id: 'auto', label: '智能响应', description: '根据组件大小和位置自动选择缩放或锚定方式' },
  { id: 'left', label: '左侧固定', description: '保持宽度和左边距' },
  { id: 'center', label: '水平居中', description: '保持宽度并相对容器居中' },
  { id: 'right', label: '右侧固定', description: '保持宽度和右边距' },
  { id: 'stretch', label: '左右拉伸', description: '保持左右边距，宽度跟随容器' },
  { id: 'scale', label: '等比缩放', description: '位置和宽度随容器同比例变化' }
];

export type ViewportSelection = {
  presetId?: string;
  orientation: WebDesignViewportOrientation;
  customHeight: number;
};

export const STUDIO_PROJECT_QUERY = 'studio-project';
export const STUDIO_DESIGN_QUERY = 'studio-design';

export function studioLocationSelection() {
  const params = new URLSearchParams(window.location.search);
  return {
    projectId: params.get(STUDIO_PROJECT_QUERY) || undefined,
    documentId: params.get(STUDIO_DESIGN_QUERY) || undefined
  };
}

export function replaceStudioLocation(projectId?: string, documentId?: string) {
  const url = new URL(window.location.href);
  if (projectId) url.searchParams.set(STUDIO_PROJECT_QUERY, projectId);
  else url.searchParams.delete(STUDIO_PROJECT_QUERY);
  if (projectId && documentId) url.searchParams.set(STUDIO_DESIGN_QUERY, documentId);
  else url.searchParams.delete(STUDIO_DESIGN_QUERY);
  window.history.replaceState(null, '', url);
}

export const DEFAULT_VIEWPORT_SELECTIONS: Record<WebDesignDevice, ViewportSelection> = {
  desktop: { presetId: 'desktop-responsive', orientation: 'default', customHeight: 900 },
  tablet: { presetId: 'tablet-responsive', orientation: 'default', customHeight: 1024 },
  mobile: { presetId: 'mobile-responsive', orientation: 'default', customHeight: 844 }
};

export function viewportSelectionsForDocument(document: WebDesignDocument): Record<WebDesignDevice, ViewportSelection> {
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

export function editableDocumentPayload(document: WebDesignDocument): string {
  const { revision: _revision, createdAt: _createdAt, updatedAt: _updatedAt, ...editable } = document;
  return JSON.stringify(editable);
}

export type Interaction = {
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

export type CanvasPan = {
  pointerX: number;
  pointerY: number;
  camera: WorkspaceCamera;
};

export type CanvasMarquee = {
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

export type LayerAction = 'front' | 'forward' | 'backward' | 'back';
export type AlignAction = 'left' | 'center' | 'right' | 'top' | 'middle' | 'bottom';
export type LibraryTab = 'components' | WebDesignLibraryName | 'my' | 'layers';
export type VariantPickerTarget = { library: WebDesignLibraryName; componentId: string; replaceComponentId?: string };
export type VariantPickerPointerDrag = Omit<LibraryPreviewPointerEvent, 'phase'> & {
  libraryName: WebDesignLibraryName;
  definitionId: string;
  variantId: string;
  startClientX: number;
  startClientY: number;
  dragging: boolean;
};
export type EditingSlot = { componentId: string; slotId: string };
export type SceneContentFocus = SceneInsertionFocus & { pageId: string };
export type SelectionCandidatePopover = {
  clientX: number;
  clientY: number;
  candidates: EditorSelectionCandidate[];
};
export type InspectorVisualState = 'default' | WebComponentVisualState;
export type InspectorTab = 'design' | 'prototype' | 'ai' | 'review';

export const PERSONAL_SYMBOLS_STORAGE_KEY = 'web-design-studio:personal-symbols:v1';
export const SCENE_SNIPPETS_STORAGE_KEY = 'web-design-studio:scene-snippets:v2';

export const WORKSPACE_ARTBOARD_HEADER_HEIGHT = 48;

export const WORKSPACE_SURFACE_LABELS: Record<WorkspaceSurfaceKind, string> = {
  page: '页面',
  modal: '弹窗',
  drawer: '抽屉',
  popover: '浮层',
  menu: '菜单',
  state: '界面状态'
};

export const WORKSPACE_SURFACE_SIZES: Record<Exclude<WorkspaceSurfaceKind, 'page' | 'state'>, { width: number; height: number }> = {
  modal: { width: 720, height: 720 },
  drawer: { width: 520, height: 900 },
  popover: { width: 420, height: 360 },
  menu: { width: 320, height: 440 }
};

export function deviceForWorkspaceArtboard(document: WebDesignDocument, artboard: WorkspaceArtboardPlacement): WebDesignDevice {
  if (artboard.surfaceKind !== 'page') return 'desktop';
  return (['desktop', 'tablet', 'mobile'] as const).reduce((closest, candidate) => (
    Math.abs(breakpointFor(document, candidate).width - artboard.viewportWidth)
      < Math.abs(breakpointFor(document, closest).width - artboard.viewportWidth) ? candidate : closest
  ), 'desktop' as WebDesignDevice);
}

export function workspaceArtboardContentBounds(document: WebDesignDocument, artboard: WorkspaceArtboardPlacement, scene?: SceneDocument) {
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

export function workspaceArtboardSignature(artboards: readonly WorkspaceArtboardPlacement[]): string {
  return JSON.stringify(artboards);
}

export function loadPersonalSymbols(): WebDesignSymbol[] {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(PERSONAL_SYMBOLS_STORAGE_KEY) ?? '[]') as unknown;
    return Array.isArray(parsed) ? parsed.filter((item): item is WebDesignSymbol => Boolean(item && typeof item === 'object' && 'id' in item && 'components' in item)) : [];
  } catch {
    return [];
  }
}

export const VARIANT_PROP_LABELS: Record<string, Record<string, string> | string> = {
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

export const INTERNAL_LIBRARY_PROPS = new Set(['componentSlug', 'registryDemo', 'registryElement', 'editorDetachedContent']);

export function inspectableLibraryProps(props: Record<string, WebDesignJsonValue>) {
  return Object.entries(props).filter(([key]) => !INTERNAL_LIBRARY_PROPS.has(key));
}

export function bindLibraryPreviewElement(component: WebDesignComponent, libraryDisplayName: string, selection: LibraryPreviewSelection): WebDesignComponent {
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

export function variantDifferenceLabels(variant: UiComponentVariant): string[] {
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

export const INTERACTIVE_COMPONENT_PREVIEWS = new Set([
  'ActionBar', 'AlertDialog', 'ColorPicker', 'Combobox', 'ContextMenu', 'DatePicker', 'Dialog', 'Drawer', 'Dropdown', 'DropdownMenu', 'Editable', 'FloatingPanel',
  'HoverCard', 'Menu', 'Modal', 'OverlayManager', 'Popover', 'Portal', 'Select', 'Sheet', 'Toast', 'ToggleTip', 'Tooltip', 'Tour'
]);

export const OPEN_OVERLAY_PREVIEWS = new Set([
  'ActionBar', 'AlertDialog', 'AutoComplete', 'Cascader', 'Combobox', 'ContextMenu', 'DatePicker', 'Dialog', 'Drawer', 'Dropdown', 'DropdownMenu', 'FloatingPanel',
  'HoverCard', 'Menu', 'Modal', 'OverlayManager', 'Popconfirm', 'Popover', 'Portal', 'Select', 'Sheet', 'Toast', 'ToggleTip', 'Tooltip', 'Tour', 'TreeSelect'
]);

export const WIDE_VARIANT_PREVIEWS = new Set(['OverlayManager']);

export function variantIsInteractive(variant: UiComponentVariant, componentId: string): boolean {
  return INTERACTIVE_COMPONENT_PREVIEWS.has(componentId)
    || variant.props.motion === true
    || ['hoverable', 'showSearch', 'multiple', 'allowClear', 'draggable', 'collapsible', 'editable', 'autoplay'].some((key) => variant.props[key] === true);
}

export function LazyVariantPreview({ children, minHeight }: { children: ReactNode; minHeight: number }) {
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

export function SelectableVariantCard({ component, previewHeight, className, interactive, variantLabel, differences, tokens, onPickItem, onPickPointerEvent }: {
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
