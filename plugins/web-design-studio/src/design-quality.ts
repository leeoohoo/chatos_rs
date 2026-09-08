import { breakpointFor, resolveComponent } from './editor-model.js';
import { isUiContentContainer } from './library-slots.js';
import {
  designSummary,
  pageIdForComponent,
  pagesForDocument,
  type WebDesignDocument
} from './schema.js';

export type WebDesignValidationMode = 'draft' | 'handoff';

export interface WebDesignQualityIssue {
  code: string;
  severity: 'blocking' | 'warning';
  message: string;
  pageId?: string;
  componentId?: string;
  evidence?: Record<string, unknown>;
}

function pageComponents(document: WebDesignDocument, pageId: string) {
  return document.components.filter((component) => pageIdForComponent(document, component) === pageId && !component.hidden);
}

function textLineCount(content: string): number {
  return content.length === 0 ? 0 : content.split(/\r?\n/).length;
}

export function analyzeDesignQuality(document: WebDesignDocument, requestedPageId?: string) {
  const pages = pagesForDocument(document);
  if (requestedPageId && !pages.some((page) => page.id === requestedPageId)) throw new Error(`Page not found: ${requestedPageId}`);
  const targetPages = requestedPageId ? pages.filter((page) => page.id === requestedPageId) : pages;
  const blockingIssues: WebDesignQualityIssue[] = [];
  const warnings: WebDesignQualityIssue[] = [];

  for (const page of targetPages) {
    const components = pageComponents(document, page.id);
    const viewport = breakpointFor(document, 'desktop');
    const viewportArea = viewport.width * viewport.height;
    const largeTextMockups = components.filter((component) => {
      if (component.type !== 'text') return false;
      const frame = resolveComponent(component, 'desktop');
      const areaRatio = (frame.width * frame.height) / viewportArea;
      const lines = textLineCount(component.content);
      const semanticWholePageName = /(?:完整|整页|页面|界面|工作台|dashboard|screen|page|mockup)/i.test(component.name);
      return areaRatio >= 0.45 && (component.content.length > 300 || lines > 8 || semanticWholePageName);
    });

    if (components.length === 0) {
      blockingIssues.push({
        code: 'empty_page',
        severity: 'blocking',
        pageId: page.id,
        message: `页面“${page.name}”为空，不能作为可交付设计。`
      });
      continue;
    }

    if (largeTextMockups.length > 0) {
      for (const component of largeTextMockups) {
        const frame = resolveComponent(component, 'desktop');
        blockingIssues.push({
          code: 'page_as_text_mockup',
          severity: 'blocking',
          pageId: page.id,
          componentId: component.id,
          message: '检测到使用单个大文本节点模拟完整界面。导航、卡片、按钮、表格和正文必须拆成独立可编辑节点。',
          evidence: {
            contentLength: component.content.length,
            lineCount: textLineCount(component.content),
            areaRatio: Number(((frame.width * frame.height) / viewportArea).toFixed(3))
          }
        });
      }
    }

    if (components.length <= 2) {
      blockingIssues.push({
        code: 'underbuilt_page',
        severity: 'blocking',
        pageId: page.id,
        message: `页面“${page.name}”只有 ${components.length} 个节点，尚未形成可编辑的界面结构。`,
        evidence: { componentCount: components.length }
      });
    }

    for (const component of components) {
      if (component.type !== 'text') continue;
      const lines = textLineCount(component.content);
      if (component.content.length > 500 || lines > 12) {
        const target = largeTextMockups.some((candidate) => candidate.id === component.id) ? warnings : blockingIssues;
        target.push({
          code: 'long_text_as_ui',
          severity: target === blockingIssues ? 'blocking' : 'warning',
          pageId: page.id,
          componentId: component.id,
          message: '文本节点包含过多界面内容；一个文本节点只应表达一个标题、标签、说明或正文段落。',
          evidence: { contentLength: component.content.length, lineCount: lines }
        });
      }
    }

    const containers = components.filter((component) =>
      component.type === 'section' || component.type === 'card' || isUiContentContainer(component)
    );
    if (components.length >= 4 && containers.length === 0) {
      warnings.push({
        code: 'flat_page_without_containers',
        severity: 'warning',
        pageId: page.id,
        message: '页面缺少有语义的容器层。请像 Figma Frame/Auto Layout 一样按 Header、Sidebar、Section、Card 等逻辑区域组织节点。'
      });
    }

    const libraryCount = components.filter((component) => component.library).length;
    const symbolInstanceCount = components.filter((component) => component.symbolId || component.symbolInstanceId).length;
    if (components.length >= 8 && libraryCount === 0 && symbolInstanceCount === 0) {
      warnings.push({
        code: 'missing_component_system',
        severity: 'warning',
        pageId: page.id,
        message: '页面包含较多节点但没有组件库绑定或可复用实例。重复 UI 应优先使用组件契约或 Symbol/Instance。',
        evidence: { componentCount: components.length }
      });
    }
  }

  return {
    pageIds: targetPages.map((page) => page.id),
    blockingIssues,
    warnings,
    score: Math.max(0, 100 - blockingIssues.length * 25 - warnings.length * 5)
  };
}

