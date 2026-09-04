import { breakpointFor, componentsForPage, resolveComponent } from './editor-model.js';
import { pagesForDocument, tokensForDocument, type WebComponentStyle, type WebDesignComponent, type WebDesignDevice, type WebDesignDocument } from './schema.js';

export interface ExportedVueFile {
  filename: string;
  content: string;
}

function cssPropertyToKebab(property: string): string {
  if (property.startsWith('--') || property.includes('-')) return property;
  return property.replace(/[A-Z]/g, (character) => `-${character.toLowerCase()}`);
}

function vueVisualStyle(style: WebComponentStyle): Record<string, string | number | undefined> {
  return {
    background: style.background, color: style.color, borderColor: style.borderColor,
    borderWidth: style.borderWidth === undefined ? undefined : `${style.borderWidth}px`,
    borderStyle: style.borderStyle ?? (style.borderWidth ? 'solid' : undefined),
    borderRadius: style.borderRadius === undefined ? undefined : `${style.borderRadius}px`,
    padding: style.padding === undefined ? undefined : `${style.padding}px`,
    fontSize: style.fontSize === undefined ? undefined : `${style.fontSize}px`,
    fontWeight: style.fontWeight, textAlign: style.textAlign, lineHeight: style.lineHeight,
    letterSpacing: style.letterSpacing === undefined ? undefined : `${style.letterSpacing}px`,
    textTransform: style.textTransform, textDecoration: style.textDecoration, opacity: style.opacity,
    boxShadow: style.shadow, filter: style.blur ? `blur(${style.blur}px)` : undefined,
    backdropFilter: style.backdropBlur ? `blur(${style.backdropBlur}px)` : undefined,
    transform: [style.rotate === undefined ? undefined : `rotate(${style.rotate}deg)`, style.scale === undefined ? undefined : `scale(${style.scale})`].filter(Boolean).join(' ') || undefined,
    overflow: style.overflow, objectFit: style.objectFit, objectPosition: style.objectPosition,
    mixBlendMode: style.mixBlendMode,
    ...style.customCss
  };
}

function vueStateCss(component: WebDesignComponent): string {
  const selectors = { hover: ':hover', active: ':active', focus: ':focus-visible' } as const;
  return (Object.keys(selectors) as Array<keyof typeof selectors>).map((state) => {
    const style = component.states?.[state];
    if (!style) return '';
    const declarations = Object.entries(vueVisualStyle(style)).filter(([, value]) => value !== undefined).map(([property, value]) => `${cssPropertyToKebab(property)}:${value}`).join(';');
    return declarations ? `[data-component-id="${component.id}"]${selectors[state]}{${declarations}}` : '';
  }).filter(Boolean).join('\n');
}

