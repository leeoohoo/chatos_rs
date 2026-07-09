// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export function parsePort(rawValue: string | undefined, fallback: number): number {
  const parsed = Number.parseInt((rawValue || '').trim(), 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

export function normalizeBasePath(rawValue: string | undefined): string {
  const value = (rawValue || '').trim();
  if (!value || value === '/') {
    return '/';
  }
  const withLeadingSlash = value.startsWith('/') ? value : `/${value}`;
  return withLeadingSlash.endsWith('/') ? withLeadingSlash : `${withLeadingSlash}/`;
}

export function basePrefixFromBase(base: string): string {
  return base === '/' ? '' : base.replace(/\/+$/, '');
}

export function createBasePathProxy(
  basePrefix: string,
  pathPrefix: string,
  target: string,
): Record<string, { target: string; changeOrigin: true; rewrite: (path: string) => string }> {
  if (!basePrefix) {
    return {};
  }
  return {
    [`${basePrefix}${pathPrefix}`]: {
      target,
      changeOrigin: true,
      rewrite: (path) => path.slice(basePrefix.length),
    },
  };
}
