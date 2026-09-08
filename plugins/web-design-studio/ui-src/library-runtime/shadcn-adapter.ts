import { createReactRegistryAdapter } from './react-registry-adapter';
import { SHADCN_REGISTRY_BY_SLUG, SHADCN_REGISTRY_MODULES } from './shadcn-registry.generated';

const editorMetadata = new Set([
  'accent', 'componentSlug', 'description', 'family', 'items', 'mode', 'sourceComponent', 'title', 'values'
]);

export const shadcnRuntimeAdapter = createReactRegistryAdapter({
  library: 'shadcn',
  entries: SHADCN_REGISTRY_BY_SLUG,
  modules: SHADCN_REGISTRY_MODULES,
  previewProps(slug, props, content) {
    const entry = SHADCN_REGISTRY_BY_SLUG[slug];
    if (entry?.demos?.length) return { registryDemo: props.registryDemo };
    const runtimeProps = Object.fromEntries(Object.entries(props).filter(([name]) => !editorMetadata.has(name)));
    return { ...runtimeProps, children: content || slug.replaceAll('-', ' ') };
  }
});
