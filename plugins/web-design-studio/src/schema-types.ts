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
export type WebDesignSurfaceKind = 'page' | 'modal' | 'drawer' | 'popover' | 'menu' | 'state';

export interface WebDesignPage {
  id: string;
  name: string;
  slug: string;
  surfaceKind?: WebDesignSurfaceKind;
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
  scopeKey?: string;
  isScopeDefault?: boolean;
  name: string;
  description?: string;
  createdAt: string;
  updatedAt: string;
  designIds: string[];
}

export interface WebDesignProjectSummary {
  projectId: string;
  scopeKey?: string;
  isScopeDefault?: boolean;
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
