import { breakpointFor, componentsForPage, resolveComponent } from './editor-model.js';
import { pagesForDocument, tokensForDocument, type WebDesignComponent, type WebDesignDevice, type WebDesignDocument } from './schema.js';

export interface ExportedHtmlFile {
  pageId: string;
  filename: string;
  html: string;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  })[character]!);
}

function cssValue(value: string | number | undefined, suffix = ''): string | undefined {
  return value === undefined || value === '' ? undefined : `${value}${typeof value === 'number' ? suffix : ''}`;
}

function componentMarkup(component: WebDesignComponent, document: WebDesignDocument, device: WebDesignDevice): string {
  const frame = resolveComponent(component, device);
  if (frame.hidden) return '';
  const declarations = [
    ['left', cssValue(frame.x, 'px')], ['top', cssValue(frame.y, 'px')],
    ['width', cssValue(frame.width, 'px')], ['height', cssValue(frame.height, 'px')],
    ['z-index', cssValue(component.zIndex)], ['background', cssValue(frame.style.background)],
    ['color', cssValue(frame.style.color)], ['border-color', cssValue(frame.style.borderColor)],
    ['border-width', cssValue(frame.style.borderWidth, 'px')], ['border-style', frame.style.borderWidth ? 'solid' : undefined],
    ['border-radius', cssValue(frame.style.borderRadius, 'px')], ['font-size', cssValue(frame.style.fontSize, 'px')],
    ['font-weight', cssValue(frame.style.fontWeight)], ['text-align', cssValue(frame.style.textAlign)],
    ['opacity', cssValue(frame.style.opacity)], ['box-shadow', cssValue(frame.style.shadow)]
  ].filter((item): item is [string, string] => item[1] !== undefined);
  const style = declarations.map(([name, value]) => `${name}:${value}`).join(';');
  const libraryAttributes = component.library ? ` data-library="${escapeHtml(component.library.name)}" data-library-component="${escapeHtml(component.library.component)}"` : '';
  const attributes = `class="component type-${component.type}${component.library ? ' library-component' : ''}" data-component-id="${escapeHtml(component.id)}"${libraryAttributes} style="${escapeHtml(style)}"`;
  const content = escapeHtml(component.content).replace(/\n/g, '<br>');
  let markup: string;
  if (component.type === 'image' || component.type === 'avatar') {
    markup = component.content && /^(data:image\/|https?:\/\/)/.test(component.content)
      ? `<img ${attributes} src="${escapeHtml(component.content)}" alt="${escapeHtml(component.name)}">`
      : `<div ${attributes}><span class="image-placeholder">${component.type === 'avatar' ? content || 'AI' : '图片'}</span></div>`;
  } else if (component.type === 'video') {
    markup = component.content
      ? `<video ${attributes} src="${escapeHtml(component.content)}" controls></video>`
      : `<div ${attributes}><span class="media-placeholder">▶<small>视频</small></span></div>`;
  } else if (component.type === 'input') {
    markup = `<input ${attributes} placeholder="${escapeHtml(component.content)}">`;
  } else if (component.type === 'textarea') {
    markup = `<textarea ${attributes} placeholder="${escapeHtml(component.content)}"></textarea>`;
  } else if (component.type === 'select') {
    markup = `<select ${attributes}>${component.content.split('\n').filter(Boolean).map((option) => `<option>${escapeHtml(option)}</option>`).join('')}</select>`;
  } else if (component.type === 'checkbox') {
    markup = `<label ${attributes}><input type="checkbox" checked> ${content}</label>`;
  } else if (component.type === 'switch') {
    markup = `<label ${attributes}><input type="checkbox" role="switch" checked> ${content}</label>`;
  } else if (component.type === 'button') {
    markup = `<button ${attributes}>${content}</button>`;
  } else if (component.type === 'list') {
    markup = `<ul ${attributes}>${component.content.split('\n').filter(Boolean).map((item) => `<li>${escapeHtml(item)}</li>`).join('')}</ul>`;
  } else if (component.type === 'table') {
    markup = `<table ${attributes}><tbody>${component.content.split('\n').filter(Boolean).map((row) => `<tr>${row.split('|').map((cell) => `<td>${escapeHtml(cell)}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
  } else {
    markup = `<div ${attributes}>${component.type === 'section' || component.type === 'divider' ? '' : content}</div>`;
  }
  if (!component.interaction) return markup;
  const targetPage = component.interaction.type === 'page' ? pagesForDocument(document).find((page) => page.id === component.interaction!.target) : undefined;
  const href = targetPage
    ? targetPage.slug === '/' ? 'index.html' : `${targetPage.slug.replace(/^\/+|\/+$/g, '') || targetPage.id}.html`
    : component.interaction.target;
  return `<a class="interaction-link" href="${escapeHtml(href)}"${component.interaction.type === 'url' ? ' target="_blank" rel="noopener noreferrer"' : ''}>${markup}</a>`;
}

export function exportPageHtml(document: WebDesignDocument, pageId: string, device: WebDesignDevice = 'desktop'): string {
  const page = pagesForDocument(document).find((candidate) => candidate.id === pageId);
  if (!page) throw new Error(`Page not found: ${pageId}`);
  const viewport = breakpointFor(document, device);
  const tokens = tokensForDocument(document);
  const components = componentsForPage(document, pageId)
    .sort((left, right) => left.zIndex - right.zIndex)
    .map((component) => componentMarkup(component, document, device))
    .filter(Boolean)
    .join('\n    ');
  return `<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${escapeHtml(`${document.title} · ${page.name}`)}</title>
  <style>
    :root { --color-primary: ${escapeHtml(tokens.colors.primary)}; --color-accent: ${escapeHtml(tokens.colors.accent)}; --color-surface: ${escapeHtml(tokens.colors.surface)}; --color-text: ${escapeHtml(tokens.colors.text)}; --color-muted: ${escapeHtml(tokens.colors.muted)}; --radius-small: ${tokens.radii.small}px; --radius-medium: ${tokens.radii.medium}px; --radius-large: ${tokens.radii.large}px; }
    * { box-sizing: border-box; }
    html, body { margin: 0; min-height: 100%; font-family: ${escapeHtml(tokens.typography.fontFamily)}; font-size: ${tokens.typography.baseFontSize}px; }
    body { background: #e5e7eb; }
    .page { position: relative; width: ${viewport.width}px; height: ${viewport.height}px; margin: 0 auto; overflow: hidden; background: ${escapeHtml(document.viewport.background)}; }
    .component { position: absolute; display: flex; white-space: pre-wrap; overflow: hidden; }
    .type-text, .type-heading, .type-card, .type-link, .type-logo { align-items: center; padding: 12px 16px; line-height: 1.22; }
    .type-button { align-items: center; justify-content: center; padding: 0 16px; font: inherit; }
    .type-input, .type-textarea, .type-select { align-items: center; padding: 0 14px; font: inherit; }
    .type-textarea { padding-top: 13px; resize: none; }
    .type-image, .type-avatar, .type-video { object-fit: cover; }
    .type-icon, .type-badge, .type-avatar { align-items: center; justify-content: center; }
    .type-checkbox, .type-switch { align-items: center; gap: 8px; }
    .type-divider { height: 1px !important; border-width: 0 !important; border-top: 1px solid; }
    .type-list { margin: 0; padding: 13px 20px 13px 38px; flex-direction: column; gap: 8px; }
    .type-table { border-collapse: collapse; display: table; }
    .type-table td { padding: 10px 12px; border-bottom: 1px solid rgba(128,128,145,.18); }
    .type-table tr:first-child td { background: rgba(0,122,255,.06); font-weight: 750; }
    .media-placeholder { width: 100%; display: grid; place-items: center; align-content: center; gap: 7px; color: white; font-size: 34px; }
    .interaction-link { color: inherit; text-decoration: none; }
    .image-placeholder { width: 100%; display: grid; place-items: center; color: #6b7280; }
  </style>
</head>
<body>
  <main class="page" data-page-id="${escapeHtml(page.id)}">
    ${components}
  </main>
</body>
</html>
`;
}

export function exportDocumentHtmlFiles(document: WebDesignDocument, device: WebDesignDevice = 'desktop'): ExportedHtmlFile[] {
  return pagesForDocument(document).map((page, index) => ({
    pageId: page.id,
    filename: index === 0 || page.slug === '/' ? 'index.html' : `${page.slug.replace(/^\/+|\/+$/g, '') || page.id}.html`,
    html: exportPageHtml(document, page.id, device)
  }));
}
