import { Component, createElement, type ComponentType, type ErrorInfo, type ReactNode } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { ThemeProvider } from 'next-themes';
import { renderReactRegistryComposition } from './react-registry-composition';
import type { LibraryRuntimeAdapter, MountedLibraryComponent, ReactModule, ReactRegistryEntry, RuntimeMountOptions } from './types';

interface ReactRegistryAdapterOptions {
  library: string;
  entries: Record<string, ReactRegistryEntry>;
  modules: Record<string, () => Promise<unknown>>;
  previewProps?: (slug: string, props: Record<string, unknown>, content: string) => Record<string, unknown>;
  wrap?: (node: ReactNode) => ReactNode;
}

export function createRegistryPreviewItems(items: readonly ReactNode[], className = 'runtime-preview-chip'): ReactNode[] {
  return items.map((item, index) => createElement('span', { className, key: `${String(item)}-${index}` }, item));
}

class RuntimeErrorBoundary extends Component<{ emit: RuntimeMountOptions['emit']; children?: ReactNode }, { error?: string }> {
  state: { error?: string } = {};
  static getDerivedStateFromError(error: unknown) { return { error: error instanceof Error ? error.message : String(error) }; }
  componentDidCatch(error: Error, info: ErrorInfo) { this.props.emit('error', { message: error.message, stack: info.componentStack }); }
  render() {
    if (this.state.error) return createElement('div', { className: 'runtime-error' }, createElement('strong', null, '组件运行失败'), createElement('span', null, this.state.error));
    return this.props.children;
  }
}

function exportedComponent(module: ReactModule, name?: string): ComponentType<Record<string, unknown>> {
  const candidate = name ? module[name] : module.default;
  if (typeof candidate === 'function' || (candidate && typeof candidate === 'object')) return candidate as ComponentType<Record<string, unknown>>;
  throw new Error(`The official registry module does not export ${name ?? 'a default component'}.`);
}

export function createReactRegistryAdapter(options: ReactRegistryAdapterOptions): LibraryRuntimeAdapter {
  return {
    library: options.library,
    async mount({ slug, props, content, target, emit }: RuntimeMountOptions): Promise<MountedLibraryComponent> {
      const entry = options.entries[slug];
      if (!entry) throw new Error(`${options.library} component is not synced: ${slug}`);
      const root: Root = createRoot(target);
      let generation = 0;
      const render = async (nextProps: Record<string, unknown>, nextContent: string) => {
        const requestedDemo = typeof nextProps.registryDemo === 'string' ? nextProps.registryDemo : undefined;
        const demo = entry.demos?.find((candidate) => candidate.id === requestedDemo);
        const currentGeneration = ++generation;
        const composedPreview = demo?.composition
          ? await renderReactRegistryComposition(demo.composition, { content: nextContent, props: nextProps, modules: options.modules })
          : undefined;
        const previewPath = demo?.path ?? entry.previewPath;
        const previewExport = demo?.export ?? entry.previewExport;
        const loader = composedPreview ? undefined : options.modules[previewPath];
        if (!composedPreview && !loader) throw new Error(`${options.library} preview module is missing: ${previewPath}`);
        const Preview = composedPreview ? undefined : exportedComponent(await loader!() as ReactModule, previewExport);
        if (currentGeneration !== generation) return;
        const configured = options.previewProps?.(slug, nextProps, nextContent) ?? nextProps;
        const { children: configuredChildren, registryDemo: _registryDemo, ...runtimeProps } = configured;
        const children = (demo || entry.demo || entry.acceptsChildren === false ? undefined : configuredChildren ?? nextContent) as ReactNode;
        const previewNode = composedPreview ?? createElement(Preview!, runtimeProps, children);
        const wrappedPreview = options.wrap ? options.wrap(previewNode) : previewNode;
        root.render(createElement(RuntimeErrorBoundary, { emit },
          createElement(ThemeProvider, { attribute: 'class', forcedTheme: 'light', enableSystem: false },
            createElement('div', { className: `react-registry-preview library-${options.library} component-${slug}` },
              createElement('div', { className: 'react-registry-stage' }, wrappedPreview)))));
      };
      await render(props, content);
      emit('mounted', { slug, officialDemo: entry.demo });
      return {
        update(nextProps, nextContent) {
          void render(nextProps, nextContent).catch((error) => emit('error', error instanceof Error ? error.message : String(error)));
        },
        destroy() { generation += 1; root.unmount(); }
      };
    }
  };
}
