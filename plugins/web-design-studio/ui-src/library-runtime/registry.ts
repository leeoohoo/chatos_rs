import { INSPIRA_REGISTRY_BY_SLUG } from './inspira-registry.generated';
import { MAGICUI_REGISTRY_BY_SLUG } from './magicui-registry.generated';
import { SPELL_REGISTRY_BY_SLUG } from './spell-registry.generated';
import { SHADCN_REGISTRY_BY_SLUG } from './shadcn-registry.generated';
import { DAISYUI_REGISTRY_BY_SLUG } from './daisyui-registry.generated';

const registries: Record<string, Record<string, unknown>> = {
  inspira: INSPIRA_REGISTRY_BY_SLUG,
  magicui: MAGICUI_REGISTRY_BY_SLUG,
  shadcn: SHADCN_REGISTRY_BY_SLUG,
  spell: SPELL_REGISTRY_BY_SLUG,
  daisyui: DAISYUI_REGISTRY_BY_SLUG
};

export function hasOfficialRuntimeComponent(library: string | undefined, slug: string) {
  if (library === 'antd' || library === 'chakra') return Boolean(slug);
  return Boolean(library && registries[library]?.[slug]);
}

export function officialRuntimePresentation(library: string | undefined, slug: string) {
  if (library === 'antd' || library === 'chakra') return { layout: 'intrinsic' as const, previewSpan: 'wide' as const };
  const entry = library ? registries[library]?.[slug] as { layout?: 'fill' | 'intrinsic'; previewHeight?: number; previewSpan?: 'normal' | 'wide' } | undefined : undefined;
  return entry ? { layout: entry.layout, previewHeight: entry.previewHeight, previewSpan: entry.previewSpan } : undefined;
}
