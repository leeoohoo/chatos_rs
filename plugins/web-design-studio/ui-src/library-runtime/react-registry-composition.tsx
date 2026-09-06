import { createElement, type ComponentType, type ReactNode } from 'react';
import type { ReactModule, ReactRegistryCompositionNode, ReactRegistryCompositionValue } from './types';

type ModuleLoaders = Record<string, () => Promise<unknown>>;

interface CompositionContext {
  content: string;
  props: Record<string, unknown>;
  modules: ModuleLoaders;
}

function isReference(value: ReactRegistryCompositionValue): value is Extract<ReactRegistryCompositionValue, { $ref: string }> {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value) && '$ref' in value);
}

function resolveValue(value: ReactRegistryCompositionValue, context: CompositionContext): unknown {
  if (isReference(value)) {
    if (value.$ref === 'content') return context.content || value.fallback || '';
    const resolved = context.props[value.name];
    return resolved === undefined ? resolveValue(value.fallback ?? null, context) : resolved;
  }
  if (Array.isArray(value)) return value.map((item) => resolveValue(item, context));
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, resolveValue(item, context)]));
  }
  return value;
}

function exportedComponent(module: ReactModule, name: string): ComponentType<Record<string, unknown>> {
  const candidate = name === 'default' ? module.default : module[name];
  if (typeof candidate === 'function' || (candidate && typeof candidate === 'object')) return candidate as ComponentType<Record<string, unknown>>;
  throw new Error(`The official registry module does not export ${name}.`);
}

function isCompositionNode(value: unknown): value is ReactRegistryCompositionNode {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value)
    && (typeof (value as ReactRegistryCompositionNode).element === 'string' || typeof (value as ReactRegistryCompositionNode).module === 'string'));
}

async function renderNode(node: ReactRegistryCompositionNode | ReactRegistryCompositionValue, context: CompositionContext, key: string): Promise<ReactNode> {
  if (!isCompositionNode(node)) {
    return resolveValue(node as ReactRegistryCompositionValue, context) as ReactNode;
  }
  const props = Object.fromEntries(Object.entries(node.props ?? {}).map(([name, value]) => [name, resolveValue(value, context)]));
  const children = await Promise.all((node.children ?? []).map((child, index) => renderNode(child, context, `${key}-${index}`)));
  if (node.element) return createElement(node.element, { ...props, key }, ...children);
  if (!node.module || !node.export) throw new Error('Registry composition nodes require an element or a module export.');
  const loader = context.modules[node.module];
  if (!loader) throw new Error(`Registry composition module is missing: ${node.module}`);
  const Component = exportedComponent(await loader() as ReactModule, node.export);
  return createElement(Component, { ...props, key }, ...children);
}

export function renderReactRegistryComposition(
  composition: ReactRegistryCompositionNode,
  context: CompositionContext
) {
  return renderNode(composition, context, 'composition-root');
}
