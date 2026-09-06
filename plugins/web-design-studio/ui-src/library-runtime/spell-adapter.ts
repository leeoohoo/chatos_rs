import { createReactRegistryAdapter, createRegistryPreviewItems } from './react-registry-adapter';
import { SPELL_REGISTRY_BY_SLUG, SPELL_REGISTRY_MODULES } from './spell-registry.generated';

const requiredPreviewProps: Record<string, Record<string, unknown>> = {
  chart: { data: [18, 31, 27, 48, 56, 72, 68, 91], labels: ['1月', '2月', '3月', '4月', '5月', '6月', '7月', '8月'], name: '访问量', reveal: true },
  'color-selector': { colors: ['default', 'blue', 'purple', 'pink', 'orange'], defaultValue: 'purple', name: 'brand-color' },
  kbd: { keys: ['command', 'k'], active: true },
  'qr-code': { value: 'https://spell.sh', size: 180 },
  'fallback-avatar': { name: 'AI Designer', size: 120 },
  tweet: { id: '1635616584598659073' },
  'spotify-card': { url: 'https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT' },
  'animated-checkbox': { title: '启用 AI 设计建议', defaultChecked: true },
  'label-input': { label: 'Email', placeholder: 'designer@example.com' },
  'logos-carousel': { children: createRegistryPreviewItems(['Figma', 'React', 'Vue', 'Tailwind', 'Motion', 'Nuxt'], 'runtime-preview-logo'), count: 3 },
  'text-marquee': { children: createRegistryPreviewItems(['DESIGN', 'BUILD', 'SHIP', 'ITERATE'], 'runtime-preview-word'), speed: 1.1, prefix: 'WE ' },
  signature: { text: 'Human + AI', fontSize: 22 },
  'exploding-input': { count: 18 },
  'animated-gradient': { config: { preset: 'Prism' }, radius: '18px' },
  'light-rays': { intensity: 0.8, rays: 18, reach: 1.1 },
  marquee: { children: createRegistryPreviewItems(['AI 共同设计', '响应式画布', '精准布局', '实时交互'], 'runtime-preview-chip'), duration: 18, pauseOnHover: true }
};

export const spellRuntimeAdapter = createReactRegistryAdapter({
  library: 'spell',
  entries: SPELL_REGISTRY_BY_SLUG,
  modules: SPELL_REGISTRY_MODULES,
  previewProps(slug, props, content) {
    const translated = slug === 'chart' && Array.isArray(props.values) ? { ...props, data: props.values } : props;
    return { ...requiredPreviewProps[slug], ...translated, children: translated.children ?? requiredPreviewProps[slug]?.children ?? content };
  }
});
