export type WebComponentType =
  | 'section' | 'text' | 'heading' | 'button' | 'link' | 'image' | 'icon' | 'logo' | 'card'
  | 'input' | 'textarea' | 'select' | 'checkbox' | 'switch' | 'divider' | 'badge' | 'avatar'
  | 'list' | 'table' | 'video';
export type WebDesignDevice = 'desktop' | 'tablet' | 'mobile';
export type WebDesignLibraryName = 'antd' | 'chakra' | 'shadcn' | 'magicui' | 'spell' | 'inspira' | 'daisyui';
export type WebContainerLayoutMode = 'free' | 'flex-row' | 'flex-column' | 'grid';
export type WebContainerAlign = 'start' | 'center' | 'end' | 'stretch';
export type WebContainerJustify = 'start' | 'center' | 'end' | 'space-between' | 'space-around';
export type AnnotationStatus = 'open' | 'resolved';
export type DesignRequestStatus = 'pending' | 'resolved';
export type WebSymbolOverride = 'content' | 'style' | 'frame';
export type WebHorizontalConstraint = 'auto' | 'left' | 'center' | 'right' | 'stretch' | 'scale';
export type WebComponentVisualState = 'hover' | 'active' | 'focus';

export interface WebDesignPage {
  id: string;
  name: string;
  slug: string;
}

export interface WebDesignAsset {
  id: string;
  name: string;
  mimeType: string;
  dataUrl: string;
  createdAt: string;
}

export interface WebDesignTokens {
  colors: {
    primary: string;
    accent: string;
    surface: string;
    text: string;
    muted: string;
  };
  radii: {
    small: number;
    medium: number;
    large: number;
  };
  typography: {
    fontFamily: string;
    baseFontSize: number;
  };
}

export interface WebDesignSymbol {
  id: string;
  name: string;
  rootIds: string[];
  components: WebDesignComponent[];
  createdAt: string;
}

export interface WebComponentStyle {
  background?: string;
  color?: string;
  borderColor?: string;
  borderWidth?: number;
  borderStyle?: 'solid' | 'dashed' | 'dotted' | 'double' | 'none';
  borderRadius?: number;
  padding?: number;
  fontSize?: number;
  fontWeight?: number;
  textAlign?: 'left' | 'center' | 'right';
  lineHeight?: number;
  letterSpacing?: number;
  textTransform?: 'none' | 'uppercase' | 'lowercase' | 'capitalize';
  textDecoration?: 'none' | 'underline' | 'line-through';
  opacity?: number;
  shadow?: string;
  blur?: number;
  backdropBlur?: number;
  rotate?: number;
  scale?: number;
  overflow?: 'visible' | 'hidden' | 'auto' | 'scroll';
  objectFit?: 'cover' | 'contain' | 'fill' | 'none' | 'scale-down';
  objectPosition?: string;
  mixBlendMode?: 'normal' | 'multiply' | 'screen' | 'overlay' | 'darken' | 'lighten' | 'difference';
  /**
   * Open-ended CSS escape hatch for visual properties that do not yet have a
   * dedicated inspector control. Keys may use CSS kebab-case, React camelCase,
   * or custom-property syntax (`--name`). Dedicated fields above still win in
   * the inspector, while custom CSS is applied last on the canvas.
   */
  customCss?: Record<string, string | number>;
}

export type WebComponentStates = Partial<Record<WebComponentVisualState, WebComponentStyle>>;

export interface WebDesignInteraction {
  type: 'page' | 'url';
  target: string;
}

export type WebDesignJsonValue = string | number | boolean | null | WebDesignJsonValue[] | { [key: string]: WebDesignJsonValue };

export interface WebDesignLibraryBinding {
  name: WebDesignLibraryName;
  version: string;
  component: string;
  variant?: string;
  props: Record<string, WebDesignJsonValue>;
}

export interface WebDesignAnnotation {
  id: string;
  text: string;
  status: AnnotationStatus;
  createdAt: string;
  resolvedAt?: string;
}

export interface WebDesignComponent {
  id: string;
  type: WebComponentType;
  name: string;
  pageId?: string;
  parentId?: string;
  slot?: string;
  symbolId?: string;
  symbolInstanceId?: string;
  symbolComponentId?: string;
  symbolOverrides?: WebSymbolOverride[];
  interaction?: WebDesignInteraction;
  library?: WebDesignLibraryBinding;
  x: number;
  y: number;
  width: number;
  height: number;
  zIndex: number;
  content: string;
  style: WebComponentStyle;
  states?: WebComponentStates;
  locked?: boolean;
  hidden?: boolean;
  layout?: WebContainerLayout;
  responsive?: Partial<Record<'tablet' | 'mobile', WebComponentResponsiveOverride>>;
  constraints?: Partial<Record<WebDesignDevice, WebComponentConstraints>>;
  annotations: WebDesignAnnotation[];
}

export interface WebComponentConstraints {
  horizontal: WebHorizontalConstraint;
  minWidth?: number;
  maxWidth?: number;
  minHeight?: number;
  maxHeight?: number;
  lockAspectRatio?: boolean;
}

export interface WebContainerLayout {
  mode: WebContainerLayoutMode;
  gap: number;
  padding: number;
  columns?: number;
  align?: WebContainerAlign;
  justify?: WebContainerJustify;
  wrap?: boolean;
}

export interface WebComponentResponsiveOverride {
  x: number;
  y: number;
  width: number;
  height: number;
  hidden?: boolean;
  style?: WebComponentStyle;
}

export interface WebDesignRequest {
  id: string;
  componentId?: string;
  instruction: string;
  status: DesignRequestStatus;
  createdAt: string;
  resolvedAt?: string;
  resolution?: string;
}

export interface WebDesignViewport {
  width: number;
  height: number;
  background: string;
}

export interface WebDesignBreakpoint {
  width: number;
  height: number;
  preview?: {
    presetId?: string;
    orientation: 'default' | 'rotated';
    viewportHeight: number;
  };
}