export function exportVueComponent(document: WebDesignDocument, device: WebDesignDevice = 'desktop'): ExportedVueFile {
  const viewport = breakpointFor(document, device);
  const tokens = tokensForDocument(document);
  const pages = pagesForDocument(document).map((page) => ({
    ...page,
    components: componentsForPage(document, page.id)
      .sort((left, right) => left.zIndex - right.zIndex)
      .map((component) => {
        const frame = resolveComponent(component, device);
        return {
          id: component.id,
          type: component.type,
          name: component.name,
          content: component.content,
          interaction: component.interaction,
          hidden: frame.hidden,
          style: {
            position: 'absolute', left: `${frame.x}px`, top: `${frame.y}px`, width: `${frame.width}px`, height: `${frame.height}px`,
            zIndex: component.zIndex, display: 'flex', whiteSpace: 'pre-wrap',
            ...vueVisualStyle(frame.style), cursor: component.interaction ? 'pointer' : undefined
          }
        };
      })
  }));
  const stateCss = document.components.map(vueStateCss).filter(Boolean).join('\n');
  const payload = JSON.stringify({ pages, viewport, background: document.viewport.background, tokens }).replace(/</g, '\\u003c');
  const content = `<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';

const design = ${payload};
const route = ref(window.location.pathname || '/');
const page = computed(() => design.pages.find((candidate) => candidate.slug === route.value) ?? design.pages[0]);
const variables = {
  '--color-primary': design.tokens.colors.primary,
  '--color-accent': design.tokens.colors.accent,
  '--color-surface': design.tokens.colors.surface,
  '--color-text': design.tokens.colors.text,
  '--color-muted': design.tokens.colors.muted,
  '--radius-small': design.tokens.radii.small + 'px',
  '--radius-medium': design.tokens.radii.medium + 'px',
  '--radius-large': design.tokens.radii.large + 'px',
  fontFamily: design.tokens.typography.fontFamily
};

function updateRoute() {
  route.value = window.location.pathname || '/';
}

function navigate(slug) {
  window.history.pushState({}, '', slug);
  route.value = slug;
}

function activate(component) {
  if (!component.interaction) return;
  if (component.interaction.type === 'page') {
    const target = design.pages.find((candidate) => candidate.id === component.interaction.target);
    if (target) navigate(target.slug);
  } else {
    window.open(component.interaction.target, '_blank', 'noopener,noreferrer');
  }
}

onMounted(() => window.addEventListener('popstate', updateRoute));
onBeforeUnmount(() => window.removeEventListener('popstate', updateRoute));
</script>

<template>
  <div class="web-design-app" :style="variables">
    <nav v-if="design.pages.length > 1" class="route-nav">
      <button v-for="item in design.pages" :key="item.id" @click="navigate(item.slug)">{{ item.name }}</button>
    </nav>
    <main class="page" :style="{ width: design.viewport.width + 'px', height: design.viewport.height + 'px', background: design.background }">
      <template v-for="component in page.components" :key="component.id">
        <img v-if="!component.hidden && (component.type === 'image' || component.type === 'avatar') && /^(data:image\\/|https?:\\/\\/)/.test(component.content)" :data-component-id="component.id" :class="['component', 'type-' + component.type]" :style="component.style" :src="component.content" :alt="component.name" @click="activate(component)" />
        <video v-else-if="!component.hidden && component.type === 'video' && component.content" :data-component-id="component.id" :class="['component', 'type-' + component.type]" :style="component.style" :src="component.content" controls @click="activate(component)" />
        <input v-else-if="!component.hidden && component.type === 'input'" :data-component-id="component.id" :class="['component', 'type-' + component.type]" :style="component.style" :placeholder="component.content" @click="activate(component)" />
        <textarea v-else-if="!component.hidden && component.type === 'textarea'" :data-component-id="component.id" :class="['component', 'type-' + component.type]" :style="component.style" :placeholder="component.content" @click="activate(component)" />
        <select v-else-if="!component.hidden && component.type === 'select'" :data-component-id="component.id" :class="['component', 'type-' + component.type]" :style="component.style" @click="activate(component)"><option v-for="option in component.content.split('\\n').filter(Boolean)" :key="option">{{ option }}</option></select>
        <label v-else-if="!component.hidden && (component.type === 'checkbox' || component.type === 'switch')" :data-component-id="component.id" :class="['component', 'type-' + component.type]" :style="component.style" @click="activate(component)"><input type="checkbox" :role="component.type === 'switch' ? 'switch' : undefined" checked />{{ component.content }}</label>
        <button v-else-if="!component.hidden && component.type === 'button'" :data-component-id="component.id" :class="['component', 'type-' + component.type]" :style="component.style" @click="activate(component)">{{ component.content }}</button>
        <ul v-else-if="!component.hidden && component.type === 'list'" :data-component-id="component.id" :class="['component', 'type-' + component.type]" :style="component.style" @click="activate(component)"><li v-for="item in component.content.split('\\n').filter(Boolean)" :key="item">{{ item }}</li></ul>
        <table v-else-if="!component.hidden && component.type === 'table'" :data-component-id="component.id" :class="['component', 'type-' + component.type]" :style="component.style" @click="activate(component)"><tbody><tr v-for="(row, rowIndex) in component.content.split('\\n').filter(Boolean)" :key="rowIndex"><td v-for="(cell, cellIndex) in row.split('|')" :key="cellIndex">{{ cell }}</td></tr></tbody></table>
        <div v-else-if="!component.hidden" :data-component-id="component.id" :class="['component', 'type-' + component.type]" :style="component.style" @click="activate(component)">{{ component.type === 'section' || component.type === 'divider' ? '' : component.content || (component.type === 'video' ? '▶ 视频' : component.type === 'image' ? '图片' : '') }}</div>
      </template>
    </main>
  </div>
</template>

<style scoped>
:global(*) { box-sizing: border-box; }
:global(body) { margin: 0; }
.web-design-app { min-height: 100vh; background: #e5e7eb; }
.page { position: relative; margin: 0 auto; overflow: hidden; }
.type-text, .type-heading, .type-card, .type-link, .type-logo { align-items: center; padding: 12px 16px; line-height: 1.22; }
.type-button { align-items: center; justify-content: center; padding: 0 16px; }
.type-input, .type-textarea, .type-select { padding: 0 14px; }
.type-textarea { padding-top: 13px; resize: none; }
.type-image, .type-avatar, .type-video { object-fit: cover; }
.type-icon, .type-badge, .type-avatar { align-items: center; justify-content: center; }
.type-checkbox, .type-switch { align-items: center; gap: 8px; }
.type-divider { height: 1px !important; border-width: 0 !important; border-top: 1px solid; }
.type-list { margin: 0; padding: 13px 20px 13px 38px; flex-direction: column; gap: 8px; }
.type-table { border-collapse: collapse; display: table; }
.type-table td { padding: 10px 12px; border-bottom: 1px solid rgba(128,128,145,.18); }
.route-nav { display: flex; justify-content: center; gap: 8px; padding: 12px; }
${stateCss}
</style>
`;
  return { filename: 'WebDesignApp.vue', content };
}
