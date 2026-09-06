import type { DomRegistryEntry, LibraryRuntimeAdapter, MountedLibraryComponent, RuntimeMountOptions } from './types';

interface DomRegistryAdapterOptions {
  library: string;
  entries: Record<string, DomRegistryEntry>;
  loadEntry?: (slug: string) => Promise<DomRegistryEntry>;
  className?: string;
}

function currentDemo(entry: DomRegistryEntry, props: Record<string, unknown>) {
  const requested = typeof props.registryDemo === 'string' ? props.registryDemo : undefined;
  return entry.demos.find((demo) => demo.id === requested) ?? entry.demos[0];
}

function installRuntimeInteractions(target: HTMLElement) {
  const click = (event: MouseEvent) => {
    const source = event.target as HTMLElement | null;
    const dialogForm = source?.closest('form[method="dialog"]') as HTMLFormElement | null;
    const dialog = dialogForm?.closest('dialog') as HTMLDialogElement | null;
    if (dialog && source?.closest('button')) {
      event.preventDefault();
      dialog.close();
      return;
    }
    const anchor = source?.closest('a[href]') as HTMLAnchorElement | null;
    if (!anchor) return;
    const href = anchor.getAttribute('href') ?? '';
    if (!href || href.startsWith('#')) return;
    event.preventDefault();
  };
  target.addEventListener('click', click);
  return () => target.removeEventListener('click', click);
}

export function createDomRegistryAdapter(options: DomRegistryAdapterOptions): LibraryRuntimeAdapter {
  return {
    library: options.library,
    async mount({ slug, props, target, emit }: RuntimeMountOptions): Promise<MountedLibraryComponent> {
      const listedEntry = options.entries[slug];
      if (!listedEntry) throw new Error(`${options.library} component is not synced: ${slug}`);
      const entry = options.loadEntry ? await options.loadEntry(slug) : listedEntry;
      const host = document.createElement('div');
      host.className = `dom-registry-preview library-${options.library} component-${slug} ${options.className ?? ''}`.trim();
      target.replaceChildren(host);
      const removeRuntimeInteractions = installRuntimeInteractions(host);
      let activeDemo = '';
      const render = (nextProps: Record<string, unknown>) => {
        const demo = currentDemo(entry, nextProps);
        if (!demo) throw new Error(`${options.library} ${slug} has no official example.`);
        if (activeDemo === demo.id) return;
        activeDemo = demo.id;
        host.dataset.demo = demo.id;
        if (!demo.html) throw new Error(`${options.library} ${slug} example ${demo.id} has no HTML payload.`);
        host.innerHTML = demo.html;
        emit('rendered', { slug, demo: demo.id });
      };
      render(props);
      emit('mounted', { slug, officialDemo: activeDemo });
      return {
        update(nextProps) { render(nextProps); },
        destroy() {
          removeRuntimeInteractions();
          target.replaceChildren();
        }
      };
    }
  };
}