export interface WebDesignBreakpoints {
  desktop: WebDesignBreakpoint;
  tablet: WebDesignBreakpoint;
  mobile: WebDesignBreakpoint;
}

export interface WebDesignProject {
  schemaVersion: 1;
  projectId: string;
  name: string;
  description?: string;
  createdAt: string;
  updatedAt: string;
  designIds: string[];
}

export interface WebDesignProjectSummary {
  projectId: string;
  name: string;
  description?: string;
  designCount: number;
  designIds: string[];
  createdAt: string;
  updatedAt: string;
}

export interface WebDesignDocument {
  schemaVersion: 1;
  documentId: string;
  revision: number;
  title: string;
  description?: string;
  createdAt: string;
  updatedAt: string;
  viewport: WebDesignViewport;
  breakpoints?: WebDesignBreakpoints;
  pages?: WebDesignPage[];
  assets?: WebDesignAsset[];
  tokens?: WebDesignTokens;
  symbols?: WebDesignSymbol[];
  components: WebDesignComponent[];
  requests: WebDesignRequest[];
}

export type WebDesignPatchOperation =
  | { op: 'set_title'; title: string }
  | { op: 'set_description'; description: string }
  | { op: 'set_viewport'; viewport: WebDesignViewport }
  | { op: 'set_breakpoint'; device: WebDesignDevice; width: number; height: number }
  | { op: 'upsert_page'; page: WebDesignPage }
  | { op: 'remove_page'; pageId: string }
  | { op: 'upsert_asset'; asset: WebDesignAsset }
  | { op: 'remove_asset'; assetId: string }
  | { op: 'set_tokens'; tokens: WebDesignTokens }
  | { op: 'upsert_symbol'; symbol: WebDesignSymbol }
  | { op: 'remove_symbol'; symbolId: string }
  | { op: 'upsert_component'; component: WebDesignComponent }
  | { op: 'remove_component'; componentId: string }
  | { op: 'set_parent'; componentId: string; parentId?: string; slot?: string }
  | { op: 'set_layout'; componentId: string; layout: WebContainerLayout }
  | { op: 'move_component'; componentId: string; x: number; y: number; device?: WebDesignDevice }
  | { op: 'resize_component'; componentId: string; width: number; height: number; device?: WebDesignDevice }
  | { op: 'update_component'; componentId: string; device?: WebDesignDevice; changes: Partial<Pick<WebDesignComponent, 'name' | 'content' | 'zIndex' | 'style' | 'states' | 'locked' | 'hidden' | 'symbolOverrides' | 'constraints'>> & { interaction?: WebDesignInteraction | null } }
  | { op: 'add_annotation'; componentId: string; annotation: WebDesignAnnotation }
  | { op: 'resolve_annotation'; componentId: string; annotationId: string }
  | { op: 'add_request'; request: WebDesignRequest }
  | { op: 'resolve_request'; requestId: string; resolution?: string };

const identifierPattern = /^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$/;
const componentTypes = new Set<WebComponentType>([
  'section', 'text', 'heading', 'button', 'link', 'image', 'icon', 'logo', 'card',
  'input', 'textarea', 'select', 'checkbox', 'switch', 'divider', 'badge', 'avatar',
  'list', 'table', 'video'
]);
const devices = new Set<WebDesignDevice>(['desktop', 'tablet', 'mobile']);
const libraryNames = new Set<WebDesignLibraryName>(['antd', 'chakra', 'shadcn', 'magicui', 'spell', 'inspira', 'daisyui']);
const layoutModes = new Set<WebContainerLayoutMode>(['free', 'flex-row', 'flex-column', 'grid']);
const layoutAlignments = new Set<WebContainerAlign>(['start', 'center', 'end', 'stretch']);
const layoutJustifications = new Set<WebContainerJustify>(['start', 'center', 'end', 'space-between', 'space-around']);
const symbolOverrides = new Set<WebSymbolOverride>(['content', 'style', 'frame']);
const horizontalConstraints = new Set<WebHorizontalConstraint>(['auto', 'left', 'center', 'right', 'stretch', 'scale']);
const visualStates = new Set<WebComponentVisualState>(['hover', 'active', 'focus']);

