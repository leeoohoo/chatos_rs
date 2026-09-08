import type { WebComponentType, WebDesignJsonValue, WebDesignLibraryName } from './schema.js';
import { defineUiComponent, type UiComponentDefinition } from './ui-library.js';

export type CreativeFamily =
  | 'card' | 'device' | 'background' | 'text' | 'progress' | 'lens' | 'pointer' | 'effect'
  | 'media' | 'comparison' | 'copy' | 'marquee' | 'matrix' | 'globe' | 'button' | 'social'
  | 'bento' | 'number' | 'list' | 'beam' | 'orbit' | 'dock' | 'avatars' | 'iconcloud'
  | 'reveal' | 'confetti' | 'tree' | 'terminal' | 'image' | 'timeline' | 'theme'
  | 'chart' | 'book' | 'badge' | 'color' | 'kbd' | 'input' | 'spinner' | 'checkbox' | 'qr'
  | 'upload' | 'tabs' | 'modal' | 'gallery' | 'tooltip' | 'loader' | 'calendar' | 'testimonial';

export interface CreativeComponentDescriptor<TCategory extends string = string> {
  slug: string;
  label: string;
  category: TCategory;
  family: CreativeFamily;
  icon: string;
  width?: number;
  height?: number;
  content?: string;
  props?: Record<string, WebDesignJsonValue>;
}

const BASE_TYPE_BY_FAMILY: Record<CreativeFamily, WebComponentType> = {
  card: 'card', device: 'card', background: 'section', text: 'heading', progress: 'card', lens: 'image', pointer: 'section', effect: 'section',
  media: 'video', comparison: 'card', copy: 'card', marquee: 'section', matrix: 'section', globe: 'card', button: 'button', social: 'card',
  bento: 'section', number: 'heading', list: 'list', beam: 'section', orbit: 'section', dock: 'card', avatars: 'avatar', iconcloud: 'section',
  reveal: 'card', confetti: 'section', tree: 'list', terminal: 'card', image: 'image', timeline: 'list', theme: 'switch', chart: 'card', book: 'card',
  badge: 'badge', color: 'card', kbd: 'card', input: 'input', spinner: 'card', checkbox: 'checkbox', qr: 'image',
  upload: 'input', tabs: 'section', modal: 'card', gallery: 'section', tooltip: 'card', loader: 'card', calendar: 'card', testimonial: 'card'
};

const SIZE_BY_FAMILY: Record<CreativeFamily, [number, number]> = {
  card: [380, 230], device: [420, 300], background: [520, 260], text: [440, 150], progress: [280, 210], lens: [380, 250], pointer: [420, 240], effect: [460, 250],
  media: [480, 280], comparison: [500, 280], copy: [430, 160], marquee: [520, 150], matrix: [430, 250], globe: [430, 360], button: [230, 70], social: [400, 230],
  bento: [520, 330], number: [300, 120], list: [400, 280], beam: [500, 280], orbit: [400, 340], dock: [420, 110], avatars: [300, 90], iconcloud: [390, 330],
  reveal: [420, 240], confetti: [440, 250], tree: [380, 290], terminal: [480, 300], image: [420, 280], timeline: [520, 280], theme: [190, 70], chart: [560, 310], book: [420, 300],
  badge: [220, 72], color: [380, 220], kbd: [360, 120], input: [380, 100], spinner: [260, 130], checkbox: [280, 80], qr: [260, 280],
  upload: [460, 300], tabs: [500, 280], modal: [480, 320], gallery: [520, 330], tooltip: [360, 180], loader: [420, 250], calendar: [420, 320], testimonial: [480, 270]
};

export function creativeComponentId(slug: string): string {
  return slug.split('-').map((part) => {
    if (!part) return '';
    if (/^3d$/i.test(part)) return 'ThreeD';
    if (/^[0-9]/.test(part)) return `N${part}`;
    return `${part[0].toUpperCase()}${part.slice(1)}`;
  }).join('');
}

export function createCreativeDefinitions<TCategory extends string>(
  descriptors: readonly CreativeComponentDescriptor<TCategory>[],
  docsBaseUrl: string
): UiComponentDefinition<TCategory>[] {
  return descriptors.map((descriptor) => {
    const id = creativeComponentId(descriptor.slug);
    const [defaultWidth, defaultHeight] = SIZE_BY_FAMILY[descriptor.family];
    return {
      ...defineUiComponent(
        id,
        descriptor.label,
        descriptor.category,
        descriptor.icon,
        BASE_TYPE_BY_FAMILY[descriptor.family],
        descriptor.width ?? defaultWidth,
        descriptor.height ?? defaultHeight,
        descriptor.content ?? descriptor.label,
        {
          family: descriptor.family,
          componentSlug: descriptor.slug,
          title: descriptor.label,
          description: `${descriptor.label} 的可编辑演示`,
          accent: '#7C3AED',
          items: ['设计', '动效', '交互'],
          values: [32, 48, 41, 68, 57, 82],
          ...descriptor.props
        },
        [descriptor.slug, descriptor.family, '动画', '创意', '营销页']
      ),
      docsUrl: `${docsBaseUrl}${descriptor.slug}`
    };
  });
}

export function assertCreativeCatalog(libraryId: WebDesignLibraryName, descriptors: readonly CreativeComponentDescriptor[]): void {
  const ids = descriptors.map((descriptor) => creativeComponentId(descriptor.slug));
  if (new Set(ids).size !== ids.length) throw new Error(`${libraryId} contains duplicate component IDs.`);
}
