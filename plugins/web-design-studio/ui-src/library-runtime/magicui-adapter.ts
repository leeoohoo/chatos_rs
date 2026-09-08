import { createReactRegistryAdapter } from './react-registry-adapter';
import { MAGICUI_REGISTRY_BY_SLUG, MAGICUI_REGISTRY_MODULES } from './magicui-registry.generated';
import { createElement } from 'react';

export const magicUiRuntimeAdapter = createReactRegistryAdapter({
  library: 'magicui',
  entries: MAGICUI_REGISTRY_BY_SLUG,
  modules: MAGICUI_REGISTRY_MODULES,
  previewProps(slug, props) {
    if (slug !== 'animated-subscribe-button') return props;
    return {
      ...props,
      children: [createElement('span', { key: 'subscribe' }, '订阅更新'), createElement('span', { key: 'subscribed' }, '已订阅 ✓')]
    };
  }
});