export function validateWebDesignDocument(
  document: WebDesignDocument,
  options: { pageId?: string; mode?: WebDesignValidationMode } = {}
) {
  const mode = options.mode ?? 'handoff';
  const selectedPageIds = new Set(
    options.pageId ? [options.pageId] : pagesForDocument(document).map((page) => page.id)
  );
  const selectedComponents = document.components.filter((component) => selectedPageIds.has(pageIdForComponent(document, component)));
  const outOfBounds = (['desktop', 'tablet', 'mobile'] as const).flatMap((device) => {
    const target = breakpointFor(document, device);
    return selectedComponents.flatMap((component) => {
      const frame = resolveComponent(component, device);
      return !frame.hidden && (frame.x < 0 || frame.y < 0 || frame.x + frame.width > target.width || frame.y + frame.height > target.height)
        ? [{ device, componentId: component.id, pageId: pageIdForComponent(document, component) }]
        : [];
    });
  });
  const openAnnotations = selectedComponents.flatMap((component) => component.annotations
    .filter((annotation) => annotation.status === 'open')
    .map((annotation) => ({ componentId: component.id, annotation })));
  const byId = new Map(document.components.map((component) => [component.id, component]));
  const childrenOutsideContainers = (['desktop', 'tablet', 'mobile'] as const).flatMap((device) => selectedComponents.flatMap((component) => {
    if (!component.parentId) return [];
    const parent = byId.get(component.parentId);
    if (!parent) return [];
    if (component.slot && isUiContentContainer(parent)) return [];
    const childFrame = resolveComponent(component, device);
    const parentFrame = resolveComponent(parent, device);
    return !childFrame.hidden && !parentFrame.hidden && (
      childFrame.x < parentFrame.x || childFrame.y < parentFrame.y
      || childFrame.x + childFrame.width > parentFrame.x + parentFrame.width
      || childFrame.y + childFrame.height > parentFrame.y + parentFrame.height
    ) ? [{ device, componentId: component.id, parentId: parent.id }] : [];
  }));
  const emptyLayoutContainers = selectedComponents
    .filter((component) => component.layout?.mode !== undefined && component.layout.mode !== 'free'
      && !document.components.some((candidate) => candidate.parentId === component.id))
    .map((component) => ({ componentId: component.id, mode: component.layout!.mode }));
  const unusualParents = selectedComponents.flatMap((component) => {
    if (!component.parentId) return [];
    const parent = byId.get(component.parentId);
    return parent && !['section', 'card'].includes(parent.type) && !isUiContentContainer(parent)
      ? [{ componentId: component.id, parentId: parent.id, parentType: parent.type }]
      : [];
  });
  const quality = analyzeDesignQuality(document, options.pageId);
  const structuralBlockingIssues = [
    ...(outOfBounds.length ? [{ code: 'out_of_bounds', items: outOfBounds }] : []),
    ...(childrenOutsideContainers.length ? [{ code: 'children_outside_container', items: childrenOutsideContainers }] : [])
  ];
  const warnings = [
    ...(emptyLayoutContainers.length ? [{ code: 'empty_layout_containers', items: emptyLayoutContainers }] : []),
    ...(unusualParents.length ? [{ code: 'unusual_parent_type', items: unusualParents }] : []),
    ...(openAnnotations.length ? [{ code: 'open_annotations', items: openAnnotations }] : []),
    ...(document.requests.some((request) => request.status === 'pending') ? [{ code: 'pending_ai_requests' }] : []),
    ...quality.warnings
  ];
  const blockingIssues = [
    ...structuralBlockingIssues,
    ...(mode === 'handoff' ? quality.blockingIssues : [])
  ];
  return {
    valid: blockingIssues.length === 0,
    mode,
    document: designSummary(document),
    pageId: options.pageId,
    blockingIssues,
    warnings,
    quality
  };
}

export function assertHandoffQuality(document: WebDesignDocument, pageId?: string): void {
  const validation = validateWebDesignDocument(document, { pageId, mode: 'handoff' });
  if (!validation.valid) {
    const codes = validation.blockingIssues.map((issue) => issue.code).join(', ');
    throw new Error(`Design is not ready for export. Fix blocking validation issues first: ${codes}.`);
  }
}

export function pageOutline(document: WebDesignDocument, pageId: string) {
  const page = pagesForDocument(document).find((candidate) => candidate.id === pageId);
  if (!page) throw new Error(`Page not found: ${pageId}`);
  const components = pageComponents(document, pageId);
  const childCounts = new Map<string, number>();
  for (const component of components) {
    if (component.parentId) childCounts.set(component.parentId, (childCounts.get(component.parentId) ?? 0) + 1);
  }
  const typeDistribution = Object.fromEntries(
    [...new Set(components.map((component) => component.type))]
      .sort()
      .map((type) => [type, components.filter((component) => component.type === type).length])
  );
  const libraries = [...new Set(components.flatMap((component) => component.library?.name ? [component.library.name] : []))].sort();
  const quality = analyzeDesignQuality(document, pageId);
  return {
    ...page,
    componentCount: components.length,
    rootNodeCount: components.filter((component) => !component.parentId).length,
    rootNodes: components
      .filter((component) => !component.parentId)
      .map((component) => ({
        id: component.id,
        name: component.name,
        type: component.type,
        childCount: childCounts.get(component.id) ?? 0
      })),
    typeDistribution,
    libraryComponentCount: components.filter((component) => component.library).length,
    libraries,
    quality: {
      blockingIssueCodes: quality.blockingIssues.map((issue) => issue.code),
      warningCodes: quality.warnings.map((issue) => issue.code),
      score: quality.score
    }
  };
}

export function componentPageId(document: WebDesignDocument, componentId: string): string | undefined {
  const component = document.components.find((candidate) => candidate.id === componentId);
  return component ? pageIdForComponent(document, component) : undefined;
}