export const DEFAULT_WEB_DESIGN_PAGE: WebDesignPage = { id: 'home', name: '首页', slug: '/' };
export const DEFAULT_WEB_DESIGN_TOKENS: WebDesignTokens = {
  colors: { primary: '#007AFF', accent: '#34C759', surface: '#FFFFFF', text: '#1D1D1F', muted: '#6E6E73' },
  radii: { small: 8, medium: 14, large: 24 },
  typography: { fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif', baseFontSize: 16 }
};

export const DEFAULT_WEB_DESIGN_BREAKPOINTS: WebDesignBreakpoints = {
  desktop: { width: 1200, height: 940 },
  tablet: { width: 768, height: 1100 },
  mobile: { width: 390, height: 844 }
};

export function assertIdentifier(value: string, label: string): void {
  if (!identifierPattern.test(value)) throw new Error(`${label} must use letters, digits, hyphens, or underscores.`);
}

export function assertWebDesignProject(value: unknown): asserts value is WebDesignProject {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Web design project must be an object.');
  const project = value as WebDesignProject;
  if (project.schemaVersion !== 1) throw new Error('Unsupported web design project schema version.');
  assertIdentifier(project.projectId, 'projectId');
  if (typeof project.name !== 'string' || !project.name.trim() || project.name.length > 240) {
    throw new Error('Project name must contain 1 to 240 characters.');
  }
  if (project.description !== undefined && (typeof project.description !== 'string' || project.description.length > 4000)) {
    throw new Error('Project description is invalid.');
  }
  if (!Array.isArray(project.designIds) || project.designIds.length > 5000) throw new Error('Project designIds are invalid.');
  const ids = new Set<string>();
  for (const designId of project.designIds) {
    if (typeof designId !== 'string') throw new Error('Project designIds must contain identifiers.');
    assertIdentifier(designId, 'designId');
    if (ids.has(designId)) throw new Error(`Duplicate project designId: ${designId}`);
    ids.add(designId);
  }
  if (typeof project.createdAt !== 'string' || typeof project.updatedAt !== 'string') throw new Error('Project timestamps are required.');
}

export function webDesignProjectSummary(project: WebDesignProject): WebDesignProjectSummary {
  return {
    projectId: project.projectId,
    name: project.name,
    description: project.description,
    designCount: project.designIds.length,
    designIds: [...project.designIds],
    createdAt: project.createdAt,
    updatedAt: project.updatedAt
  };
}

function assertPage(value: unknown): asserts value is WebDesignPage {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Page must be an object.');
  const page = value as WebDesignPage;
  assertIdentifier(page.id, 'page.id');
  if (typeof page.name !== 'string' || !page.name.trim() || page.name.length > 120) throw new Error('Page name is invalid.');
  if (typeof page.slug !== 'string' || !page.slug.startsWith('/') || page.slug.length > 240 || /\s/.test(page.slug)) {
    throw new Error('Page slug must start with / and contain no spaces.');
  }
}

function assertAsset(value: unknown): asserts value is WebDesignAsset {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Asset must be an object.');
  const asset = value as WebDesignAsset;
  assertIdentifier(asset.id, 'asset.id');
  if (typeof asset.name !== 'string' || !asset.name.trim() || asset.name.length > 240) throw new Error('Asset name is invalid.');
  if (typeof asset.mimeType !== 'string' || !asset.mimeType.startsWith('image/') || asset.mimeType.length > 120) throw new Error('Asset mimeType is invalid.');
  if (typeof asset.dataUrl !== 'string' || !asset.dataUrl.startsWith(`data:${asset.mimeType};base64,`) || asset.dataUrl.length > 15_000_000) {
    throw new Error('Asset dataUrl is invalid or too large.');
  }
  if (typeof asset.createdAt !== 'string') throw new Error('Asset createdAt is required.');
}

function assertTokens(value: unknown): asserts value is WebDesignTokens {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Design tokens must be an object.');
  const tokens = value as WebDesignTokens;
  if (!tokens.colors || !tokens.radii || !tokens.typography) throw new Error('Design token groups are required.');
  for (const key of ['primary', 'accent', 'surface', 'text', 'muted'] as const) {
    if (typeof tokens.colors[key] !== 'string' || !/^#[0-9a-fA-F]{6}$/.test(tokens.colors[key])) throw new Error(`Token color ${key} must be a six-digit hex color.`);
  }
  for (const key of ['small', 'medium', 'large'] as const) finiteNumber(tokens.radii[key], `tokens.radii.${key}`, 0, 999);
  if (typeof tokens.typography.fontFamily !== 'string' || !tokens.typography.fontFamily.trim() || tokens.typography.fontFamily.length > 300) {
    throw new Error('Token fontFamily is invalid.');
  }
  finiteNumber(tokens.typography.baseFontSize, 'tokens.typography.baseFontSize', 6, 120);
}

function assertSymbol(value: unknown): asserts value is WebDesignSymbol {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Reusable component must be an object.');
  const symbol = value as WebDesignSymbol;
  assertIdentifier(symbol.id, 'symbol.id');
  if (typeof symbol.name !== 'string' || !symbol.name.trim() || symbol.name.length > 240) throw new Error('Reusable component name is invalid.');
  if (!Array.isArray(symbol.components) || symbol.components.length === 0 || symbol.components.length > 1000) throw new Error('Reusable component contents are invalid.');
  symbol.components.forEach(assertComponent);
  const ids = new Set(symbol.components.map((component) => component.id));
  if (ids.size !== symbol.components.length) throw new Error(`Reusable component contains duplicate component IDs: ${symbol.id}`);
  if (!Array.isArray(symbol.rootIds) || symbol.rootIds.length === 0 || symbol.rootIds.some((id) => !ids.has(id))) throw new Error('Reusable component root IDs are invalid.');
  for (const component of symbol.components) {
    if (component.parentId && !ids.has(component.parentId)) throw new Error(`Reusable component references missing parent: ${component.parentId}`);
    const visited = new Set<string>([component.id]);
    let parentId = component.parentId;
    while (parentId) {
      if (visited.has(parentId)) throw new Error(`Reusable component parent cycle detected: ${component.id}`);
      visited.add(parentId);
      parentId = symbol.components.find((candidate) => candidate.id === parentId)?.parentId;
    }
  }
  if (typeof symbol.createdAt !== 'string') throw new Error('Reusable component createdAt is required.');
}

function finiteNumber(value: unknown, label: string, min: number, max: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max) {
    throw new Error(`${label} must be between ${min} and ${max}.`);
  }
  return value;
}

function assertStyle(value: unknown): asserts value is WebComponentStyle {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Component style must be an object.');
  const style = value as WebComponentStyle;
  for (const key of ['background', 'color', 'borderColor', 'shadow', 'objectPosition'] as const) {
    if (style[key] !== undefined && (typeof style[key] !== 'string' || style[key]!.length > 300)) {
      throw new Error(`Component style ${key} is invalid.`);
    }
  }
  if (style.borderWidth !== undefined) finiteNumber(style.borderWidth, 'borderWidth', 0, 40);
  if (style.borderRadius !== undefined) finiteNumber(style.borderRadius, 'borderRadius', 0, 999);
  if (style.padding !== undefined) finiteNumber(style.padding, 'padding', 0, 2000);
  if (style.fontSize !== undefined) finiteNumber(style.fontSize, 'fontSize', 6, 240);
  if (style.fontWeight !== undefined) finiteNumber(style.fontWeight, 'fontWeight', 100, 1000);
  if (style.lineHeight !== undefined) finiteNumber(style.lineHeight, 'lineHeight', 0.5, 5);
  if (style.letterSpacing !== undefined) finiteNumber(style.letterSpacing, 'letterSpacing', -20, 100);
  if (style.opacity !== undefined) finiteNumber(style.opacity, 'opacity', 0, 1);
  if (style.blur !== undefined) finiteNumber(style.blur, 'blur', 0, 200);
  if (style.backdropBlur !== undefined) finiteNumber(style.backdropBlur, 'backdropBlur', 0, 200);
  if (style.rotate !== undefined) finiteNumber(style.rotate, 'rotate', -360, 360);
  if (style.scale !== undefined) finiteNumber(style.scale, 'scale', 0.01, 20);
  if (style.textAlign !== undefined && !['left', 'center', 'right'].includes(style.textAlign)) {
    throw new Error('Component textAlign is invalid.');
  }
  if (style.borderStyle !== undefined && !['solid', 'dashed', 'dotted', 'double', 'none'].includes(style.borderStyle)) throw new Error('Component borderStyle is invalid.');
  if (style.textTransform !== undefined && !['none', 'uppercase', 'lowercase', 'capitalize'].includes(style.textTransform)) throw new Error('Component textTransform is invalid.');
  if (style.textDecoration !== undefined && !['none', 'underline', 'line-through'].includes(style.textDecoration)) throw new Error('Component textDecoration is invalid.');
  if (style.overflow !== undefined && !['visible', 'hidden', 'auto', 'scroll'].includes(style.overflow)) throw new Error('Component overflow is invalid.');
  if (style.objectFit !== undefined && !['cover', 'contain', 'fill', 'none', 'scale-down'].includes(style.objectFit)) throw new Error('Component objectFit is invalid.');
  if (style.mixBlendMode !== undefined && !['normal', 'multiply', 'screen', 'overlay', 'darken', 'lighten', 'difference'].includes(style.mixBlendMode)) throw new Error('Component mixBlendMode is invalid.');
  if (style.customCss !== undefined) {
    if (!style.customCss || typeof style.customCss !== 'object' || Array.isArray(style.customCss)) throw new Error('Component customCss must be an object.');
    const entries = Object.entries(style.customCss);
    if (entries.length > 80) throw new Error('Component customCss supports at most 80 declarations.');
    for (const [property, propertyValue] of entries) {
      if (['__proto__', 'prototype', 'constructor'].includes(property) || !/^(?:--[a-zA-Z0-9_-]{1,80}|[a-zA-Z][a-zA-Z0-9-]{0,80})$/.test(property)) {
        throw new Error(`Component customCss property is invalid: ${property}`);
      }
      if (typeof propertyValue === 'number') {
        if (!Number.isFinite(propertyValue)) throw new Error(`Component customCss value is invalid: ${property}`);
      } else if (typeof propertyValue !== 'string' || propertyValue.length > 1000) {
        throw new Error(`Component customCss value is invalid: ${property}`);
      }
    }
  }
}

function assertResponsiveOverride(value: unknown, label: string): asserts value is WebComponentResponsiveOverride {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${label} must be an object.`);
  const frame = value as WebComponentResponsiveOverride;
  finiteNumber(frame.x, `${label}.x`, -100000, 100000);
  finiteNumber(frame.y, `${label}.y`, -100000, 100000);
  finiteNumber(frame.width, `${label}.width`, 16, 100000);
  finiteNumber(frame.height, `${label}.height`, 16, 100000);
  if (frame.hidden !== undefined && typeof frame.hidden !== 'boolean') throw new Error(`${label}.hidden is invalid.`);
  if (frame.style !== undefined) assertStyle(frame.style);
}

function assertStates(value: unknown): asserts value is WebComponentStates {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Component states must be an object.');
  const states = value as WebComponentStates;
  for (const state of visualStates) if (states[state] !== undefined) assertStyle(states[state]);
  for (const key of Object.keys(states)) if (!visualStates.has(key as WebComponentVisualState)) throw new Error(`Unsupported component visual state: ${key}`);
}

function assertContainerLayout(value: unknown): asserts value is WebContainerLayout {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Component layout must be an object.');
  const layout = value as WebContainerLayout;
  if (!layoutModes.has(layout.mode)) throw new Error('Component layout mode is invalid.');
  finiteNumber(layout.gap, 'layout.gap', 0, 1000);
  finiteNumber(layout.padding, 'layout.padding', 0, 2000);
  if (layout.columns !== undefined) {
    finiteNumber(layout.columns, 'layout.columns', 1, 100);
    if (!Number.isInteger(layout.columns)) throw new Error('Component layout columns must be an integer.');
  }
  if (layout.align !== undefined && !layoutAlignments.has(layout.align)) throw new Error('Component layout alignment is invalid.');
  if (layout.justify !== undefined && !layoutJustifications.has(layout.justify)) throw new Error('Component layout justification is invalid.');
  if (layout.wrap !== undefined && typeof layout.wrap !== 'boolean') throw new Error('Component layout wrap is invalid.');
}

export function assertAnnotation(value: unknown): asserts value is WebDesignAnnotation {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Annotation must be an object.');
  const annotation = value as WebDesignAnnotation;
  assertIdentifier(annotation.id, 'annotation.id');
  if (typeof annotation.text !== 'string' || !annotation.text.trim() || annotation.text.length > 4000) {
    throw new Error('Annotation text must contain 1 to 4000 characters.');
  }
  if (!['open', 'resolved'].includes(annotation.status)) throw new Error('Annotation status is invalid.');
  if (typeof annotation.createdAt !== 'string') throw new Error('Annotation createdAt is required.');
}

export function assertComponent(value: unknown): asserts value is WebDesignComponent {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Component must be an object.');
  const component = value as WebDesignComponent;
  assertIdentifier(component.id, 'component.id');
  if (component.pageId !== undefined) assertIdentifier(component.pageId, 'component.pageId');
  if (component.parentId !== undefined) assertIdentifier(component.parentId, 'component.parentId');
  if (component.slot !== undefined) assertIdentifier(component.slot, 'component.slot');
  if (component.slot !== undefined && component.parentId === undefined) throw new Error('component.slot requires component.parentId.');
  if (component.symbolId !== undefined) assertIdentifier(component.symbolId, 'component.symbolId');
  if (component.symbolInstanceId !== undefined) assertIdentifier(component.symbolInstanceId, 'component.symbolInstanceId');
  if (component.symbolComponentId !== undefined) assertIdentifier(component.symbolComponentId, 'component.symbolComponentId');
  if ((component.symbolInstanceId === undefined) !== (component.symbolComponentId === undefined)) {
    throw new Error('Component symbol instance and source IDs must be provided together.');
  }
  if (component.symbolInstanceId !== undefined && component.symbolId === undefined) throw new Error('Component symbol instance requires symbolId.');
  if (component.symbolOverrides !== undefined) {
    if (!Array.isArray(component.symbolOverrides) || component.symbolOverrides.length > symbolOverrides.size
      || component.symbolOverrides.some((override) => !symbolOverrides.has(override))
      || new Set(component.symbolOverrides).size !== component.symbolOverrides.length) {
      throw new Error('Component symbol overrides are invalid.');
    }
  }
  if (component.interaction !== undefined) {
    if (!component.interaction || typeof component.interaction !== 'object' || Array.isArray(component.interaction)) throw new Error('Component interaction is invalid.');
    if (!['page', 'url'].includes(component.interaction.type)) throw new Error('Component interaction type is invalid.');
    if (typeof component.interaction.target !== 'string' || !component.interaction.target.trim() || component.interaction.target.length > 2048) {
      throw new Error('Component interaction target is invalid.');
    }
    if (component.interaction.type === 'page') assertIdentifier(component.interaction.target, 'component.interaction.target');
    if (component.interaction.type === 'url' && !/^https?:\/\//i.test(component.interaction.target)) {
      throw new Error('Component URL interaction must use http or https.');
    }
  }
  if (component.library !== undefined) {
    if (!component.library || typeof component.library !== 'object' || Array.isArray(component.library)) throw new Error('Component library binding is invalid.');
    if (!libraryNames.has(component.library.name)) throw new Error('Component library is unsupported.');
    if (typeof component.library.version !== 'string' || !component.library.version.trim() || component.library.version.length > 32) throw new Error('Component library version is invalid.');
    if (typeof component.library.component !== 'string' || !/^[A-Za-z][A-Za-z0-9.]{0,63}$/.test(component.library.component)) throw new Error('Component library component is invalid.');
    if (component.library.variant !== undefined && (typeof component.library.variant !== 'string' || !/^[a-z0-9][a-z0-9-]{0,63}$/i.test(component.library.variant))) throw new Error('Component library variant is invalid.');
    if (!component.library.props || typeof component.library.props !== 'object' || Array.isArray(component.library.props)) throw new Error('Component library props are invalid.');
    let serializedProps: string;
    try {
      serializedProps = JSON.stringify(component.library.props);
    } catch {
      throw new Error('Component library props must be JSON serializable.');
    }
    if (serializedProps.length > 50000) throw new Error('Component library props are too large.');
  }
  if (!componentTypes.has(component.type)) throw new Error('Component type is invalid.');
  if (typeof component.name !== 'string' || !component.name.trim() || component.name.length > 240) {
    throw new Error('Component name must contain 1 to 240 characters.');
  }
  finiteNumber(component.x, 'component.x', -100000, 100000);
  finiteNumber(component.y, 'component.y', -100000, 100000);
  finiteNumber(component.width, 'component.width', 16, 100000);
  finiteNumber(component.height, 'component.height', 16, 100000);
  finiteNumber(component.zIndex, 'component.zIndex', -10000, 10000);
  if (typeof component.content !== 'string' || component.content.length > 100000) throw new Error('Component content is invalid.');
  assertStyle(component.style);
  if (component.states !== undefined) assertStates(component.states);
  if (component.locked !== undefined && typeof component.locked !== 'boolean') throw new Error('Component locked is invalid.');
  if (component.hidden !== undefined && typeof component.hidden !== 'boolean') throw new Error('Component hidden is invalid.');
  if (component.layout !== undefined) assertContainerLayout(component.layout);
  if (component.responsive !== undefined) {
    if (!component.responsive || typeof component.responsive !== 'object' || Array.isArray(component.responsive)) {
      throw new Error('Component responsive overrides are invalid.');
    }
    if (component.responsive.tablet !== undefined) assertResponsiveOverride(component.responsive.tablet, 'component.responsive.tablet');
    if (component.responsive.mobile !== undefined) assertResponsiveOverride(component.responsive.mobile, 'component.responsive.mobile');
  }
  if (component.constraints !== undefined) {
    if (!component.constraints || typeof component.constraints !== 'object' || Array.isArray(component.constraints)) throw new Error('Component responsive constraints are invalid.');
    for (const device of devices) {
      const constraint = component.constraints[device];
      if (constraint === undefined) continue;
      if (!constraint || typeof constraint !== 'object' || Array.isArray(constraint) || !horizontalConstraints.has(constraint.horizontal)) {
        throw new Error(`Component ${device} horizontal constraint is invalid.`);
      }
      for (const key of ['minWidth', 'maxWidth', 'minHeight', 'maxHeight'] as const) {
        if (constraint[key] !== undefined) finiteNumber(constraint[key], `component.constraints.${device}.${key}`, 16, 100000);
      }
      if (constraint.minWidth !== undefined && constraint.maxWidth !== undefined && constraint.minWidth > constraint.maxWidth) throw new Error(`Component ${device} width constraints are invalid.`);
      if (constraint.minHeight !== undefined && constraint.maxHeight !== undefined && constraint.minHeight > constraint.maxHeight) throw new Error(`Component ${device} height constraints are invalid.`);
      if (constraint.lockAspectRatio !== undefined && typeof constraint.lockAspectRatio !== 'boolean') throw new Error(`Component ${device} aspect ratio constraint is invalid.`);
    }
  }
  if (!Array.isArray(component.annotations) || component.annotations.length > 1000) throw new Error('Component annotations are invalid.');
  component.annotations.forEach(assertAnnotation);
}

export function assertRequest(value: unknown): asserts value is WebDesignRequest {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Design request must be an object.');
  const request = value as WebDesignRequest;
  assertIdentifier(request.id, 'request.id');
  if (request.componentId !== undefined) assertIdentifier(request.componentId, 'request.componentId');
  if (typeof request.instruction !== 'string' || !request.instruction.trim() || request.instruction.length > 12000) {
    throw new Error('Design request instruction must contain 1 to 12000 characters.');
  }
  if (!['pending', 'resolved'].includes(request.status)) throw new Error('Design request status is invalid.');
  if (typeof request.createdAt !== 'string') throw new Error('Design request createdAt is required.');
}

export function assertWebDesignDocument(value: unknown): asserts value is WebDesignDocument {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Web design document must be an object.');
  const document = value as WebDesignDocument;
  if (document.schemaVersion !== 1) throw new Error('Unsupported web design schema version.');
  assertIdentifier(document.documentId, 'documentId');
  if (!Number.isSafeInteger(document.revision) || document.revision < 0) throw new Error('Document revision is invalid.');
  if (typeof document.title !== 'string' || !document.title.trim() || document.title.length > 240) {
    throw new Error('Document title must contain 1 to 240 characters.');
  }
  if (document.description !== undefined && (typeof document.description !== 'string' || document.description.length > 12000)) {
    throw new Error('Document description is invalid.');
  }
  if (typeof document.createdAt !== 'string' || typeof document.updatedAt !== 'string') throw new Error('Document timestamps are required.');
  if (!document.viewport || typeof document.viewport !== 'object') throw new Error('Document viewport is required.');
  finiteNumber(document.viewport.width, 'viewport.width', 320, 10000);
  finiteNumber(document.viewport.height, 'viewport.height', 320, 30000);
  if (typeof document.viewport.background !== 'string' || document.viewport.background.length > 200) throw new Error('Viewport background is invalid.');
  if (document.breakpoints !== undefined) {
    if (!document.breakpoints || typeof document.breakpoints !== 'object' || Array.isArray(document.breakpoints)) {
      throw new Error('Document breakpoints are invalid.');
    }
    for (const device of devices) {
      const breakpoint = document.breakpoints[device];
      if (!breakpoint || typeof breakpoint !== 'object') throw new Error(`Breakpoint ${device} is required.`);
      finiteNumber(breakpoint.width, `breakpoints.${device}.width`, 320, 10000);
      finiteNumber(breakpoint.height, `breakpoints.${device}.height`, 320, 30000);
      if (breakpoint.preview !== undefined) {
        if (!breakpoint.preview || typeof breakpoint.preview !== 'object' || Array.isArray(breakpoint.preview)) throw new Error(`breakpoints.${device}.preview is invalid.`);
        if (breakpoint.preview.presetId !== undefined) assertIdentifier(breakpoint.preview.presetId, `breakpoints.${device}.preview.presetId`);
        if (breakpoint.preview.orientation !== 'default' && breakpoint.preview.orientation !== 'rotated') throw new Error(`breakpoints.${device}.preview.orientation is invalid.`);
        finiteNumber(breakpoint.preview.viewportHeight, `breakpoints.${device}.preview.viewportHeight`, 320, 10000);
      }
    }
  }
  const pages = document.pages ?? [DEFAULT_WEB_DESIGN_PAGE];
  if (!Array.isArray(pages) || pages.length === 0 || pages.length > 200) throw new Error('Document pages are invalid.');
  pages.forEach(assertPage);
  const pageIds = new Set<string>();
  const pageSlugs = new Set<string>();
  for (const page of pages) {
    if (pageIds.has(page.id)) throw new Error(`Duplicate page id: ${page.id}`);
    if (pageSlugs.has(page.slug)) throw new Error(`Duplicate page slug: ${page.slug}`);
    pageIds.add(page.id);
    pageSlugs.add(page.slug);
  }
  if (document.assets !== undefined) {
    if (!Array.isArray(document.assets) || document.assets.length > 200) throw new Error('Document assets are invalid.');
    document.assets.forEach(assertAsset);
    const assetIds = new Set<string>();
    for (const asset of document.assets) {
      if (assetIds.has(asset.id)) throw new Error(`Duplicate asset id: ${asset.id}`);
      assetIds.add(asset.id);
    }
  }
  if (document.tokens !== undefined) assertTokens(document.tokens);
  if (document.symbols !== undefined) {
    if (!Array.isArray(document.symbols) || document.symbols.length > 500) throw new Error('Document reusable components are invalid.');
    document.symbols.forEach(assertSymbol);
    const symbolIds = new Set<string>();
    for (const symbol of document.symbols) {
      if (symbolIds.has(symbol.id)) throw new Error(`Duplicate reusable component id: ${symbol.id}`);
      symbolIds.add(symbol.id);
      for (const component of symbol.components) {
        if (component.interaction?.type === 'page' && !pageIds.has(component.interaction.target)) {
          throw new Error(`Reusable component interaction references missing page: ${component.interaction.target}`);
        }
      }
    }
  }
  if (!Array.isArray(document.components) || document.components.length > 10000) throw new Error('Document components are invalid.');
  document.components.forEach(assertComponent);
  const ids = new Set<string>();
  for (const component of document.components) {
    if (ids.has(component.id)) throw new Error(`Duplicate component id: ${component.id}`);
    ids.add(component.id);
  }
  for (const component of document.components) {
    const componentPageId = component.pageId ?? pages[0].id;
    if (!pageIds.has(componentPageId)) throw new Error(`Component references missing page: ${componentPageId}`);
    if (component.parentId && !ids.has(component.parentId)) throw new Error(`Component references missing parent: ${component.parentId}`);
    if (component.parentId === component.id) throw new Error(`Component cannot parent itself: ${component.id}`);
    if (component.symbolId && !document.symbols?.some((symbol) => symbol.id === component.symbolId)) {
      throw new Error(`Component references missing reusable component: ${component.symbolId}`);
    }
    if (component.symbolComponentId) {
      const symbol = document.symbols?.find((candidate) => candidate.id === component.symbolId);
      if (!symbol?.components.some((candidate) => candidate.id === component.symbolComponentId)) {
        throw new Error(`Component references missing reusable component layer: ${component.symbolComponentId}`);
      }
    }
    if (component.interaction?.type === 'page' && !pageIds.has(component.interaction.target)) {
      throw new Error(`Component interaction references missing page: ${component.interaction.target}`);
    }
    const visited = new Set<string>([component.id]);
    let parentId = component.parentId;
    while (parentId) {
      if (visited.has(parentId)) throw new Error(`Component parent cycle detected at: ${component.id}`);
      visited.add(parentId);
      const parent = document.components.find((candidate) => candidate.id === parentId);
      if (parent && (parent.pageId ?? pages[0].id) !== componentPageId) throw new Error(`Component parent must be on the same page: ${component.id}`);
      parentId = parent?.parentId;
    }
  }
  if (!Array.isArray(document.requests) || document.requests.length > 10000) throw new Error('Document requests are invalid.');
  document.requests.forEach(assertRequest);
  for (const request of document.requests) {
    if (request.componentId && !ids.has(request.componentId)) throw new Error(`Request references missing component: ${request.componentId}`);
  }
}

export function designSummary(document: WebDesignDocument) {
  return {
    documentId: document.documentId,
    revision: document.revision,
    title: document.title,
    componentCount: document.components.length,
    pageCount: document.pages?.length ?? 1,
    pendingRequestCount: document.requests.filter((request) => request.status === 'pending').length,
    updatedAt: document.updatedAt
  };
}

export function pagesForDocument(document: WebDesignDocument): WebDesignPage[] {
  return document.pages?.length ? document.pages : [DEFAULT_WEB_DESIGN_PAGE];
}

export function pageIdForComponent(document: WebDesignDocument, component: WebDesignComponent): string {
  return component.pageId ?? pagesForDocument(document)[0].id;
}

export function tokensForDocument(document: WebDesignDocument): WebDesignTokens {
  return document.tokens ?? DEFAULT_WEB_DESIGN_TOKENS;
}

function requiredComponent(document: WebDesignDocument, componentId: string): WebDesignComponent {
  const component = document.components.find((candidate) => candidate.id === componentId);
  if (!component) throw new Error(`Component not found: ${componentId}`);
  return component;
}

function assertDevice(device: WebDesignDevice | undefined): WebDesignDevice {
  const resolved = device ?? 'desktop';
  if (!devices.has(resolved)) throw new Error(`Unsupported design device: ${resolved}`);
  return resolved;
}

function responsiveOverride(component: WebDesignComponent, device: Exclude<WebDesignDevice, 'desktop'>): WebComponentResponsiveOverride {
  const current = component.responsive?.[device];
  if (current) return current;
  const created: WebComponentResponsiveOverride = {
    x: component.x,
    y: component.y,
    width: component.width,
    height: component.height
  };
  component.responsive = { ...component.responsive, [device]: created };
  return created;
}

export function applyWebDesignPatch(document: WebDesignDocument, operations: WebDesignPatchOperation[]): WebDesignDocument {
  if (!Array.isArray(operations) || operations.length === 0 || operations.length > 1000) {
    throw new Error('Patch operations must contain 1 to 1000 items.');
  }
  const next = structuredClone(document);
  for (const operation of operations) {
    switch (operation.op) {
      case 'set_title':
        next.title = operation.title.trim();
        break;
      case 'set_description':
        next.description = operation.description.trim() || undefined;
        break;
      case 'set_viewport':
        next.viewport = { ...operation.viewport };
        break;
      case 'set_breakpoint': {
        const device = assertDevice(operation.device);
        const breakpoints = structuredClone(next.breakpoints ?? {
          ...DEFAULT_WEB_DESIGN_BREAKPOINTS,
          desktop: { width: next.viewport.width, height: next.viewport.height }
        });
        breakpoints[device] = { ...breakpoints[device], width: operation.width, height: operation.height };
        next.breakpoints = breakpoints;
        if (device === 'desktop') {
          next.viewport.width = operation.width;
          next.viewport.height = operation.height;
        }
        break;
      }
      case 'upsert_page': {
        assertPage(operation.page);
        const pages = structuredClone(next.pages ?? [DEFAULT_WEB_DESIGN_PAGE]);
        const index = pages.findIndex((page) => page.id === operation.page.id);
        if (index >= 0) pages[index] = structuredClone(operation.page);
        else pages.push(structuredClone(operation.page));
        next.pages = pages;
        break;
      }
      case 'remove_page': {
        const pages = structuredClone(next.pages ?? [DEFAULT_WEB_DESIGN_PAGE]);
        if (pages.length <= 1) throw new Error('A design document must keep at least one page.');
        if (!pages.some((page) => page.id === operation.pageId)) throw new Error(`Page not found: ${operation.pageId}`);
        next.pages = pages.filter((page) => page.id !== operation.pageId);
        next.components = next.components.filter((component) => (component.pageId ?? pages[0].id) !== operation.pageId);
        next.components = next.components.map((component) => component.interaction?.type === 'page' && component.interaction.target === operation.pageId
          ? { ...component, interaction: undefined }
          : component);
        next.symbols = next.symbols?.map((symbol) => ({
          ...symbol,
          components: symbol.components.map((component) => component.interaction?.type === 'page' && component.interaction.target === operation.pageId
            ? { ...component, interaction: undefined }
            : component)
        }));
        const remainingIds = new Set(next.components.map((component) => component.id));
        next.requests = next.requests.filter((request) => !request.componentId || remainingIds.has(request.componentId));
        break;
      }
      case 'upsert_asset': {
        assertAsset(operation.asset);
        const assets = structuredClone(next.assets ?? []);
        const index = assets.findIndex((asset) => asset.id === operation.asset.id);
        if (index >= 0) assets[index] = structuredClone(operation.asset);
        else assets.push(structuredClone(operation.asset));
        next.assets = assets;
        break;
      }
      case 'remove_asset':
        next.assets = (next.assets ?? []).filter((asset) => asset.id !== operation.assetId);
        break;
      case 'set_tokens':
        assertTokens(operation.tokens);
        next.tokens = structuredClone(operation.tokens);
        break;
      case 'upsert_symbol': {
        assertSymbol(operation.symbol);
        const symbols = structuredClone(next.symbols ?? []);
        const index = symbols.findIndex((symbol) => symbol.id === operation.symbol.id);
        if (index >= 0) symbols[index] = structuredClone(operation.symbol);
        else symbols.push(structuredClone(operation.symbol));
        next.symbols = symbols;
        break;
      }
      case 'remove_symbol':
        next.symbols = (next.symbols ?? []).filter((symbol) => symbol.id !== operation.symbolId);
        next.components = next.components.map((component) => component.symbolId === operation.symbolId ? {
          ...component,
          symbolId: undefined,
          symbolInstanceId: undefined,
          symbolComponentId: undefined,
          symbolOverrides: undefined
        } : component);
        break;
      case 'upsert_component': {
        assertComponent(operation.component);
        const index = next.components.findIndex((component) => component.id === operation.component.id);
        if (index >= 0) next.components[index] = structuredClone(operation.component);
        else next.components.push(structuredClone(operation.component));
        break;
      }
      case 'remove_component':
        next.components = next.components.filter((component) => component.id !== operation.componentId);
        next.components = next.components.map((component) => component.parentId === operation.componentId ? { ...component, parentId: undefined, slot: undefined } : component);
        next.requests = next.requests.filter((request) => request.componentId !== operation.componentId);
        break;
      case 'set_parent': {
        const component = requiredComponent(next, operation.componentId);
        if (operation.parentId !== undefined) requiredComponent(next, operation.parentId);
        component.parentId = operation.parentId;
        component.slot = operation.parentId ? operation.slot : undefined;
        break;
      }
      case 'set_layout':
        requiredComponent(next, operation.componentId).layout = structuredClone(operation.layout);
        break;
      case 'move_component': {
        const component = requiredComponent(next, operation.componentId);
        const device = assertDevice(operation.device);
        if (device === 'desktop') {
          component.x = operation.x;
          component.y = operation.y;
        } else {
          const override = responsiveOverride(component, device);
          override.x = operation.x;
          override.y = operation.y;
        }
        break;
      }
      case 'resize_component': {
        const component = requiredComponent(next, operation.componentId);
        const device = assertDevice(operation.device);
        if (device === 'desktop') {
          component.width = operation.width;
          component.height = operation.height;
        } else {
          const override = responsiveOverride(component, device);
          override.width = operation.width;
          override.height = operation.height;
        }
        break;
      }
      case 'update_component': {
        const component = requiredComponent(next, operation.componentId);
        if (operation.changes.name !== undefined) component.name = operation.changes.name;
        if (operation.changes.content !== undefined) component.content = operation.changes.content;
        if (operation.changes.zIndex !== undefined) component.zIndex = operation.changes.zIndex;
        if (operation.changes.locked !== undefined) component.locked = operation.changes.locked;
        if (operation.changes.symbolOverrides !== undefined) component.symbolOverrides = [...operation.changes.symbolOverrides];
        if (operation.changes.constraints !== undefined) component.constraints = structuredClone(operation.changes.constraints);
        if (operation.changes.states !== undefined) component.states = structuredClone(operation.changes.states);
        if ('interaction' in operation.changes) component.interaction = operation.changes.interaction ? structuredClone(operation.changes.interaction) : undefined;
        const device = assertDevice(operation.device);
        if (device === 'desktop') {
          if (operation.changes.hidden !== undefined) component.hidden = operation.changes.hidden;
          if (operation.changes.style !== undefined) component.style = { ...component.style, ...operation.changes.style };
        } else {
          const override = responsiveOverride(component, device);
          if (operation.changes.hidden !== undefined) override.hidden = operation.changes.hidden;
          if (operation.changes.style !== undefined) override.style = { ...override.style, ...operation.changes.style };
        }
        break;
      }
      case 'add_annotation':
        requiredComponent(next, operation.componentId).annotations.push(structuredClone(operation.annotation));
        break;
      case 'resolve_annotation': {
        const annotation = requiredComponent(next, operation.componentId).annotations.find((item) => item.id === operation.annotationId);
        if (!annotation) throw new Error(`Annotation not found: ${operation.annotationId}`);
        annotation.status = 'resolved';
        annotation.resolvedAt = new Date().toISOString();
        break;
      }
      case 'add_request':
        next.requests.push(structuredClone(operation.request));
        break;
      case 'resolve_request': {
        const request = next.requests.find((item) => item.id === operation.requestId);
        if (!request) throw new Error(`Design request not found: ${operation.requestId}`);
        request.status = 'resolved';
        request.resolution = operation.resolution?.trim() || undefined;
        request.resolvedAt = new Date().toISOString();
        break;
      }
      default:
        throw new Error(`Unsupported patch operation: ${(operation as { op?: string }).op ?? 'unknown'}`);
    }
  }
  assertWebDesignDocument(next);
  return next;
}
