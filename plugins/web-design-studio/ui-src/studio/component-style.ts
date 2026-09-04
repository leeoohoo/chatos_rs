import type { CSSProperties } from 'react';
import type { WebComponentStyle } from '../../src/schema';

function cssPropertyToReact(property: string): string {
  if (property.startsWith('--') || !property.includes('-')) return property;
  return property.replace(/-([a-z])/g, (_, character: string) => character.toUpperCase());
}

export function customCssToReactStyle(customCss: WebComponentStyle['customCss']): CSSProperties {
  if (!customCss) return {};
  return Object.fromEntries(Object.entries(customCss).map(([property, value]) => [cssPropertyToReact(property), value])) as CSSProperties;
}

export function mergeComponentStyles(base: WebComponentStyle, override?: WebComponentStyle): WebComponentStyle {
  if (!override) return base;
  return {
    ...base,
    ...override,
    customCss: base.customCss || override.customCss
      ? { ...base.customCss, ...override.customCss }
      : undefined
  };
}

export function componentStyleToCss(style: WebComponentStyle): CSSProperties {
  const transforms = [
    style.rotate === undefined ? undefined : `rotate(${style.rotate}deg)`,
    style.scale === undefined ? undefined : `scale(${style.scale})`
  ].filter(Boolean).join(' ');
  return {
    background: style.background,
    color: style.color,
    borderColor: style.borderColor,
    borderWidth: style.borderWidth,
    borderStyle: style.borderStyle ?? (style.borderWidth ? 'solid' : undefined),
    borderRadius: style.borderRadius,
    padding: style.padding,
    fontSize: style.fontSize,
    fontWeight: style.fontWeight,
    textAlign: style.textAlign,
    lineHeight: style.lineHeight,
    letterSpacing: style.letterSpacing,
    textTransform: style.textTransform,
    textDecoration: style.textDecoration,
    opacity: style.opacity,
    boxShadow: style.shadow,
    filter: style.blur ? `blur(${style.blur}px)` : undefined,
    backdropFilter: style.backdropBlur ? `blur(${style.backdropBlur}px)` : undefined,
    WebkitBackdropFilter: style.backdropBlur ? `blur(${style.backdropBlur}px)` : undefined,
    transform: transforms || undefined,
    overflow: style.overflow,
    objectFit: style.objectFit,
    objectPosition: style.objectPosition,
    mixBlendMode: style.mixBlendMode,
    ...customCssToReactStyle(style.customCss)
  };
}

export function componentEffectStyleToCss(style: WebComponentStyle): CSSProperties {
  const complete = componentStyleToCss(style);
  return {
    opacity: complete.opacity,
    filter: complete.filter,
    backdropFilter: complete.backdropFilter,
    WebkitBackdropFilter: complete.WebkitBackdropFilter,
    transform: complete.transform,
    overflow: complete.overflow,
    mixBlendMode: complete.mixBlendMode,
    ...customCssToReactStyle(style.customCss)
  };
}

export function designStyleScopeProps(style: WebComponentStyle): { className: string; style: CSSProperties } {
  const flags = [
    style.background ? 'has-design-fill' : '',
    style.color ? 'has-design-color' : '',
    style.borderWidth !== undefined || style.borderColor || style.borderStyle ? 'has-design-stroke' : '',
    style.borderRadius !== undefined ? 'has-design-radius' : '',
    style.shadow ? 'has-design-shadow' : '',
    style.padding !== undefined ? 'has-design-padding' : '',
    style.fontSize !== undefined || style.fontWeight !== undefined || style.lineHeight !== undefined || style.letterSpacing !== undefined || style.textAlign || style.textTransform || style.textDecoration ? 'has-design-type' : ''
  ].filter(Boolean).join(' ');
  return {
    className: `design-style-scope ${flags}`,
    style: {
      width: '100%',
      height: '100%',
      ...customCssToReactStyle(style.customCss),
      '--design-fill': style.background,
      '--design-color': style.color,
      '--design-border-color': style.borderColor,
      '--design-border-width': style.borderWidth === undefined ? undefined : `${style.borderWidth}px`,
      '--design-border-style': style.borderStyle ?? (style.borderWidth ? 'solid' : undefined),
      '--design-radius': style.borderRadius === undefined ? undefined : `${style.borderRadius}px`,
      '--design-shadow': style.shadow,
      '--design-padding': style.padding === undefined ? undefined : `${style.padding}px`,
      '--design-font-size': style.fontSize === undefined ? undefined : `${style.fontSize}px`,
      '--design-font-weight': style.fontWeight,
      '--design-line-height': style.lineHeight,
      '--design-letter-spacing': style.letterSpacing === undefined ? undefined : `${style.letterSpacing}px`,
      '--design-text-align': style.textAlign,
      '--design-text-transform': style.textTransform,
      '--design-text-decoration': style.textDecoration
    } as CSSProperties
  };
}
