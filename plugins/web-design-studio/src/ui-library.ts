import { componentDefaults } from './templates.js';
import type { WebComponentType, WebDesignComponent, WebDesignJsonValue, WebDesignLibraryName } from './schema.js';

export interface UiComponentDefinition<TCategory extends string = string> {
  id: string;
  label: string;
  category: TCategory;
  icon: string;
  keywords: string[];
  baseType: WebComponentType;
  content: string;
  width: number;
  height: number;
  props?: Record<string, WebDesignJsonValue>;
}

export interface UiComponentVariant {
  id: string;
  label: string;
  props: Record<string, WebDesignJsonValue>;
  content?: string;
  width?: number;
  height?: number;
}

export interface UiEditableSlot {
  id: string;
  label: string;
  description: string;
  width: number;
  height: number;
}

export interface UiLibraryCatalog<TCategory extends string = string> {
  id: WebDesignLibraryName;
  displayName: string;
  shortName: string;
  version: string;
  brandMark: string;
  categories: readonly TCategory[];
  components: readonly UiComponentDefinition<TCategory>[];
  variants: Record<string, UiComponentVariant[]>;
}

export const DEFAULT_UI_COMPONENT_VARIANT: UiComponentVariant = { id: 'default', label: '默认款式', props: {} };

export function defineUiComponent<TCategory extends string>(
  id: string,
  label: string,
  category: TCategory,
  icon: string,
  baseType: WebComponentType,
  width: number,
  height: number,
  content: string,
  props: Record<string, WebDesignJsonValue> = {},
  keywords: string[] = []
): UiComponentDefinition<TCategory> {
  return { id, label, category, icon, baseType, width, height, content, props, keywords: [id, label, ...keywords] };
}

export function variantsForUiComponent(catalog: UiLibraryCatalog, componentId: string): UiComponentVariant[] {
  return catalog.variants[componentId] ?? [DEFAULT_UI_COMPONENT_VARIANT];
}

export function createUiLibraryComponent(catalog: UiLibraryCatalog, definitionId: string, x: number, y: number): WebDesignComponent {
  const definition = catalog.components.find((candidate) => candidate.id === definitionId);
  if (!definition) throw new Error(`${catalog.displayName} component not found: ${definitionId}`);
  const component = componentDefaults(definition.baseType, x, y);
  component.id = `${catalog.id}-${definition.id.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${globalThis.crypto.randomUUID().slice(0, 8)}`;
  component.name = `${catalog.displayName} · ${definition.id} ${definition.label}`;
  component.width = definition.width;
  component.height = definition.height;
  component.content = definition.content;
  component.style = { background: 'transparent', color: '#1D1D1F', borderWidth: 0, borderRadius: 0, fontSize: 14, fontWeight: 400 };
  component.library = {
    name: catalog.id,
    version: catalog.version,
    component: definition.id,
    props: structuredClone(definition.props ?? {})
  };
  return applyUiComponentVariant(catalog, component, variantsForUiComponent(catalog, definition.id)[0].id);
}

export function applyUiComponentVariant(catalog: UiLibraryCatalog, component: WebDesignComponent, variantId: string): WebDesignComponent {
  if (component.library?.name !== catalog.id) return component;
  const definition = catalog.components.find((candidate) => candidate.id === component.library?.component);
  const variants = variantsForUiComponent(catalog, component.library.component);
  const variant = variants.find((candidate) => candidate.id === variantId) ?? variants[0];
  const variantKeys = new Set(variants.flatMap((candidate) => Object.keys(candidate.props)));
  const customProps = Object.fromEntries(Object.entries(component.library.props).filter(([key]) => !variantKeys.has(key)));
  return {
    ...component,
    content: variant.content ?? component.content,
    width: variant.width ?? component.width,
    height: variant.height ?? component.height,
    library: {
      ...component.library,
      variant: variant.id,
      props: { ...(definition?.props ?? {}), ...customProps, ...variant.props }
    }
  };
}

export function numericLibraryProp(value: WebDesignJsonValue | undefined, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

export function recordLibraryItems(value: WebDesignJsonValue | undefined): Array<Record<string, WebDesignJsonValue>> {
  return Array.isArray(value)
    ? value.filter((item): item is Record<string, WebDesignJsonValue> => Boolean(item) && typeof item === 'object' && !Array.isArray(item))
    : [];
}

export function contentSlot(component: WebDesignComponent, id: string, label: string, description: string, options: { width?: number; height?: number } = {}): UiEditableSlot {
  return {
    id,
    label,
    description,
    width: options.width ?? Math.max(280, component.width - 32),
    height: options.height ?? Math.max(180, component.height - 56)
  };
}

export function namedItemSlots(component: WebDesignComponent, prefix: string, fallbackLabel: string): UiEditableSlot[] {
  const items = recordLibraryItems(component.library?.props.items);
  return items.map((item, index) => {
    const key = typeof item.key === 'string' || typeof item.key === 'number'
      ? String(item.key)
      : typeof item.value === 'string' || typeof item.value === 'number' ? String(item.value) : String(index + 1);
    const label = typeof item.label === 'string' ? item.label : typeof item.title === 'string' ? item.title : `${fallbackLabel} ${index + 1}`;
    return contentSlot(component, `${prefix}-${key.replace(/[^a-zA-Z0-9_-]/g, '-')}`, label, `编辑“${label}”区域中的组件`);
  });
}

export function splitPanelSlots(component: WebDesignComponent, labels: [string, string] = ['面板一', '面板二']): UiEditableSlot[] {
  const height = Math.max(180, component.height - 56);
  return labels.map((label, index) => ({
    id: `panel-${index + 1}`,
    label,
    description: `编辑${label}中的组件`,
    width: Math.max(220, component.width / 2 - 18),
    height
  }));
}
