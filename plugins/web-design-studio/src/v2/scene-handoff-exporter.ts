import type { SceneDocument } from './scene-schema.js';
import { renderSceneDocumentRoot } from './scene-html-renderer.js';

function pageRootId(document: SceneDocument, pageId: string): string {
  const page = document.pages.find((candidate) => candidate.id === pageId);
  if (!page) throw new Error(`Scene page not found: ${pageId}`);
  const root = page.children.find((node) => node.role === 'page-root') ?? page.children[0];
  if (!root) throw new Error(`Scene page ${pageId} has no root frame to export.`);
  return root.id;
}

function uniqueSceneId(markup: string, suffix: string): string {
  return markup.replace('id="phase2-scene"', `id="phase2-scene-${suffix}"`);
}

export function renderResponsiveSceneHandoff(
  document: SceneDocument,
  pageId: string,
  desktopWidth = 1440,
  mobileWidth = 390
): { html: string; desktopHeight: number; mobileHeight: number } {
  const rootNodeId = pageRootId(document, pageId);
  const desktop = renderSceneDocumentRoot(document, rootNodeId, desktopWidth, `${document.documentId}:desktop`);
  const mobile = renderSceneDocumentRoot(document, rootNodeId, mobileWidth, `${document.documentId}:mobile`);
  const desktopMarkup = uniqueSceneId(desktop.html, 'desktop');
  const mobileMarkup = uniqueSceneId(mobile.html, 'mobile');
  return {
    desktopHeight: desktop.height,
    mobileHeight: mobile.height,
    html: `<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>${document.name}</title>
  <style>
    *{box-sizing:border-box}html,body{margin:0;min-height:100%;overflow-x:hidden;background:#fff}
    .handoff-shell{position:relative;width:100%;overflow:hidden;margin:0 auto}
    .handoff-stage{position:absolute;left:50%;top:0;transform-origin:top center}
    .handoff-mobile{display:none}
    .scene-library-instance iframe{display:block;width:100%;height:100%;border:0;background:transparent;pointer-events:none}
    @media(max-width:600px){.handoff-desktop{display:none}.handoff-mobile{display:block}}
  </style>
</head>
<body data-chatos-responsive-handoff="true">
  <section class="handoff-shell handoff-desktop" data-native-width="${desktop.width}" data-native-height="${desktop.height}">
    <div class="handoff-stage" style="width:${desktop.width}px;height:${desktop.height}px">${desktopMarkup}</div>
  </section>
  <section class="handoff-shell handoff-mobile" data-native-width="${mobile.width}" data-native-height="${mobile.height}">
    <div class="handoff-stage" style="width:${mobile.width}px;height:${mobile.height}px">${mobileMarkup}</div>
  </section>
  <script>
    (() => {
      const fit = () => {
        for (const shell of document.querySelectorAll('.handoff-shell')) {
          if (getComputedStyle(shell).display === 'none') continue;
          const width = Number(shell.dataset.nativeWidth);
          const height = Number(shell.dataset.nativeHeight);
          const scale = Math.min(1, innerWidth / width);
          const stage = shell.querySelector('.handoff-stage');
          stage.style.transform = 'translateX(-50%) scale(' + scale + ')';
          shell.style.height = Math.ceil(height * scale) + 'px';
        }
      };
      addEventListener('resize', fit, { passive: true });
      fit();
    })();
  </script>
</body>
</html>`
  };
}
