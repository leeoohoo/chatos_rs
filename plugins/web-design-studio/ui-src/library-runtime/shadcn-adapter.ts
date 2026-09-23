import { createReactRegistryAdapter } from './react-registry-adapter';
import { SHADCN_REGISTRY_BY_SLUG, SHADCN_REGISTRY_MODULES } from './shadcn-registry.generated';

const editorMetadata = new Set([
  'accent', 'componentSlug', 'description', 'family', 'items', 'mode', 'sourceComponent', 'title', 'values'
]);
const rootContentComponents = new Set(['button']);

export const shadcnRuntimeAdapter = createReactRegistryAdapter({
  library: 'shadcn',
  entries: SHADCN_REGISTRY_BY_SLUG,
  modules: SHADCN_REGISTRY_MODULES,
  renderRootWithContent(slug, _props, content) {
    return rootContentComponents.has(slug) && content.trim().length > 0;
  },
  previewProps(slug, props, content, context) {
    const entry = SHADCN_REGISTRY_BY_SLUG[slug];
    const runtimeProps = Object.fromEntries(Object.entries(props).filter(([name]) => !editorMetadata.has(name)));
    if (context.mode === 'preview' && entry?.demos?.length) return { registryDemo: props.registryDemo };
    return { ...runtimeProps, children: content || slug.replaceAll('-', ' ') };
  }
});
