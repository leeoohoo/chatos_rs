import type { WebComponentType } from '../../src/schema';

export interface InspectorCapabilities {
  content: boolean;
  library: boolean;
  interaction: boolean;
  visualStates: boolean;
  typography: boolean;
  media: boolean;
  layout: boolean;
}

const TEXT_CONTENT_TYPES = new Set<WebComponentType>([
  'text', 'heading', 'button', 'link', 'input', 'textarea', 'select', 'checkbox', 'switch', 'badge', 'logo', 'icon'
]);
const INTERACTIVE_TYPES = new Set<WebComponentType>([
  'button', 'link', 'input', 'textarea', 'select', 'checkbox', 'switch', 'card', 'image', 'avatar'
]);
const MEDIA_TYPES = new Set<WebComponentType>(['image', 'video', 'avatar']);

export function inspectorCapabilities(
  type: WebComponentType,
  options: { library: boolean; directChildCount: number; editableSlotCount: number }
): InspectorCapabilities {
  return {
    content: !options.library && (TEXT_CONTENT_TYPES.has(type) || MEDIA_TYPES.has(type)),
    library: options.library,
    interaction: true,
    visualStates: options.library || INTERACTIVE_TYPES.has(type),
    typography: options.library || TEXT_CONTENT_TYPES.has(type),
    media: MEDIA_TYPES.has(type),
    layout: options.editableSlotCount === 0 && (type === 'section' || options.directChildCount > 0)
  };
}
